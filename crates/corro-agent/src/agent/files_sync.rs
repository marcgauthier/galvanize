use std::time::Duration;

use corro_types::agent::Agent;
use galv_files::{
    airgap::download_file_from_transport, calculate_sha256, derive_file_key, EncryptedFileStore,
    sealed_file::open_file,
    FileError,
};
use rusqlite::params;
use spawn::spawn_counted;
use time::OffsetDateTime;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error, info, warn};
use tripwire::Tripwire;

pub fn spawn_airgap_file_sync(agent: Agent, mut tripwire: Tripwire) -> tokio::task::JoinHandle<()> {
    spawn_counted(async move {
        let poll_interval = agent.config().files.airgap_poll_interval_seconds.max(1);
        let mut ticker = interval(Duration::from_secs(poll_interval));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!(
            interval_secs = poll_interval,
            "starting High node air-gap file synchronization worker"
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if !agent.is_unlocked() {
                        continue;
                    }
                    if let Err(e) = sync_missing_airgap_files(&agent).await {
                        warn!("air-gap file sync cycle encountered error: {e}");
                    }
                }
                _ = &mut tripwire => {
                    info!("stopping High node air-gap file sync worker");
                    break;
                }
            }
        }
    })
}

pub async fn sync_missing_airgap_files(agent: &Agent) -> eyre::Result<usize> {
    let transport_cfg = match &agent.config().highlow.transport {
        Some(t) => t.to_galv_transport(),
        None => {
            debug!("airgap file sync skipped: no highlow.transport configured");
            return Ok(0);
        }
    };

    let key_payload = match galv_rekey_cli::get_active_key() {
        Some(k) => k,
        None => {
            debug!("airgap file sync skipped: database encryption key not active in memory");
            return Ok(0);
        }
    };

    let file_key = derive_file_key(&key_payload.key);
    let config = agent.config();
    let high = config.highlow.high.as_ref()
        .ok_or_else(|| eyre::eyre!("airgap file sync requires highlow.high recipient configuration"))?;
    let recipient_private_key = high.recipient_rsa_private_key().map_err(|e| eyre::eyre!(e))?;
    let store_path = agent.config().files_path();
    let store = EncryptedFileStore::new(store_path.as_std_path())?;

    // Find missing files in SQLite `files` table
    let missing_files: Vec<(String, String, u64, String)> = {
        let conn = agent.pool().read().await?;
        let mut stmt = conn.prepare_cached(
            "SELECT f.uuid, f.filename, f.size, f.sha256
             FROM files f
             LEFT JOIN __galv_files_local l ON f.uuid = l.uuid
             WHERE l.uuid IS NULL OR l.status != 'available'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u64,
                row.get::<_, String>(3)?,
            ))
        })?;
        rows.filter_map(Result::ok).collect()
    };

    if missing_files.is_empty() {
        return Ok(0);
    }

    info!(count = missing_files.len(), "found missing files on High node to fetch from air-gap");
    let mut synced_count = 0;

    for (uuid, filename, _size, expected_sha256) in missing_files {
        debug!(%uuid, %filename, "fetching missing file from airgap transport");
        match download_file_from_transport(&transport_cfg, &uuid)
            .and_then(|artifact| open_file(&uuid, &artifact, &recipient_private_key)) {
            Ok(payload) => {
                let actual_sha256 = calculate_sha256(&payload);
                if actual_sha256 != expected_sha256 {
                    error!(
                        %uuid,
                        expected = %expected_sha256,
                        actual = %actual_sha256,
                        "air-gap file checksum mismatch, skipping"
                    );
                    if let Ok(wconn) = agent.pool().write_priority().await {
                        let now = OffsetDateTime::now_utc()
                            .format(&time::format_description::well_known::Rfc3339)
                            .unwrap_or_default();
                        let _ = wconn.execute(
                            "INSERT INTO __galv_files_local (uuid, status, downloaded_at, error) VALUES (?1, 'failed', ?2, 'checksum mismatch') ON CONFLICT (uuid) DO UPDATE SET status = 'failed', downloaded_at = excluded.downloaded_at, error = 'checksum mismatch';",
                            params![uuid, now],
                        );
                    }
                    continue;
                }

                if let Err(e) = store.save_file(&uuid, &payload, &file_key) {
                    error!(%uuid, "failed to save decrypted airgap file to encrypted local store: {e}");
                    continue;
                }

                let now = OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default();
                if let Ok(wconn) = agent.pool().write_priority().await {
                    let _ = wconn.execute(
                        "INSERT INTO __galv_files_local (uuid, status, downloaded_at, error) VALUES (?1, 'available', ?2, NULL) ON CONFLICT (uuid) DO UPDATE SET status = 'available', downloaded_at = excluded.downloaded_at, error = NULL;",
                        params![uuid, now],
                    );
                }

                info!(%uuid, %filename, "successfully downloaded and encrypted airgap file");
                synced_count += 1;
            }
            Err(FileError::NotFound(_)) => {
                debug!(%uuid, "file payload not yet available on airgap transport");
            }
            Err(e) => {
                warn!(%uuid, "failed to download file from airgap transport: {e}");
            }
        }
    }

    Ok(synced_count)
}
