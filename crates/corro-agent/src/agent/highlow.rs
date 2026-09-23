//! Background workers for Galvanize High/Low Air-Gap Replication.

use std::{collections::HashSet, sync::Arc, time::Duration};

use super::CountedExecutor;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use corro_types::agent::Agent;
use eyre::WrapErr;
use hyper_util::rt::TokioIo;
use rusqlite::Connection;
use rustls::{
    pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer},
    RootCertStore, ServerConfig,
};
use sha2::{Digest, Sha256};
use spawn::spawn_counted;
use tokio::net::TcpListener;
use tokio::time::{interval, MissedTickBehavior};
use tokio_rustls::TlsAcceptor;
use tower::Service;
use tracing::{debug, error, info, warn};
use tripwire::Tripwire;

#[derive(Clone)]
struct ControlState {
    agent: Agent,
}

#[derive(serde::Deserialize)]
struct ReplayRequest {
    scope: galv_highlow::ReplayScope,
    since_utc: Option<String>,
}
#[derive(serde::Deserialize)]
struct ProvenanceRequest {
    records: Vec<ProvenanceRecord>,
}
#[derive(serde::Deserialize)]
struct ProvenanceRecord {
    table: String,
    primary_key: serde_json::Map<String, serde_json::Value>,
}

/// Starts the dedicated mTLS-only OVERWATCH control API. It intentionally has
/// no relationship to the normal Corrosion HTTP listener.
pub fn spawn_control_api(
    agent: Agent,
    listener: TcpListener,
    mut tripwire: Tripwire,
) -> eyre::Result<tokio::task::JoinHandle<()>> {
    let control = agent
        .config()
        .highlow
        .control_api
        .clone()
        .ok_or_else(|| eyre::eyre!("missing highlow control API configuration"))?;
    let acceptor = TlsAcceptor::from(Arc::new(control_tls_config(&control)?));
    let state = ControlState { agent };
    let app = Router::new()
        .route("/v1/highlow/status", get(control_status))
        .route("/v1/highlow/replay", post(control_replay))
        .route("/v1/highlow/replay/status", get(control_replay_status))
        .route("/v1/highlow/provenance", post(control_provenance))
        .with_state(state);
    let addr = listener.local_addr()?;
    Ok(spawn_counted(async move {
        info!(%addr, "starting High/Low mTLS control API");
        loop {
            let (stream, _) = tokio::select! { result = listener.accept() => match result { Ok(v) => v, Err(e) => { warn!("High/Low control listener failed: {e}"); break; } }, _ = &mut tripwire => break };
            let acceptor = acceptor.clone();
            let app = app.clone();
            tokio::spawn(async move {
                let Ok(stream) = acceptor.accept(stream).await else {
                    return;
                }; // no HTTP response before mTLS authentication
                let service = hyper::service::service_fn(move |request| app.clone().call(request));
                let _ = hyper_util::server::conn::auto::Builder::new(CountedExecutor)
                    .http1_only()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    }))
}

fn control_tls_config(
    control: &corro_types::config::HighLowControlApiConfig,
) -> eyre::Result<ServerConfig> {
    let read = |name: &str| {
        std::env::var(name)
            .wrap_err_with(|| format!("required High/Low TLS environment variable {name} is unset"))
    };
    let cert_pem = read(&control.server_cert_env)?;
    let key_pem = read(&control.server_key_env)?;
    let ca_pem = read(&control.client_ca_cert_env)?;
    let certs: Vec<CertificateDer<'static>> =
        CertificateDer::pem_slice_iter(cert_pem.as_bytes()).collect::<Result<_, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes())?;
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(ca_pem.as_bytes()) {
        roots.add(cert?)?;
    }
    let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
    Ok(ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)?)
}

fn role(agent: &Agent) -> &'static str {
    let h = &agent.config().highlow;
    if h.low.is_some() {
        "low"
    } else if h.high.is_some() {
        "high"
    } else {
        "high-replica"
    }
}
async fn control_status(State(state): State<ControlState>) -> Response {
    let mut body = serde_json::json!({"enabled":state.agent.config().highlow.enabled,"role":role(&state.agent),"worker_state":"running","pending_export_count":null,"latest_export_result":null,"latest_import_result":null,"active_replay_job":null});
    if let Ok(conn) = state.agent.pool().read().await {
        if let Some(low) = &state.agent.config().highlow.low {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM __galv_highlow_events WHERE stream_id=?1 AND exported_at_ms IS NULL", [&low.stream_id], |r| r.get(0)).unwrap_or(0);
            body["pending_export_count"] = serde_json::json!(count);
        }
        if let Ok(job) = galv_highlow::latest_replay_job(&conn) {
            body["active_replay_job"] = serde_json::json!(job);
        }
        body["latest_export_result"] = galv_highlow::worker_result(&conn, "export")
            .ok()
            .flatten()
            .into();
        body["latest_import_result"] = galv_highlow::worker_result(&conn, "import")
            .ok()
            .flatten()
            .into();
    }
    Json(body).into_response()
}
async fn control_replay(
    State(state): State<ControlState>,
    Json(request): Json<ReplayRequest>,
) -> Response {
    if role(&state.agent) != "low" {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"replay is LOW-only"})),
        )
            .into_response();
    }
    if matches!(request.scope, galv_highlow::ReplayScope::All) && request.since_utc.is_some()
        || matches!(request.scope, galv_highlow::ReplayScope::Since) && request.since_utc.is_none()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"scope and since_utc are inconsistent"})),
        )
            .into_response();
    }
    let stream_id = state
        .agent
        .config()
        .highlow
        .low
        .as_ref()
        .expect("role checked")
        .stream_id
        .clone();
    let result = match state.agent.pool().write_priority().await {
        Ok(mut conn) => galv_highlow::create_replay_job(
            &mut conn,
            &stream_id,
            request.scope,
            request.since_utc.as_deref(),
        ),
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error":"database is locked"})),
            )
                .into_response()
        }
    };
    match result {
        Ok(job) => {
            spawn_highlow_replay(state.agent.clone());
            (StatusCode::ACCEPTED, Json(serde_json::json!(job))).into_response()
        }
        Err(galv_highlow::Error::Configuration(s)) if s == "replay-conflict" => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error":"a replay job is already active"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}
async fn control_replay_status(State(state): State<ControlState>) -> Response {
    if role(&state.agent) != "low" {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"replay is LOW-only"})),
        )
            .into_response();
    }
    match state.agent.pool().read().await {
        Ok(conn) => {
            let logs: Vec<serde_json::Value> = conn.prepare("SELECT at_ms,level,message FROM __galv_highlow_replay_logs ORDER BY ordinal DESC LIMIT 200").and_then(|mut s| s.query_map([], |r| Ok(serde_json::json!({"at_ms":r.get::<_,i64>(0)?,"level":r.get::<_,String>(1)?,"message":r.get::<_,String>(2)?})))?.collect()).unwrap_or_default();
            Json(serde_json::json!({"job":galv_highlow::latest_replay_job(&conn).ok().flatten(),"logs":logs})).into_response()
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error":"database is locked"})),
        )
            .into_response(),
    }
}
async fn control_provenance(
    State(state): State<ControlState>,
    Json(request): Json<ProvenanceRequest>,
) -> Response {
    if role(&state.agent) == "high-replica" {
        let base = state
            .agent
            .config()
            .highlow
            .control_api
            .as_ref()
            .and_then(|v| v.authoritative_high_url.as_ref())
            .expect("validated")
            .trim_end_matches('/')
            .to_string();
        return (
            StatusCode::TEMPORARY_REDIRECT,
            [(
                axum::http::header::LOCATION,
                format!("{base}/v1/highlow/provenance"),
            )],
        )
            .into_response();
    }
    if role(&state.agent) != "high" {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"provenance is HIGH-only"})),
        )
            .into_response();
    }
    let Ok(conn) = state.agent.pool().read().await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error":"database is locked"})),
        )
            .into_response();
    };
    let mut result = Vec::new();
    for record in request.records {
        let key = match galv_highlow::canonical_primary_key(&record.primary_key) {
            Ok(v) => v,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error":e.to_string()})),
                )
                    .into_response()
            }
        };
        let row: Option<(String,i64,i64,Option<i64>,String)> = conn.query_row("SELECT stream_id,low_first_applied_at_ms,low_last_applied_at_ms,last_high_override_at_ms,high_owned_fields_json FROM __galv_highlow_provenance WHERE table_name=?1 AND primary_key_json=?2", rusqlite::params![record.table,key], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).ok();
        result.push(match row { Some((stream,first,last,override_at,fields))=>serde_json::json!({"table":record.table,"primary_key":record.primary_key,"low_origin":true,"high_owned_fields":serde_json::from_str::<serde_json::Value>(&fields).unwrap_or_default(),"stream":stream,"low_first_applied_at_ms":first,"low_last_applied_at_ms":last,"last_high_override_at_ms":override_at}), None=>serde_json::json!({"table":record.table,"primary_key":record.primary_key,"low_origin":false,"high_owned_fields":[],"stream":null,"low_first_applied_at_ms":null,"low_last_applied_at_ms":null,"last_high_override_at_ms":null}) });
    }
    Json(serde_json::json!({"records":result})).into_response()
}

/// Replay intentionally does not touch `exported_at_ms`: it packages retained
/// journal events independently of the scheduled exporter watermark.
fn spawn_highlow_replay(agent: Agent) {
    spawn_counted(async move {
        let Some(low) = agent.config().highlow.low.clone() else {
            return;
        };
        let Some(transport) = agent
            .config()
            .highlow
            .transport
            .as_ref()
            .map(|v| v.to_galv_transport())
        else {
            return;
        };
        loop {
            let (job, events, schema_hash) = match agent.pool().read().await {
                Ok(conn) => match galv_highlow::latest_replay_job(&conn) {
                    Ok(Some(job)) if job.state == "queued" || job.state == "running" => {
                        let events = galv_highlow::replay_events(&conn, &low.stream_id, &job, 1)
                            .unwrap_or_default();
                        let hash = compute_schema_hash(&conn).unwrap_or_else(|_| "0".repeat(64));
                        (job, events, hash)
                    }
                    _ => return,
                },
                Err(_) => return,
            };
            if events.is_empty() {
                if let Ok(mut conn) = agent.pool().write_priority().await {
                    if let Ok(tx) = conn.transaction() {
                        let _ = galv_highlow::update_replay_job(
                            &tx,
                            &job.job_id,
                            job.end_sequence,
                            0,
                            true,
                            None,
                        );
                        let _ = tx.commit();
                    }
                }
                return;
            }
            let event_sequence = events[0].sequence;
            let bundle = match galv_highlow::Bundle::new(events, schema_hash, true) {
                Ok(v) => v,
                Err(e) => {
                    replay_failed(&agent, &job.job_id, job.cursor_sequence, &e.to_string()).await;
                    return;
                }
            };
            let payload_name = format!(
                "replay-{}-{:016}.zstd.galvh",
                &job.job_id[..8],
                bundle.sequence_first
            );
            let rsa = match low.recipient_rsa_public_key() {
                Ok(v) => v,
                Err(e) => {
                    replay_failed(&agent, &job.job_id, job.cursor_sequence, &e).await;
                    return;
                }
            };
            let signing = match low.sender_signing_key() {
                Ok(v) => v,
                Err(e) => {
                    replay_failed(&agent, &job.job_id, job.cursor_sequence, &e).await;
                    return;
                }
            };
            let artifact = match galv_highlow::seal(
                bundle,
                &low.recipient_key_id,
                &rsa,
                &signing,
                &payload_name,
            ) {
                Ok(v) => v,
                Err(e) => {
                    replay_failed(&agent, &job.job_id, job.cursor_sequence, &e.to_string()).await;
                    return;
                }
            };
            if let Err(e) = galv_highlow::transport::publish(&transport, &artifact) {
                replay_failed(&agent, &job.job_id, job.cursor_sequence, &e.to_string()).await;
                return;
            }
            if let Ok(mut conn) = agent.pool().write_priority().await {
                if let Ok(tx) = conn.transaction() {
                    let _ = galv_highlow::update_replay_job(
                        &tx,
                        &job.job_id,
                        event_sequence,
                        1,
                        false,
                        None,
                    );
                    let _ = tx.commit();
                }
            }
        }
    });
}
pub fn resume_highlow_replay(agent: Agent) {
    spawn_highlow_replay(agent);
}
async fn replay_failed(agent: &Agent, job_id: &str, cursor: i64, error: &str) {
    if let Ok(mut conn) = agent.pool().write_priority().await {
        if let Ok(tx) = conn.transaction() {
            let _ = galv_highlow::update_replay_job(&tx, job_id, cursor, 0, false, Some(error));
            let _ = tx.commit();
        }
    }
}

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
                    Ok(mut conn) => match conn.transaction() {
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
                    },
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
                    Ok(mut conn) => match conn.transaction() {
                        Ok(tx) => {
                            let res =
                                galv_highlow::mark_uploaded(&tx, &artifacts.manifest.bundle_id);
                            if res.is_ok() {
                                _ = tx.commit();
                            }
                            res
                        }
                        Err(e) => Err(galv_highlow::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            e.to_string(),
                        ))),
                    },
                    Err(e) => Err(galv_highlow::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))),
                }
            };

            if let Err(e) = mark_res {
                error!("failed to mark highlow outbox uploaded: {e}");
            } else {
                if let Ok(conn) = pool.read().await {
                    let _ = galv_highlow::record_worker_result(
                        &conn,
                        "export",
                        "success",
                        Some(&artifacts.manifest.bundle_id),
                        Some(&low.stream_id),
                        Some(event_count as i64),
                        None,
                    );
                }
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
                        Ok(conn) => {
                            compute_schema_hash(&conn).unwrap_or_else(|_| "empty_schema".into())
                        }
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
                    Err(galv_highlow::Error::Configuration(msg))
                        if msg.contains("waiting-schema") =>
                    {
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
                    Ok(mut conn) => {
                        process_incoming_bundle(&agent, &mut conn, &bundle, &manifest_name)
                    }
                    Err(e) => Err(galv_highlow::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    ))),
                };

                match apply_res {
                    Ok(Some(stats)) => {
                        if let Ok(conn) = pool.read().await {
                            let _ = galv_highlow::record_worker_result(
                                &conn,
                                "import",
                                "success",
                                Some(&bundle.bundle_id),
                                Some(&bundle.stream_id),
                                Some((stats.upserts + stats.deletes) as i64),
                                None,
                            );
                        }
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
    let booked = agent.booked();
    let mut book_writer = bookie_write.write_tx(&booked);

    let tx = conn.transaction().map_err(|e| {
        galv_highlow::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;

    let is_new = galv_highlow::register_received(&tx, bundle)?;
    let (res, insert_info) = if is_new {
        let stats = galv_highlow::apply::apply_bundle(&tx, bundle)?;
        tx.execute(
            "UPDATE __galv_highlow_inbox SET detail = ?1 WHERE bundle_id = ?2",
            rusqlite::params![manifest_name, bundle.bundle_id],
        )
        .map_err(|e| {
            galv_highlow::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        let insert_info = corro_types::change::insert_local_changes(agent, &tx, &mut book_writer)
            .map_err(|e| {
            galv_highlow::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        (Some(stats), insert_info)
    } else {
        (None, None)
    };

    tx.commit().map_err(|e| {
        galv_highlow::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
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
