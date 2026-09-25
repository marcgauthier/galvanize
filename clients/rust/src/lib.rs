//! GALVANIZE HTTP(S) APIs and PostgreSQL-wire client.
//!
//! SQL is sent through `tokio-postgres` to the configured wire listener; this
//! crate never opens a node's SQLite database files.

use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::{header, Client as HttpClient, Method, Response, Url};
use rustls::{ClientConfig, RootCertStore};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, io::BufReader, path::Path};
use thiserror::Error;
use tokio_postgres::{Client as PgClient, Config as PgConfig, Error as PgError};

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API returned HTTP {status}: {body}")]
    Api {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("PostgreSQL connection failed: {0}")]
    Postgres(#[from] PgError),
    #[error("TLS configuration failed: {0}")]
    Tls(String),
    #[error("environment variable {0:?} is unset")]
    MissingEnv(String),
    #[error("this API endpoint is not configured")]
    NotConfigured,
    #[error("database unlock requires an HTTPS API URL")]
    UnlockRequiresHttps,
    #[error("admin command failed: {0}")]
    AdminCommand(String),
}

#[derive(Clone)]
pub struct Client {
    api_url: Url,
    admin_url: Option<Url>,
    highlow_url: Option<Url>,
    bearer_token: Option<String>,
    http: HttpClient,
}

pub struct ClientBuilder {
    api_url: String,
    admin_url: Option<String>,
    highlow_url: Option<String>,
    bearer_token: Option<String>,
    ca_pem: Option<Vec<u8>>,
    identity_pem: Option<Vec<u8>>,
}

impl Client {
    pub fn builder(api_url: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            api_url: api_url.into(),
            admin_url: None,
            highlow_url: None,
            bearer_token: None,
            ca_pem: None,
            identity_pem: None,
        }
    }

    async fn request(
        &self,
        method: Method,
        base: Option<&Url>,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, Error> {
        let base = base.ok_or(Error::NotConfigured)?;
        let mut url = base.clone();
        let prefix = url.path().trim_end_matches('/');
        url.set_path(&format!("{prefix}{path}"));
        let mut request = self.http.request(method, url);
        if base == &self.api_url
            && path != "/v1/admin/commands"
            && !path.starts_with("/v1/highlow/")
        {
            if let Some(token) = &self.bearer_token {
                request = request.bearer_auth(token);
            }
        }
        Ok(request)
    }

    async fn json<T: for<'de> Deserialize<'de>>(&self, response: Response) -> Result<T, Error> {
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Api {
                status,
                body: response.text().await.unwrap_or_default(),
            });
        }
        Ok(response.json().await?)
    }

    pub async fn query(
        &self,
        statement: &Value,
        timeout_seconds: Option<u64>,
    ) -> Result<EventStream, Error> {
        let mut request = self
            .request(Method::POST, Some(&self.api_url), "/v1/queries")
            .await?
            .json(statement);
        if let Some(timeout) = timeout_seconds {
            request = request.query(&[("timeout", timeout)]);
        }
        self.stream(request.send().await?).await
    }

    pub async fn transaction(
        &self,
        statements: &[Value],
        timeout_seconds: Option<u64>,
    ) -> Result<Value, Error> {
        let mut request = self
            .request(Method::POST, Some(&self.api_url), "/v1/transactions")
            .await?
            .json(statements);
        if let Some(timeout) = timeout_seconds {
            request = request.query(&[("timeout", timeout)]);
        }
        self.json(request.send().await?).await
    }

    pub async fn subscribe(
        &self,
        statement: &Value,
        from: Option<u64>,
        skip_rows: bool,
    ) -> Result<EventStream, Error> {
        let request = self
            .request(Method::POST, Some(&self.api_url), "/v1/subscriptions")
            .await?
            .query(&[("skip_rows", skip_rows.to_string())])
            .json(statement);
        let request = if let Some(from) = from {
            request.query(&[("from", from)])
        } else {
            request
        };
        self.stream(request.send().await?).await
    }

    pub async fn resume_subscription(
        &self,
        id: &str,
        from: Option<u64>,
        skip_rows: bool,
    ) -> Result<EventStream, Error> {
        let path = format!("/v1/subscriptions/{}", urlencoding::encode(id));
        let request = self
            .request(Method::GET, Some(&self.api_url), &path)
            .await?
            .query(&[("skip_rows", skip_rows.to_string())]);
        let request = if let Some(from) = from {
            request.query(&[("from", from)])
        } else {
            request
        };
        self.stream(request.send().await?).await
    }

    pub async fn updates(&self, table: &str) -> Result<EventStream, Error> {
        let path = format!("/v1/updates/{}", urlencoding::encode(table));
        let response = self
            .request(Method::POST, Some(&self.api_url), &path)
            .await?
            .send()
            .await?;
        self.stream(response).await
    }

    async fn stream(&self, response: Response) -> Result<EventStream, Error> {
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Api {
                status,
                body: response.text().await.unwrap_or_default(),
            });
        }
        let query_id = response
            .headers()
            .get("corro-query-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let query_hash = response
            .headers()
            .get("corro-query-hash")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        Ok(EventStream {
            body: Box::pin(response.bytes_stream()),
            buffer: Bytes::new(),
            query_id,
            query_hash,
        })
    }

    pub async fn table_stats(&self, tables: &[&str]) -> Result<Value, Error> {
        let response = self
            .request(Method::POST, Some(&self.api_url), "/v1/table_stats")
            .await?
            .json(&json!({"tables": tables}))
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn health(&self, thresholds: &[(&str, &str)]) -> Result<Value, Error> {
        let response = self
            .request(Method::GET, Some(&self.api_url), "/v1/health")
            .await?
            .query(thresholds)
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn unlock_from_env(
        &self,
        env_name: &str,
        cipher: Option<&str>,
        cipher_params: Option<&str>,
    ) -> Result<Value, Error> {
        if self.api_url.scheme() != "https" {
            return Err(Error::UnlockRequiresHttps);
        }
        let key = env::var(env_name).map_err(|_| Error::MissingEnv(env_name.to_owned()))?;
        let body = json!({"key": key, "cipher": cipher, "cipher_params": cipher_params});
        let response = self
            .request(Method::POST, Some(&self.api_url), "/v1/admin/unlock")
            .await?
            .json(&body)
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn file_stats(&self) -> Result<Value, Error> {
        self.get_json(&self.api_url, "/v1/files/stats").await
    }
    pub async fn file_metadata(&self, id: &str) -> Result<Value, Error> {
        self.get_json(
            &self.api_url,
            &format!("/v1/files/{}/metadata", urlencoding::encode(id)),
        )
        .await
    }
    pub async fn search_files(&self, name: &str, limit: u64, offset: u64) -> Result<Value, Error> {
        let response = self
            .request(Method::GET, Some(&self.api_url), "/v1/files/search")
            .await?
            .query(&[
                ("name", name),
                ("limit", &limit.to_string()),
                ("offset", &offset.to_string()),
            ])
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn delete_file(&self, id: &str) -> Result<(), Error> {
        let response = self
            .request(
                Method::DELETE,
                Some(&self.api_url),
                &format!("/v1/files/{}", urlencoding::encode(id)),
            )
            .await?
            .send()
            .await?;
        self.check_empty(response).await
    }
    pub async fn sync_files(&self) -> Result<Value, Error> {
        let response = self
            .request(Method::POST, Some(&self.api_url), "/v1/files/sync")
            .await?
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn upload_file(
        &self,
        filename: &str,
        content_type: &str,
        data: Bytes,
    ) -> Result<Value, Error> {
        let response = self
            .request(Method::POST, Some(&self.api_url), "/v1/files/upload")
            .await?
            .query(&[("filename", filename)])
            .header(header::CONTENT_TYPE, content_type)
            .body(data)
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn upload_file_with_uuid(
        &self,
        id: &str,
        filename: &str,
        content_type: &str,
        data: Bytes,
    ) -> Result<Value, Error> {
        let path = format!("/v1/files/{}", urlencoding::encode(id));
        let response = self
            .request(Method::POST, Some(&self.api_url), &path)
            .await?
            .query(&[("filename", filename), ("content_type", content_type)])
            .header(header::CONTENT_TYPE, content_type)
            .body(data)
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn get_file(&self, id: &str) -> Result<Bytes, Error> {
        let response = self
            .request(
                Method::GET,
                Some(&self.api_url),
                &format!("/v1/files/{}", urlencoding::encode(id)),
            )
            .await?
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Api {
                status,
                body: response.text().await.unwrap_or_default(),
            });
        }
        Ok(response.bytes().await?)
    }
    pub async fn peer_fetch_file(&self, id: &str) -> Result<Bytes, Error> {
        let response = self
            .request(
                Method::GET,
                Some(&self.api_url),
                &format!("/v1/files/{}/peer_fetch", urlencoding::encode(id)),
            )
            .await?
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Api {
                status,
                body: response.text().await.unwrap_or_default(),
            });
        }
        Ok(response.bytes().await?)
    }
    pub async fn highlow_status(&self) -> Result<Value, Error> {
        self.get_json(
            self.highlow_url.as_ref().ok_or(Error::NotConfigured)?,
            "/v1/highlow/status",
        )
        .await
    }
    pub async fn highlow_replay_status(&self) -> Result<Value, Error> {
        self.get_json(
            self.highlow_url.as_ref().ok_or(Error::NotConfigured)?,
            "/v1/highlow/replay/status",
        )
        .await
    }
    pub async fn replay_highlow(
        &self,
        scope: &str,
        since_utc: Option<&str>,
    ) -> Result<Value, Error> {
        let body = json!({"scope":scope,"since_utc":since_utc});
        let response = self
            .request(
                Method::POST,
                self.highlow_url.as_ref(),
                "/v1/highlow/replay",
            )
            .await?
            .json(&body)
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn highlow_provenance(&self, records: &[Value]) -> Result<Value, Error> {
        let response = self
            .request(
                Method::POST,
                self.highlow_url.as_ref(),
                "/v1/highlow/provenance",
            )
            .await?
            .json(&json!({"records":records}))
            .send()
            .await?;
        self.json(response).await
    }
    pub async fn run_admin_command(&self, command: &Value) -> Result<Value, Error> {
        let response = self
            .request(Method::POST, self.admin_url.as_ref(), "/v1/admin/commands")
            .await?
            .json(command)
            .send()
            .await?;
        let result: Value = self.json(response).await?;
        if let Some(responses) = result.get("responses").and_then(Value::as_array) {
            for item in responses {
                if let Some(e) = item.get("Error") {
                    return Err(Error::AdminCommand(
                        e.get("msg")
                            .and_then(Value::as_str)
                            .unwrap_or("command failed")
                            .to_owned(),
                    ));
                }
            }
        }
        Ok(result)
    }
    async fn get_json(&self, base: &Url, path: &str) -> Result<Value, Error> {
        let response = self
            .request(Method::GET, Some(base), path)
            .await?
            .send()
            .await?;
        self.json(response).await
    }
    async fn check_empty(&self, response: Response) -> Result<(), Error> {
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(Error::Api {
                status,
                body: response.text().await.unwrap_or_default(),
            })
        }
    }
}

impl ClientBuilder {
    pub fn admin_url(mut self, value: impl Into<String>) -> Self {
        self.admin_url = Some(value.into());
        self
    }
    pub fn highlow_url(mut self, value: impl Into<String>) -> Self {
        self.highlow_url = Some(value.into());
        self
    }
    pub fn bearer_token(mut self, value: impl Into<String>) -> Self {
        self.bearer_token = Some(value.into());
        self
    }
    pub fn ca_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.ca_pem = Some(pem.into());
        self
    }
    pub fn identity_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.identity_pem = Some(pem.into());
        self
    }
    pub fn ca_file(mut self, path: impl AsRef<Path>) -> Result<Self, Error> {
        self.ca_pem = Some(std::fs::read(path).map_err(|e| Error::Tls(e.to_string()))?);
        Ok(self)
    }
    pub fn identity_file(mut self, path: impl AsRef<Path>) -> Result<Self, Error> {
        self.identity_pem = Some(std::fs::read(path).map_err(|e| Error::Tls(e.to_string()))?);
        Ok(self)
    }
    pub fn build(self) -> Result<Client, Error> {
        let api_url = normalize_url(&self.api_url)?;
        let admin_url = self.admin_url.as_deref().map(normalize_url).transpose()?;
        let highlow_url = self.highlow_url.as_deref().map(normalize_url).transpose()?;
        let mut builder = HttpClient::builder();
        if let Some(pem) = self.ca_pem {
            builder = builder.add_root_certificate(reqwest::Certificate::from_pem(&pem)?);
        }
        if let Some(pem) = self.identity_pem {
            builder = builder.identity(reqwest::Identity::from_pem(&pem)?);
        }
        Ok(Client {
            api_url,
            admin_url,
            highlow_url,
            bearer_token: self.bearer_token,
            http: builder.build()?,
        })
    }
}

fn normalize_url(raw: &str) -> Result<Url, Error> {
    let mut url = Url::parse(raw)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(Error::Tls("expected http(s) URL with host".into()));
    }
    url.set_query(None);
    url.set_fragment(None);
    while url.path().ends_with('/') && url.path() != "/" {
        let p = url.path().trim_end_matches('/').to_owned();
        url.set_path(&p);
    }
    Ok(url)
}

/// NDJSON response stream. Lines are decoded to one JSON event each.
pub struct EventStream {
    body: std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buffer: Bytes,
    pub query_id: Option<String>,
    pub query_hash: Option<String>,
}
impl EventStream {
    pub async fn next(&mut self) -> Result<Option<Value>, Error> {
        loop {
            if let Some(i) = self.buffer.iter().position(|b| *b == b'\n') {
                let line = self.buffer.slice(..i);
                self.buffer = self.buffer.slice(i + 1..);
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                return Ok(Some(serde_json::from_slice(&line)?));
            }
            match self.body.next().await {
                Some(Ok(chunk)) => {
                    let mut bytes = self.buffer.to_vec();
                    bytes.extend_from_slice(&chunk);
                    self.buffer = Bytes::from(bytes);
                }
                Some(Err(e)) => return Err(e.into()),
                None if self.buffer.iter().all(u8::is_ascii_whitespace) => return Ok(None),
                None => {
                    let line = std::mem::take(&mut self.buffer);
                    return Ok(Some(serde_json::from_slice(&line)?));
                }
            }
        }
    }
}

/// Open a PostgreSQL-wire connection with TLS using a CA and optional client identity.
pub async fn connect_postgres(
    dsn: &str,
    ca_pem: &[u8],
    identity_pem: Option<&[u8]>,
) -> Result<(PgClient, tokio::task::JoinHandle<Result<(), PgError>>), Error> {
    let mut roots = RootCertStore::empty();
    let mut reader = BufReader::new(ca_pem);
    for cert in rustls_pemfile::certs(&mut reader) {
        roots
            .add(cert.map_err(|e| Error::Tls(e.to_string()))?)
            .map_err(|e| Error::Tls(e.to_string()))?;
    }
    let config = ClientConfig::builder().with_root_certificates(roots);
    let tls = if let Some(identity) = identity_pem {
        let mut cert_reader = BufReader::new(identity);
        let certs = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::Tls(e.to_string()))?;
        let mut key_reader = BufReader::new(identity);
        let key = rustls_pemfile::private_key(&mut key_reader)
            .map_err(|e| Error::Tls(e.to_string()))?
            .ok_or_else(|| Error::Tls("client identity has no private key".into()))?;
        config
            .with_client_auth_cert(certs, key)
            .map_err(|e| Error::Tls(e.to_string()))?
    } else {
        config.with_no_client_auth()
    };
    let connector = tokio_postgres_rustls::MakeRustlsConnect::new(tls);
    let mut config = dsn
        .parse::<PgConfig>()
        .map_err(|e| Error::Tls(e.to_string()))?;
    config.ssl_mode(tokio_postgres::config::SslMode::Require);
    let (client, connection) = config.connect(connector).await?;
    let task = tokio::spawn(connection);
    Ok((client, task))
}

/// Open a PostgreSQL-wire connection using DSN config (without custom TLS roots).
pub async fn connect_postgres_from_env(
    env_name: &str,
) -> Result<(PgClient, tokio::task::JoinHandle<Result<(), PgError>>), Error> {
    let dsn = env::var(env_name).map_err(|_| Error::MissingEnv(env_name.to_owned()))?;
    let ca_path = env::var("GALVANIZE_PG_CA_FILE")
        .map_err(|_| Error::MissingEnv("GALVANIZE_PG_CA_FILE".into()))?;
    let ca = std::fs::read(ca_path).map_err(|e| Error::Tls(e.to_string()))?;
    let identity = env::var("GALVANIZE_PG_IDENTITY_PEM_FILE")
        .ok()
        .map(std::fs::read)
        .transpose()
        .map_err(|e| Error::Tls(e.to_string()))?;
    connect_postgres(&dsn, &ca, identity.as_deref()).await
}

/// Simple JSON encodings for commands accepted by the remote admin endpoint.
pub mod admin {
    use serde_json::{json, Value};
    pub fn ping() -> Value {
        json!("Ping")
    }
    pub fn reload_schema() -> Value {
        json!("Reload")
    }
    pub fn reload_dictionaries() -> Value {
        json!("ReloadDicts")
    }
    pub fn sync_generate() -> Value {
        json!({"Sync":"Generate"})
    }
    pub fn sync_reconcile_gaps() -> Value {
        json!({"Sync":"ReconcileGaps"})
    }
    pub fn sync_check_bookie_consistency() -> Value {
        json!({"Sync":"CheckBookieConsistency"})
    }
    pub fn sync_process_buffered_changes(actor_id: &str, version: u64, chunk_size: u64) -> Value {
        json!({"Sync":{"ProcessBufferedChanges":{"actor_id":actor_id,"version":version,"chunk_size":chunk_size}}})
    }
    pub fn cluster_rejoin() -> Value {
        json!({"Cluster":"Rejoin"})
    }
    pub fn cluster_members() -> Value {
        json!({"Cluster":"Members"})
    }
    pub fn cluster_membership_states() -> Value {
        json!({"Cluster":"MembershipStates"})
    }
    pub fn cluster_set_id(id: u16) -> Value {
        json!({"Cluster":{"SetId":id}})
    }
    pub fn actor_version(actor_id: &str, version: u64) -> Value {
        json!({"Actor":{"Version":{"actor_id":actor_id,"version":version}}})
    }
    pub fn subscriptions_list() -> Value {
        json!({"Subs":"List"})
    }
    pub fn subscription_info(hash: Option<&str>, id: Option<&str>) -> Value {
        json!({"Subs":{"Info":{"hash":hash,"id":id}}})
    }
    pub fn set_log_filter(filter: &str) -> Value {
        json!({"Log":{"Set":{"filter":filter}}})
    }
    pub fn reset_log_filter() -> Value {
        json!({"Log":"Reset"})
    }
    pub fn plumtree_stats() -> Value {
        json!({"Plumtree":"Stats"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admin_command_has_expected_external_tag_shape() {
        assert_eq!(admin::ping(), json!("Ping"));
        assert_eq!(admin::cluster_set_id(7), json!({"Cluster":{"SetId":7}}));
    }
    #[test]
    fn urls_are_normalized_and_http_only_schemes_are_accepted() {
        let url = normalize_url("https://db.example/base///").unwrap();
        assert_eq!(url.as_str(), "https://db.example/base");
        assert!(normalize_url("file:///tmp/db").is_err());
    }
}
