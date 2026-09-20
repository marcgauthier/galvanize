//! Background workers for Galvanize High/Low Air-Gap Replication.

use std::{
    collections::HashSet,
    time::Duration,
};

use corro_types::agent::Agent;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use spawn::spawn_counted;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error, info, warn};
use tripwire::Tripwire;

pub fn compute_schema_hash(conn: &Connection) -> Result<String, rusqlite::Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT sql FROM sqlite_schema
         WHERE sql IS NOT NULL
           AND name NOT LIKE '__corro_%'
           AND name NOT LIKE '__galv_%'
           AND name NOT LIKE 'crsql_%'
           AND name NOT LIKE 'sqlite_%'
         ORDER BY name ASC",
    )?;

    let mut rows = stmt.query([])?;
    let mut hasher = Sha256::new();
    let mut found = false;

    while let Some(row) = rows.next()? {
        let sql: String = row.get(0)?;
        hasher.update(sql.as_bytes());
        hasher.update(b"\n");
        found = true;
    }

    if !found {
        // Fallback default schema hash if no user tables yet
        hasher.update(b"empty_schema");
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Spawns the Low-side exporter loop that packages pending events, seals them
/// into encrypted bundles, and publishes them via the configured transport.
pub fn spawn_highlow_exporter(agent: Agent, mut tripwire: Tripwire) {
    let low = match &agent.config().highlow.low {
        Some(low) if agent.config().highlow.enabled => low.clone(),
        _ => return,
    };

    let transport_config = match &agent.config().highlow.transport {
        Some(t) => t.to_galv_transport(),
        None => {
            error!("highlow low role enabled but transport is missing");
            return;
        }
    };

    spawn_counted(async move {
        info!(
            stream_id = %low.stream_id,
            interval_secs = low.upload_interval_seconds,
            "starting highlow exporter loop"
        );

        let mut timer = interval(Duration::from_secs(low.upload_interval_seconds));
        timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = &mut tripwire => {
                    info!("highlow exporter received shutdown signal");
                    break;
                }
                _ = timer.tick() => {}
            }

            let pool = agent.pool();

            let pending_res = {
                match pool.read().await {
                    Ok(conn) => {
                        let schema_hash = match compute_schema_hash(&conn) {
                            Ok(h) => h,
                            Err(e) => {
                                error!("failed to compute schema hash for highlow export: {e}");
                                continue;
                            }
                        };
                        match galv_highlow::pending_events(&conn, &low.stream_id, 1000) {
                            Ok(events) => Ok((events, schema_hash)),
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => {
                        error!("could not acquire read conn for highlow export: {e}");
                        continue;
                    }
                }
            };

            let (events, schema_hash) = match pending_res {
                Ok((events, hash)) => {
                    if events.is_empty() {
                        continue;
                    }
                    (events, hash)
                }
                Err(e) => {
                    error!("failed to query pending highlow events: {e}");
                    continue;
                }
            };

            let event_count = events.len();
            debug!(count = event_count, "packaging highlow events into bundle");

            let bundle = match galv_highlow::Bundle::new(events, schema_hash, false) {
                Ok(b) => b,
                Err(e) => {
                    error!("failed to create highlow bundle: {e}");
                    continue;
                }
            };

            let rsa_pub = match low.recipient_rsa_public_key() {
                Ok(k) => k,
                Err(e) => {
                    error!("highlow configuration error: {e}");
                    continue;
                }
            };

            let sender_signing_key = match low.sender_signing_key() {
                Ok(k) => k,
                Err(e) => {
                    error!("highlow configuration error: {e}");
                    continue;
                }
            };

            let payload_filename = format!(
                "{}-{:016}-{}.zstd.galvh",
                low.stream_id,
                bundle.sequence_last,
                &bundle.bundle_id[..bundle.bundle_id.len().min(8)]
            );

            let manifest_filename = match galv_highlow::manifest_filename(&payload_filename) {
                Ok(m) => m,
                Err(e) => {
                    error!("failed to format manifest filename: {e}");
                    continue;
                }
            };

            let artifacts = match galv_highlow::seal(
                bundle,
                &low.recipient_key_id,
                &rsa_pub,
                &sender_signing_key,
                &payload_filename,
            ) {
                Ok(a) => a,
                Err(e) => {
                    error!("failed to seal highlow bundle: {e}");
                    continue;
                }
            };

            // 1. Record outbox
            let outbox_res = {
                match pool.write_priority().await {
                    Ok(mut conn) => {
                        match conn.transaction() {
                            Ok(tx) => {
                                let res = galv_highlow::record_outbox(
                                    &tx,
                                    &artifacts,
                                    &payload_filename,
                                    &manifest_filename,
                                );
                                if res.is_ok() {
                                    _ = tx.commit();
                                }
                                res
                            }
                            Err(e) => Err(galv_highlow::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                e.to_string(),
                            ))),
                        }
                    }
                    Err(e) => Err(galv_highlow::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))),
                }
            };

            if let Err(e) = outbox_res {
                error!("failed to record highlow outbox: {e}");
                continue;
            }

            // 2. Publish to transport
            if let Err(e) = galv_highlow::transport::publish(&transport_config, &artifacts) {
                error!("failed to publish highlow bundle via transport: {e}");
                continue;
            }

            // 3. Mark uploaded
            let mark_res = {
                match pool.write_priority().await {
                    Ok(mut conn) => {
                        match conn.transaction() {
                            Ok(tx) => {
                                let res = galv_highlow::mark_uploaded(
                                    &tx,
                                    &artifacts.manifest.bundle_id,
                                );
                                if res.is_ok() {
                                    _ = tx.commit();
                                }
                                res
                            }
                            Err(e) => Err(galv_highlow::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                e.to_string(),
                            ))),
                        }
                    }
                    Err(e) => Err(galv_highlow::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))),
                }
            };

            if let Err(e) = mark_res {
                error!("failed to mark highlow outbox uploaded: {e}");
            } else {
                info!(
                    bundle_id = %artifacts.manifest.bundle_id,
                    stream = %low.stream_id,
                    count = event_count,
                    "successfully published and marked uploaded highlow bundle"
                );
            }
        }
    });
}

/// Spawns the High-side receiver loop that polls the transport source for manifests,
/// downloads matching payloads, authenticates & decrypts bundles, records inbox state,
/// and applies changes to the database.
pub fn spawn_highlow_receiver(agent: Agent, mut tripwire: Tripwire) {
    let high = match &agent.config().highlow.high {
        Some(high) if agent.config().highlow.enabled => high.clone(),
        _ => return,
    };

    let transport_config = match &agent.config().highlow.transport {
        Some(t) => t.to_galv_transport(),
        None => {
            error!("highlow high role enabled but transport is missing");
            return;
        }
    };

    let accepted_streams_set: HashSet<String> = high.accepted_streams.iter().cloned().collect();

    spawn_counted(async move {
        info!(
            interval_secs = high.download_interval_seconds,
            "starting highlow receiver loop"
        );

        let mut timer = interval(Duration::from_secs(high.download_interval_seconds));
        timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = &mut tripwire => {
                    info!("highlow receiver received shutdown signal");
                    break;
                }
                _ = timer.tick() => {}
            }

            let manifests = match galv_highlow::transport::list_manifests(&transport_config) {
                Ok(m) => m,
                Err(e) => {
                    debug!("failed to list highlow manifests: {e}");
                    continue;
                }
            };

            if manifests.is_empty() {
                continue;
            }

            let pool = agent.pool();

            let rsa_priv = match high.recipient_rsa_private_key() {
                Ok(k) => k,
                Err(e) => {
                    error!("highlow configuration error: {e}");
                    continue;
                }
            };

            let permitted_senders = match high.permitted_sender_keys() {
                Ok(s) => s,
                Err(e) => {
                    error!("highlow configuration error: {e}");
                    continue;
                }
            };

            for manifest_name in manifests {
                // Check if already in inbox
                let already_processed = {
                    match pool.read().await {
                        Ok(conn) => {
                            let found: Option<i64> = conn
                                .query_row(
                                    "SELECT 1 FROM __galv_highlow_inbox WHERE detail = ?1 LIMIT 1",
                                    [&manifest_name],
                                    |r| r.get(0),
                                )
                                .ok();
                            found.is_some()
                        }
                        Err(_) => false,
                    }
                };

                if already_processed {
                    continue;
                }

                // Fetch pair
                let (manifest_bytes, payload_bytes) = match galv_highlow::transport::fetch_pair(
                    &transport_config,
                    &manifest_name,
                ) {
                    Ok(pair) => pair,
                    Err(e) => {
                        error!(manifest = %manifest_name, "failed to fetch highlow bundle pair: {e}");
                        continue;
                    }
                };

                let schema_hash = {
                    match pool.read().await {
                        Ok(conn) => compute_schema_hash(&conn).unwrap_or_else(|_| "empty_schema".into()),
                        Err(_) => "empty_schema".into(),
                    }
                };

                // Decrypt and open bundle
                let bundle = match galv_highlow::open(
                    &manifest_bytes,
                    &payload_bytes,
                    &rsa_priv,
                    &permitted_senders,
                    &schema_hash,
                ) {
                    Ok(b) => b,
                    Err(galv_highlow::Error::Configuration(msg)) if msg.contains("waiting-schema") => {
                        warn!(manifest = %manifest_name, "highlow bundle held: waiting-schema");
                        continue;
                    }
                    Err(e) => {
                        error!(manifest = %manifest_name, "failed to open highlow bundle: {e}");
                        continue;
                    }
                };

                if !accepted_streams_set.contains(&bundle.stream_id) {
                    debug!(
                        stream = %bundle.stream_id,
                        manifest = %manifest_name,
                        "skipping highlow bundle from non-accepted stream"
                    );
                    continue;
                }

                // In a write transaction: register received and apply events
                let apply_res = match pool.write_priority().await {
                    Ok(mut conn) => process_incoming_bundle(&agent, &mut conn, &bundle, &manifest_name),
                    Err(e) => Err(galv_highlow::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))),
                };

                match apply_res {
                    Ok(Some(stats)) => {
                        info!(
                            bundle_id = %bundle.bundle_id,
                            stream = %bundle.stream_id,
                            upserts = stats.upserts,
                            deletes = stats.deletes,
                            "successfully ingested and applied highlow bundle"
                        );
                    }
                    Ok(None) => {
                        debug!(bundle_id = %bundle.bundle_id, "highlow bundle was already registered");
                    }
                    Err(e) => {
                        error!(bundle_id = %bundle.bundle_id, "failed to apply highlow bundle: {e}");
                    }
                }
            }
        }
    });
}

fn process_incoming_bundle(
    agent: &Agent,
    conn: &mut Connection,
    bundle: &galv_highlow::Bundle,
    manifest_name: &str,
) -> Result<Option<galv_highlow::apply::ApplyStats>, galv_highlow::Error> {
    let bookie_write = agent.bookie().write_lock_blocking();
    let mut book_writer = bookie_write.write_tx(agent.booked());

    let tx = conn.transaction().map_err(|e| {
        galv_highlow::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    })?;

    let is_new = galv_highlow::register_received(&tx, bundle)?;
    let (res, insert_info) = if is_new {
        let stats = galv_highlow::apply::apply_bundle(&tx, bundle)?;
        tx.execute(
            "UPDATE __galv_highlow_inbox SET detail = ?1 WHERE bundle_id = ?2",
            rusqlite::params![manifest_name, bundle.bundle_id],
        )
        .map_err(|e| {
            galv_highlow::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        })?;

        let insert_info = corro_types::change::insert_local_changes(agent, &tx, &mut book_writer)
            .map_err(|e| {
                galv_highlow::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            })?;

        (Some(stats), insert_info)
    } else {
        (None, None)
    };

    tx.commit().map_err(|e| {
        galv_highlow::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    })?;

    if let Some(insert_info) = insert_info {
        book_writer.commit();
        let agent = agent.clone();
        spawn_counted(async move {
            let _ = corro_types::broadcast::broadcast_changes(
                agent,
                insert_info.db_version,
                insert_info.last_seq,
                insert_info.ts,
            )
            .await;
        });
    }

    Ok(res)
}
