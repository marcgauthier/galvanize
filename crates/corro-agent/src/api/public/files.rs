use std::net::SocketAddr;

use axum::{
    body::Bytes,
    extract::{Path, Query},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Extension, Json,
};
use corro_types::agent::{Agent, ChangeError};
use galv_files::{
    airgap::publish_file_to_transport, calculate_sha256, derive_file_key, validate_file_uuid,
    sealed_file::seal_file,
    EncryptedFileStore, FileRecord, FileSearchQuery, FileStorageStats,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use tracing::{error, info, warn};

use super::make_broadcastable_changes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResponse {
    pub status: String,
    pub uuid: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub status: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadataResponse {
    pub uuid: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub content_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Option<serde_json::Value>,
    pub is_local: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UploadQueryParams {
    pub uuid: Option<String>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
}

fn get_active_store_and_key(agent: &Agent) -> Result<(EncryptedFileStore, [u8; 32]), (StatusCode, Json<ErrorResponse>)> {
    if !agent.is_unlocked() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                status: "error".into(),
                error: "database is locked / awaiting unlock".into(),
            }),
        ));
    }

    let key_payload = galv_rekey_cli::get_active_key().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                status: "error".into(),
                error: "database encryption key not active in memory".into(),
            }),
        )
    })?;

    let file_key = derive_file_key(&key_payload.key);
    let store_path = agent.config().files_path();
    let store = EncryptedFileStore::new(store_path.as_std_path()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("could not initialize file store: {e}"),
            }),
        )
    })?;

    Ok((store, file_key))
}

/// POST /v1/files/upload or POST /v1/files/{uuid}
pub async fn api_v1_files_upload(
    Extension(agent): Extension<Agent>,
    path_uuid: Option<Path<String>>,
    Query(query): Query<UploadQueryParams>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<UploadResponse>), (StatusCode, Json<ErrorResponse>)> {
    let conf = agent.config().files.clone();
    if !conf.accept_uploads {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                status: "error".into(),
                error: "file uploads are disabled on this node".into(),
            }),
        ));
    }

    let (store, file_key) = get_active_store_and_key(&agent)?;

    let raw_uuid = path_uuid
        .map(|p| p.0)
        .or(query.uuid)
        .or_else(|| {
            headers
                .get("x-file-uuid")
                .and_then(|v| v.to_str().ok())
                .map(String::from)
        })
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if let Err(e) = validate_file_uuid(&raw_uuid) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: "error".into(),
                error: e.to_string(),
            }),
        ));
    }

    if let Some(limit) = conf.max_file_size_bytes {
        if body.len() as u64 > limit {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ErrorResponse {
                    status: "error".into(),
                    error: format!("payload size {} exceeds limit of {} bytes", body.len(), limit),
                }),
            ));
        }
    }

    let filename = query
        .filename
        .or_else(|| {
            headers
                .get("x-filename")
                .and_then(|v| v.to_str().ok())
                .map(String::from)
        })
        .unwrap_or_else(|| format!("{raw_uuid}.bin"));

    let content_type = query.content_type.or_else(|| {
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    });

    let extra_metadata = headers
        .get("x-metadata")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    let sha256 = calculate_sha256(&body);
    let size = body.len() as u64;
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();

    // 1. Save encrypted file to disk
    if let Err(e) = store.save_file(&raw_uuid, &body, &file_key) {
        error!("failed to write encrypted file {raw_uuid}: {e}");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("failed to store file: {e}"),
            }),
        ));
    }

    // 2. Insert metadata into SQLite `files` table and local status
    let uuid_for_db = raw_uuid.clone();
    let filename_for_db = filename.clone();
    let content_type_for_db = content_type.clone();
    let sha256_for_db = sha256.clone();
    let now_for_db = now.clone();
    let meta_json = extra_metadata.as_ref().map(|m| m.to_string());

    let db_res = make_broadcastable_changes(&agent, Some(30), move |tx| {
        tx.execute(
            r#"
            INSERT INTO files (uuid, filename, size, sha256, content_type, created_at, updated_at, metadata)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT (uuid) DO UPDATE SET
                filename = excluded.filename,
                size = excluded.size,
                sha256 = excluded.sha256,
                content_type = excluded.content_type,
                updated_at = excluded.updated_at,
                metadata = excluded.metadata;
            "#,
            params![
                uuid_for_db,
                filename_for_db,
                size as i64,
                sha256_for_db,
                content_type_for_db,
                now_for_db,
                now_for_db,
                meta_json,
            ],
        )
        .map_err(|e| ChangeError::Rusqlite {
            source: e,
            actor_id: None,
            version: None,
        })?;

        // Update local bookkeeping
        tx.execute(
            r#"
            INSERT INTO __galv_files_local (uuid, status, downloaded_at, error)
            VALUES (?1, 'available', ?2, NULL)
            ON CONFLICT (uuid) DO UPDATE SET
                status = 'available',
                downloaded_at = excluded.downloaded_at,
                error = NULL;
            "#,
            params![uuid_for_db, now_for_db],
        )
        .map_err(|e| ChangeError::Rusqlite {
            source: e,
            actor_id: None,
            version: None,
        })?;

        Ok(())
    })
    .await;

    if let Err(e) = db_res {
        error!("failed to commit file metadata transaction: {e}");
        let _ = store.delete_file(&raw_uuid);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("database error: {e}"),
            }),
        ));
    }

    // 3. Seal for the High recipient before publishing to air-gap transport.
    if conf.push_to_high {
        if let Some(transport_cfg) = &agent.config().highlow.transport {
            if let Some(low) = &agent.config().highlow.low {
                match low.recipient_rsa_public_key()
                    .map_err(|e| e.to_string())
                    .and_then(|pem| seal_file(&raw_uuid, &body, &pem).map_err(|e| e.to_string()))
                {
                    Ok(sealed) => {
                        let galv_transport = transport_cfg.to_galv_transport();
                        if let Err(e) = publish_file_to_transport(&galv_transport, &raw_uuid, &sealed) {
                            error!("failed to push encrypted file {raw_uuid} to airgap transport: {e}");
                        } else {
                            info!(uuid = %raw_uuid, "successfully published encrypted file to air-gap transport");
                        }
                    }
                    Err(e) => error!("failed to encrypt file {raw_uuid} for High recipient: {e}"),
                }
            } else {
                warn!("push_to_high is enabled but highlow.low is not configured");
            }
        } else {
            warn!("push_to_high is enabled but highlow.transport is not configured");
        }
    }

    Ok((
        StatusCode::OK,
        Json(UploadResponse {
            status: "ok".into(),
            uuid: raw_uuid,
            filename,
            size,
            sha256,
            content_type,
            created_at: now.clone(),
            updated_at: now,
        }),
    ))
}

/// GET /v1/files/{uuid}
pub async fn api_v1_files_get(
    Extension(agent): Extension<Agent>,
    Path(uuid): Path<String>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = validate_file_uuid(&uuid) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: "error".into(),
                error: e.to_string(),
            }),
        ));
    }

    let (store, file_key) = get_active_store_and_key(&agent)?;

    // Look up metadata in SQLite `files` table
    let conn = agent.pool().read().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("database pool error: {e}"),
            }),
        )
    })?;

    let record: Option<FileRecord> = conn
        .query_row(
            "SELECT uuid, filename, size, sha256, content_type, created_at, updated_at, metadata FROM files WHERE uuid = ?",
            [&uuid],
            |row| {
                let meta_str: Option<String> = row.get(7)?;
                let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
                Ok(FileRecord {
                    uuid: row.get(0)?,
                    filename: row.get(1)?,
                    size: row.get::<_, i64>(2)? as u64,
                    sha256: row.get(3)?,
                    content_type: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    metadata,
                })
            },
        )
        .optional()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: "error".into(),
                    error: e.to_string(),
                }),
            )
        })?;

    drop(conn);

    let record = match record {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: "error".into(),
                    error: format!("file '{uuid}' not found"),
                }),
            ));
        }
    };

    // Check if payload exists locally
    let payload = if store.has_file(&uuid) {
        store.read_file(&uuid, &file_key).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: "error".into(),
                    error: format!("failed to read encrypted file: {e}"),
                }),
            )
        })?
    } else if agent.config().files.accept_from_peers {
        // Attempt peer fetch
        match fetch_file_from_peers(&agent, &uuid, &record.sha256).await {
            Ok(data) => {
                let _ = store.save_file(&uuid, &data, &file_key);
                let now = OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default();
                if let Ok(wconn) = agent.pool().write_priority().await {
                    let _ = wconn.execute(
                        "INSERT INTO __galv_files_local (uuid, status, downloaded_at, error) VALUES (?1, 'available', ?2, NULL) ON CONFLICT (uuid) DO UPDATE SET status = 'available', downloaded_at = excluded.downloaded_at, error = NULL;",
                        params![uuid, now],
                    );
                }
                data
            }
            Err(e) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        status: "error".into(),
                        error: format!("file payload not available locally and peer fetch failed: {e}"),
                    }),
                ));
            }
        }
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: "error".into(),
                error: "file payload not available locally".into(),
            }),
        ));
    };

    let mime = record
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let disposition = format!("attachment; filename=\"{}\"", record.filename);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, payload.len().to_string())
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::ETAG, format!("\"{}\"", record.sha256))
        .body(axum::body::Body::from(payload))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: "error".into(),
                    error: format!("response build failed: {e}"),
                }),
            )
        })?;

    Ok(response)
}

/// GET /v1/files/{uuid}/peer_fetch - used by cluster peers to fetch decrypted payload
pub async fn api_v1_files_peer_fetch(
    Extension(agent): Extension<Agent>,
    Path(uuid): Path<String>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = validate_file_uuid(&uuid) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: "error".into(),
                error: e.to_string(),
            }),
        ));
    }

    let (store, file_key) = get_active_store_and_key(&agent)?;
    if !store.has_file(&uuid) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: "error".into(),
                error: "file not found locally".into(),
            }),
        ));
    }

    let payload = store.read_file(&uuid, &file_key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("failed to read file: {e}"),
            }),
        )
    })?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, payload.len().to_string())
        .body(axum::body::Body::from(payload))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: "error".into(),
                    error: format!("response build failed: {e}"),
                }),
            )
        })?;

    Ok(response)
}

/// Helper function to fetch missing file payload from cluster peers over HTTP API
async fn fetch_file_from_peers(agent: &Agent, uuid: &str, expected_sha256: &str) -> eyre::Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let member_addrs: Vec<SocketAddr> = {
        let members = agent.members().read();
        members.by_addr.keys().copied().collect()
    };

    for addr in member_addrs {
        let mut candidate_ports = vec![addr.port()];
        if addr.port() > 1000 {
            candidate_ports.push(addr.port() - 1000);
        }
        if addr.port() > 4100 {
            candidate_ports.push(addr.port() - 4100);
        }
        for port in candidate_ports {
            let mut api_addr = addr;
            api_addr.set_port(port);
            let url = format!("http://{api_addr}/v1/files/{uuid}/peer_fetch");
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() {
                    if let Ok(bytes) = resp.bytes().await {
                        let actual_sha256 = calculate_sha256(&bytes);
                        if actual_sha256 == expected_sha256 {
                            return Ok(bytes.to_vec());
                        }
                    }
                }
            }
        }
    }
    Err(eyre::eyre!("could not retrieve file {uuid} from any active peer"))
}

/// GET /v1/files/{uuid}/metadata
pub async fn api_v1_files_metadata(
    Extension(agent): Extension<Agent>,
    Path(uuid): Path<String>,
) -> Result<Json<FileMetadataResponse>, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = validate_file_uuid(&uuid) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: "error".into(),
                error: e.to_string(),
            }),
        ));
    }

    if !agent.is_unlocked() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                status: "error".into(),
                error: "database is locked / awaiting unlock".into(),
            }),
        ));
    }

    let conn = agent.pool().read().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("database error: {e}"),
            }),
        )
    })?;

    let record: Option<FileRecord> = conn
        .query_row(
            "SELECT uuid, filename, size, sha256, content_type, created_at, updated_at, metadata FROM files WHERE uuid = ?",
            [&uuid],
            |row| {
                let meta_str: Option<String> = row.get(7)?;
                let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
                Ok(FileRecord {
                    uuid: row.get(0)?,
                    filename: row.get(1)?,
                    size: row.get::<_, i64>(2)? as u64,
                    sha256: row.get(3)?,
                    content_type: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    metadata,
                })
            },
        )
        .optional()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: "error".into(),
                    error: e.to_string(),
                }),
            )
        })?;

    let record = record.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("file '{uuid}' not found"),
            }),
        )
    })?;

    let is_local = if let Ok(store) = EncryptedFileStore::new(agent.config().files_path().as_std_path()) {
        store.has_file(&uuid)
    } else {
        false
    };

    Ok(Json(FileMetadataResponse {
        uuid: record.uuid,
        filename: record.filename,
        size: record.size,
        sha256: record.sha256,
        content_type: record.content_type,
        created_at: record.created_at,
        updated_at: record.updated_at,
        metadata: record.metadata,
        is_local,
    }))
}

/// GET /v1/files/search or GET /v1/files
pub async fn api_v1_files_search(
    Extension(agent): Extension<Agent>,
    Query(query): Query<FileSearchQuery>,
) -> Result<Json<Vec<FileRecord>>, (StatusCode, Json<ErrorResponse>)> {
    if !agent.is_unlocked() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                status: "error".into(),
                error: "database is locked / awaiting unlock".into(),
            }),
        ));
    }

    let conn = agent.pool().read().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("database pool error: {e}"),
            }),
        )
    })?;

    let limit = query.limit.unwrap_or(100).min(1000) as i64;
    let offset = query.offset.unwrap_or(0) as i64;
    let name_filter = query.name.clone();

    let records = match name_filter {
        Some(filter) if !filter.is_empty() => {
            let pattern = format!("%{filter}%");
            let mut stmt = conn
                .prepare_cached(
                    "SELECT uuid, filename, size, sha256, content_type, created_at, updated_at, metadata FROM files WHERE filename LIKE ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
                )
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: "error".into(),
                            error: e.to_string(),
                        }),
                    )
                })?;
            let rows = stmt
                .query_map(params![pattern, limit, offset], |row| {
                    let meta_str: Option<String> = row.get(7)?;
                    let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
                    Ok(FileRecord {
                        uuid: row.get(0)?,
                        filename: row.get(1)?,
                        size: row.get::<_, i64>(2)? as u64,
                        sha256: row.get(3)?,
                        content_type: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                        metadata,
                    })
                })
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: "error".into(),
                            error: e.to_string(),
                        }),
                    )
                })?;
            rows.filter_map(Result::ok).collect::<Vec<_>>()
        }
        _ => {
            let mut stmt = conn
                .prepare_cached(
                    "SELECT uuid, filename, size, sha256, content_type, created_at, updated_at, metadata FROM files ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
                )
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: "error".into(),
                            error: e.to_string(),
                        }),
                    )
                })?;
            let rows = stmt
                .query_map(params![limit, offset], |row| {
                    let meta_str: Option<String> = row.get(7)?;
                    let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
                    Ok(FileRecord {
                        uuid: row.get(0)?,
                        filename: row.get(1)?,
                        size: row.get::<_, i64>(2)? as u64,
                        sha256: row.get(3)?,
                        content_type: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                        metadata,
                    })
                })
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            status: "error".into(),
                            error: e.to_string(),
                        }),
                    )
                })?;
            rows.filter_map(Result::ok).collect::<Vec<_>>()
        }
    };

    Ok(Json(records))
}

/// DELETE /v1/files/{uuid}
pub async fn api_v1_files_delete(
    Extension(agent): Extension<Agent>,
    Path(uuid): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if let Err(e) = validate_file_uuid(&uuid) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: "error".into(),
                error: e.to_string(),
            }),
        ));
    }

    if !agent.is_unlocked() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                status: "error".into(),
                error: "database is locked / awaiting unlock".into(),
            }),
        ));
    }

    let uuid_for_db = uuid.clone();
    let res = make_broadcastable_changes(&agent, Some(30), move |tx| {
        tx.execute("DELETE FROM files WHERE uuid = ?", params![uuid_for_db])
            .map_err(|e| ChangeError::Rusqlite {
                source: e,
                actor_id: None,
                version: None,
            })?;
        tx.execute(
            "DELETE FROM __galv_files_local WHERE uuid = ?",
            params![uuid_for_db],
        )
        .map_err(|e| ChangeError::Rusqlite {
            source: e,
            actor_id: None,
            version: None,
        })?;
        Ok(())
    })
    .await;

    if let Err(e) = res {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("database error on delete: {e}"),
            }),
        ));
    }

    if let Ok(store) = EncryptedFileStore::new(agent.config().files_path().as_std_path()) {
        let _ = store.delete_file(&uuid);
    }

    Ok(Json(json!({
        "status": "ok",
        "deleted": uuid,
    })))
}

/// GET /v1/files/stats
pub async fn api_v1_files_stats(
    Extension(agent): Extension<Agent>,
) -> Result<Json<FileStorageStats>, (StatusCode, Json<ErrorResponse>)> {
    if !agent.is_unlocked() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                status: "error".into(),
                error: "database is locked / awaiting unlock".into(),
            }),
        ));
    }

    let conn = agent.pool().read().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("database pool error: {e}"),
            }),
        )
    })?;

    let (total_files, total_bytes): (i64, i64) = conn
        .query_row(
            "SELECT COALESCE(COUNT(*), 0), COALESCE(SUM(size), 0) FROM files",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or((0, 0));

    let (local_cached_files, local_cached_bytes): (i64, i64) = conn
        .query_row(
            "SELECT COALESCE(COUNT(f.uuid), 0), COALESCE(SUM(f.size), 0) FROM files f INNER JOIN __galv_files_local l ON f.uuid = l.uuid WHERE l.status = 'available'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or((0, 0));

    let missing = (total_files - local_cached_files).max(0) as usize;

    Ok(Json(FileStorageStats {
        total_files: total_files as usize,
        total_bytes: total_bytes as u64,
        local_cached_files: local_cached_files as usize,
        local_cached_bytes: local_cached_bytes as u64,
        missing_local_files: missing,
    }))
}

/// POST /v1/files/sync - manually triggers one air-gap sync cycle
pub async fn api_v1_files_sync(
    Extension(agent): Extension<Agent>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if !agent.is_unlocked() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                status: "error".into(),
                error: "database is locked / awaiting unlock".into(),
            }),
        ));
    }

    match crate::agent::files_sync::sync_missing_airgap_files(&agent).await {
        Ok(synced) => Ok(Json(json!({
            "status": "ok",
            "synced_count": synced,
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: "error".into(),
                error: format!("air-gap sync failed: {e}"),
            }),
        )),
    }
}
