use std::collections::HashMap;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};

use crate::actor::MemberId;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use serde_with::{formats::PreferOne, serde_as, OneOrMany};

pub const DEFAULT_GOSSIP_PORT: u16 = 4001;
const DEFAULT_GOSSIP_IDLE_TIMEOUT: u32 = 30;

#[cfg(test)]
pub const DEFAULT_MAX_SYNC_BACKOFF: u32 = 2;
#[cfg(not(test))]
pub const DEFAULT_MAX_SYNC_BACKOFF: u32 = 15;

const fn default_apply_batch_min() -> usize {
    100
}

const fn default_apply_batch_step() -> usize {
    500
}

const fn default_apply_batch_max() -> usize {
    16_000
}

const fn default_batch_threshold_ratio() -> f64 {
    0.9
}

pub const DEFAULT_CACHE_SIZE_KIB: i64 = -1048576; // 1 GB (negative value means KiB)
pub const DEFAULT_MMAP_SIZE_BYTES: i64 = 8589934592; // 8 GB
pub const DEFAULT_JOURNAL_SIZE_LIMIT_BYTES: i64 = 1073741824; // 1 GB

const fn default_cache_size_kib() -> i64 {
    DEFAULT_CACHE_SIZE_KIB
}

const fn default_mmap_size_bytes() -> i64 {
    DEFAULT_MMAP_SIZE_BYTES
}

const fn default_journal_size_limit_bytes() -> i64 {
    DEFAULT_JOURNAL_SIZE_LIMIT_BYTES
}

const fn default_reaper_interval() -> usize {
    3600
}

const fn default_reaper_limit() -> usize {
    500
}

const fn default_wal_threshold() -> usize {
    // Default of 5GB
    5 * 1024
}

const fn default_processing_queue() -> usize {
    20000
}

const fn default_plumtree_send_queue() -> usize {
    30000
}

/// Used for the apply channel
const fn default_huge_channel() -> usize {
    2048
}

//
const fn default_big_channel() -> usize {
    1024
}

const fn default_mid_channel() -> usize {
    512
}

const fn default_small_channel() -> usize {
    256
}

const fn default_apply_timeout() -> usize {
    10
}

fn default_sql_tx_timeout() -> usize {
    60
}

fn default_min_sync_backoff() -> u32 {
    1
}

fn default_max_sync_backoff() -> u32 {
    DEFAULT_MAX_SYNC_BACKOFF
}

fn default_partial_retry_backoff() -> u32 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub db: DbConfig,
    pub api: ApiConfig,
    pub gossip: GossipConfig,

    #[serde(default)]
    pub perf: PerfConfig,

    #[serde(default)]
    pub admin: AdminConfig,

    #[serde(default)]
    pub telemetry: TelemetryConfig,

    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub consul: Option<ConsulConfig>,
    #[serde(default)]
    pub reaper: Option<ReaperConfig>,
    /// Optional one-way Low -> High air-gap replication. Disabled by default.
    #[serde(default)]
    pub highlow: HighLowConfig,
    /// Optional file upload, storage, and airgap replication.
    #[serde(default)]
    pub files: FilesConfig,
}

const fn default_airgap_poll_interval() -> u64 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct FilesConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub accept_uploads: bool,
    #[serde(default)]
    pub accept_from_peers: bool,
    #[serde(default)]
    pub push_to_high: bool,
    #[serde(default)]
    pub sync_airgap_files: bool,
    pub storage_path: Option<Utf8PathBuf>,
    pub max_file_size_bytes: Option<u64>,
    #[serde(default = "default_airgap_poll_interval")]
    pub airgap_poll_interval_seconds: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HighLowConfig {
    #[serde(default)]
    pub enabled: bool,
    pub transport: Option<HighLowTransportConfig>,
    pub low: Option<HighLowLowConfig>,
    pub high: Option<HighLowHighConfig>,
    pub high_replica: Option<HighLowHighReplicaConfig>,
    #[serde(default)]
    pub control_api: Option<HighLowControlApiConfig>,
}

/// Dedicated HTTPS API for High/Low control operations. TLS material is read
/// from named environment variables so it never appears in configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HighLowControlApiConfig {
    pub addr: SocketAddr,
    pub server_cert_env: String,
    pub server_key_env: String,
    pub client_ca_cert_env: String,
    pub authoritative_high_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HighLowTransportConfig {
    pub kind: HighLowTransportKind,
    pub endpoint: String,
    pub username: Option<String>,
    pub password_env: Option<String>,
    pub private_key_env: Option<String>,
    pub private_key_passphrase_env: Option<String>,
    pub host_key_sha256: Option<String>,
    pub ca_file: Option<Utf8PathBuf>,
    pub bearer_token_env: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HighLowTransportKind {
    Directory,
    Ftp,
    Ftps,
    Sftp,
    Smb,
    Http,
    Https,
}

impl HighLowTransportConfig {
    pub fn to_galv_transport(&self) -> galv_highlow::transport::TransportConfig {
        let kind = match self.kind {
            HighLowTransportKind::Directory => galv_highlow::transport::TransportKind::Directory,
            HighLowTransportKind::Ftp => galv_highlow::transport::TransportKind::Ftp,
            HighLowTransportKind::Ftps => galv_highlow::transport::TransportKind::Ftps,
            HighLowTransportKind::Sftp => galv_highlow::transport::TransportKind::Sftp,
            HighLowTransportKind::Smb => galv_highlow::transport::TransportKind::Directory,
            HighLowTransportKind::Http => galv_highlow::transport::TransportKind::Http,
            HighLowTransportKind::Https => galv_highlow::transport::TransportKind::Https,
        };
        let password = self
            .password_env
            .as_deref()
            .and_then(|env| std::env::var(env).ok());
        let bearer_token = self
            .bearer_token_env
            .as_deref()
            .and_then(|env| std::env::var(env).ok());
        galv_highlow::transport::TransportConfig {
            kind,
            endpoint: self.endpoint.clone(),
            username: self.username.clone(),
            password,
            bearer_token,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HighLowLowConfig {
    pub stream_id: String,
    pub network_name: String,
    pub upload_interval_seconds: u64,
    pub recipient_key_id: String,
    pub recipient_rsa_public_key_env: String,
    pub sender_signing_key_env: String,
}

impl HighLowLowConfig {
    pub fn recipient_rsa_public_key(&self) -> Result<String, String> {
        std::env::var(&self.recipient_rsa_public_key_env).map_err(|_| {
            format!(
                "environment variable {} for recipient RSA public key not found",
                self.recipient_rsa_public_key_env
            )
        })
    }

    pub fn sender_signing_key(&self) -> Result<String, String> {
        std::env::var(&self.sender_signing_key_env).map_err(|_| {
            format!(
                "environment variable {} for sender Ed25519 signing key not found",
                self.sender_signing_key_env
            )
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HighLowHighConfig {
    pub accepted_streams: Vec<String>,
    pub download_interval_seconds: u64,
    pub recipient_key_id: String,
    pub recipient_rsa_private_key_env: String,
    pub permitted_sender_key_envs: Vec<String>,
}

impl HighLowHighConfig {
    pub fn recipient_rsa_private_key(&self) -> Result<String, String> {
        std::env::var(&self.recipient_rsa_private_key_env).map_err(|_| {
            format!(
                "environment variable {} for recipient RSA private key not found",
                self.recipient_rsa_private_key_env
            )
        })
    }

    pub fn permitted_sender_keys(&self) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        for env_name in &self.permitted_sender_key_envs {
            let key = std::env::var(env_name).map_err(|_| {
                format!("environment variable {env_name} for permitted sender key not found")
            })?;
            keys.push(key);
        }
        Ok(keys)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HighLowHighReplicaConfig {
    pub accepted_streams: Vec<String>,
}

impl HighLowConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }

        let roles = self.low.is_some() as u8
            + self.high.is_some() as u8
            + self.high_replica.is_some() as u8;
        if roles != 1 {
            return Err(ConfigError::HighLow(
                "exactly one of low, high, or high-replica must be configured".into(),
            ));
        }

        if self.high_replica.is_none() && self.transport.is_none() {
            return Err(ConfigError::HighLow(
                "low and high roles require a transport".into(),
            ));
        }
        if let Some(transport) = &self.transport {
            if transport.endpoint.is_empty() {
                return Err(ConfigError::HighLow(
                    "transport.endpoint must not be empty".into(),
                ));
            }
            if transport.endpoint.contains('@') {
                return Err(ConfigError::HighLow(
                    "credentials must not be embedded in transport.endpoint".into(),
                ));
            }
            if transport.kind == HighLowTransportKind::Sftp
                && transport
                    .host_key_sha256
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .is_none()
            {
                return Err(ConfigError::HighLow(
                    "sftp requires transport.host-key-sha256 pinning".into(),
                ));
            }
        }
        if let Some(low) = &self.low {
            if low.stream_id.is_empty()
                || low.network_name.is_empty()
                || low.recipient_key_id.is_empty()
                || low.recipient_rsa_public_key_env.is_empty()
                || low.sender_signing_key_env.is_empty()
            {
                return Err(ConfigError::HighLow(
                    "low role has a required empty field".into(),
                ));
            }
            if low.upload_interval_seconds == 0 {
                return Err(ConfigError::HighLow(
                    "low.upload-interval-seconds must be positive".into(),
                ));
            }
        }
        if let Some(high) = &self.high {
            if high.accepted_streams.is_empty()
                || high.recipient_key_id.is_empty()
                || high.recipient_rsa_private_key_env.is_empty()
                || high.permitted_sender_key_envs.is_empty()
            {
                return Err(ConfigError::HighLow(
                    "high role has a required empty field".into(),
                ));
            }
            if high.download_interval_seconds == 0 {
                return Err(ConfigError::HighLow(
                    "high.download-interval-seconds must be positive".into(),
                ));
            }
        }
        if let Some(replica) = &self.high_replica {
            if replica.accepted_streams.is_empty() {
                return Err(ConfigError::HighLow(
                    "high-replica.accepted-streams must not be empty".into(),
                ));
            }
        }
        if let Some(control) = &self.control_api {
            if control.server_cert_env.is_empty()
                || control.server_key_env.is_empty()
                || control.client_ca_cert_env.is_empty()
            {
                return Err(ConfigError::HighLow(
                    "control-api TLS environment variable names must not be empty".into(),
                ));
            }
            if self.high_replica.is_some()
                && control
                    .authoritative_high_url
                    .as_deref()
                    .filter(|url| url.starts_with("https://"))
                    .is_none()
            {
                return Err(ConfigError::HighLow(
                    "high-replica control-api requires an https authoritative-high-url".into(),
                ));
            }
            if let Some(url) = &control.authoritative_high_url {
                if !url.starts_with("https://") {
                    return Err(ConfigError::HighLow(
                        "control-api authoritative-high-url must use https".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TelemetryConfig {
    pub prometheus: Option<PrometheusConfig>,
    pub open_telemetry: Option<OtelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    #[serde(alias = "addr")]
    pub bind_addr: SocketAddr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OtelConfig {
    FromEnv,
    Exporter { endpoint: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    #[serde(alias = "path")]
    pub uds_path: Utf8PathBuf,
    /// Optional remote operator API. Private TLS material stays in environment variables.
    #[serde(default)]
    pub control_api: Option<AdminControlApiConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AdminControlApiConfig {
    pub addr: SocketAddr,
    pub server_cert_env: String,
    pub server_key_env: String,
    pub client_ca_cert_env: String,
}

impl AdminControlApiConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.server_cert_env.is_empty()
            || self.server_key_env.is_empty()
            || self.client_ca_cert_env.is_empty()
        {
            return Err(ConfigError::AdminControlApi(
                "control-api TLS environment variable names must not be empty".into(),
            ));
        }
        Ok(())
    }
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            uds_path: default_admin_path(),
            control_api: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbConfig {
    pub path: Utf8PathBuf,
    #[serde(default)]
    pub schema_paths: Vec<Utf8PathBuf>,
    #[serde(default)]
    pub subscriptions_path: Option<Utf8PathBuf>,
    /// Whether the database must wait for a remote unlock API call (POST /v1/admin/unlock)
    /// before initializing the database connection pool, migrations, and background services.
    /// Default: false (unless existing DB file is encrypted).
    #[serde(default, alias = "await-unlock", alias = "encrypted")]
    pub await_unlock: bool,
    /// SQLite page cache size in KiB for writes (negative value).
    /// Default: -1048576 (1 GB). Larger values improve write performance but use more RAM.
    /// WARNING: Setting this too low (<100MB) can severely degrade performance.
    #[serde(default = "default_cache_size_kib")]
    pub cache_size_kib: i64,
    /// SQLite memory-mapped I/O limit in bytes (PRAGMA mmap_size).
    /// Default: 8589934592 (8 GB). Set to 0 to disable memory mapping.
    #[serde(default = "default_mmap_size_bytes")]
    pub mmap_size_bytes: i64,
    /// SQLite WAL journal size limit in bytes (PRAGMA journal_size_limit).
    /// Default: 1073741824 (1 GB).
    #[serde(default = "default_journal_size_limit_bytes")]
    pub journal_size_limit_bytes: i64,
}

impl DbConfig {
    pub fn subscriptions_path(&self) -> Utf8PathBuf {
        self.subscriptions_path
            .as_ref()
            .cloned()
            .unwrap_or_else(|| {
                self.path
                    .parent()
                    .map(|parent| parent.join("subscriptions"))
                    .unwrap_or_else(|| "/subscriptions".into())
            })
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(alias = "addr")]
    #[serde_as(deserialize_as = "OneOrMany<_, PreferOne>")]
    pub bind_addr: Vec<SocketAddr>,
    #[serde(default)]
    pub endpoint_name: Option<String>,
    #[serde(alias = "authz", default)]
    pub authorization: Option<AuthzConfig>,
    #[serde_as(deserialize_as = "Option<OneOrMany<_, PreferOne>>")]
    #[serde(default)]
    pub pg: Option<Vec<PgConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgConfig {
    #[serde(alias = "addr")]
    pub bind_addr: SocketAddr,
    pub tls: Option<PgTlsConfig>,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgTlsConfig {
    pub cert_file: Utf8PathBuf,
    pub key_file: Utf8PathBuf,
    #[serde(default)]
    pub ca_file: Option<Utf8PathBuf>,
    #[serde(default)]
    pub verify_client: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthzConfig {
    #[serde(alias = "bearer")]
    BearerToken(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BroadcastMethod {
    Gossip,
    Plumtree,
}

fn default_broadcast_config() -> BroadcastConfig {
    BroadcastConfig::Gossip
}

pub fn default_plumtree_prune_threshold() -> u32 {
    5
}

pub fn default_plumtree_prune_throttle_secs() -> Option<u64> {
    Some(1)
}

pub fn default_plumtree_ring_locked_radius() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlumtreeConfig {
    #[serde(default = "default_plumtree_prune_threshold")]
    pub prune_threshold: u32,
    #[serde(default)]
    pub optimization_threshold: Option<u32>,
    #[serde(default)]
    pub batch_gossip: bool,
    /// Suppress repeat PRUNEs to the same peer within this window. `None` disables.
    #[serde(default = "default_plumtree_prune_throttle_secs")]
    pub prune_throttle_secs: Option<u64>,
    /// Near/Mid/Far split when selecting eager and lazy peers.
    #[serde(default)]
    pub eager_ratios: plum_foca::EagerRatios,
    /// Neighbors locked eager on each side of the identity ring.
    /// Total locked peers is `2 * radius` (default 1 → 2).
    #[serde(default = "default_plumtree_ring_locked_radius")]
    pub ring_locked_radius: usize,
}

impl Default for PlumtreeConfig {
    fn default() -> Self {
        Self {
            prune_threshold: default_plumtree_prune_threshold(),
            optimization_threshold: None,
            batch_gossip: false,
            prune_throttle_secs: default_plumtree_prune_throttle_secs(),
            eager_ratios: plum_foca::EagerRatios::default(),
            ring_locked_radius: default_plumtree_ring_locked_radius(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BroadcastConfig {
    Gossip,
    Plumtree(PlumtreeConfig),
}

impl Default for BroadcastConfig {
    fn default() -> Self {
        Self::Gossip
    }
}

impl BroadcastConfig {
    pub fn method(&self) -> BroadcastMethod {
        match self {
            Self::Gossip => BroadcastMethod::Gossip,
            Self::Plumtree(_) => BroadcastMethod::Plumtree,
        }
    }

    pub fn plumtree(&self) -> Option<&PlumtreeConfig> {
        match self {
            Self::Plumtree(cfg) => Some(cfg),
            Self::Gossip => None,
        }
    }

    pub fn from_method(method: BroadcastMethod) -> Self {
        match method {
            BroadcastMethod::Gossip => Self::Gossip,
            BroadcastMethod::Plumtree => Self::Plumtree(PlumtreeConfig::default()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    #[serde(alias = "addr")]
    pub bind_addr: SocketAddr,
    pub external_addr: Option<SocketAddr>,
    /// Bind address for outgoing QUIC. When unset, and neither family-specific
    /// bind is set, defaults to `[::]:0`. Mutually exclusive with
    /// `client_addr_v4` / `client_addr_v6`.
    #[serde(default)]
    pub client_addr: Option<SocketAddr>,
    /// IPv4 bind for outgoing QUIC. Mutually exclusive with `client_addr`.
    #[serde(default)]
    pub client_addr_v4: Option<SocketAddr>,
    /// IPv6 bind for outgoing QUIC. Mutually exclusive with `client_addr`.
    #[serde(default)]
    pub client_addr_v6: Option<SocketAddr>,
    #[serde(default)]
    pub bootstrap: Vec<String>,
    /// When true, bootstrap from both IPv4 and IPv6 peers regardless of this
    /// node's gossip bind family. The other family is only reachable if a
    /// matching `client_addr_v4` / `client_addr_v6` socket is bound or `client_addr`
    /// is set to `[::]:0`.
    #[serde(default)]
    pub allow_mixed_ip: bool,
    #[serde(default)]
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub plaintext: bool,
    #[serde(default)]
    pub max_mtu: Option<u16>,
    #[serde(default = "default_gossip_idle_timeout")]
    pub idle_timeout_secs: u32,
    #[serde(default)]
    pub disable_gso: bool,
    #[serde(default)]
    pub member_id: Option<MemberId>,
    #[serde(default)]
    pub compression: Option<CompressionConfig>,
    #[serde(default = "default_broadcast_config")]
    pub broadcast: BroadcastConfig,
    /// Allowed peers IP/CIDR/host filter. Defaults to ["*"] (all peers allowed).
    #[serde(
        default = "default_allow_list",
        alias = "allow_list",
        alias = "allow-list"
    )]
    pub allow_list: AllowList,
}

/// Filter rule for allowed peers in the gossip network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowRule {
    Wildcard,
    Ip(std::net::IpAddr),
    Network(ipnet::IpNet),
}

/// List of allowed peer IPs, CIDR networks, or wildcards (`*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub struct AllowList {
    raw: Vec<String>,
    #[serde(skip)]
    rules: Vec<AllowRule>,
}

impl Default for AllowList {
    fn default() -> Self {
        Self {
            raw: vec!["*".to_string()],
            rules: vec![AllowRule::Wildcard],
        }
    }
}

impl AllowList {
    pub fn new(rules: Vec<AllowRule>) -> Self {
        Self {
            raw: rules
                .iter()
                .map(|r| match r {
                    AllowRule::Wildcard => "*".to_string(),
                    AllowRule::Ip(ip) => ip.to_string(),
                    AllowRule::Network(net) => net.to_string(),
                })
                .collect(),
            rules,
        }
    }

    pub fn is_allowed(&self, addr: &SocketAddr) -> bool {
        self.is_allowed_ip(&addr.ip())
    }

    pub fn is_allowed_ip(&self, ip: &std::net::IpAddr) -> bool {
        if self.rules.is_empty() {
            return true;
        }
        for rule in &self.rules {
            match rule {
                AllowRule::Wildcard => return true,
                AllowRule::Ip(allowed_ip) => {
                    if allowed_ip == ip {
                        return true;
                    }
                }
                AllowRule::Network(net) => {
                    if net.contains(ip) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn raw(&self) -> &[String] {
        &self.raw
    }
}

impl TryFrom<Vec<String>> for AllowList {
    type Error = String;

    fn try_from(raw: Vec<String>) -> Result<Self, Self::Error> {
        let mut rules = Vec::new();
        for item in &raw {
            let trimmed = item.trim();
            if trimmed == "*" {
                rules.push(AllowRule::Wildcard);
            } else if let Ok(net) = trimmed.parse::<ipnet::IpNet>() {
                rules.push(AllowRule::Network(net));
            } else if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
                rules.push(AllowRule::Ip(ip));
            } else if let Ok(sock) = trimmed.parse::<SocketAddr>() {
                rules.push(AllowRule::Ip(sock.ip()));
            } else {
                return Err(format!("invalid allow-list entry '{item}': expected '*', IP, CIDR (e.g. 10.0.0.0/24), or socket address"));
            }
        }
        if rules.is_empty() {
            rules.push(AllowRule::Wildcard);
        }
        Ok(Self { raw, rules })
    }
}

impl From<AllowList> for Vec<String> {
    fn from(list: AllowList) -> Self {
        list.raw
    }
}

pub fn default_allow_list() -> AllowList {
    AllowList::default()
}

impl GossipConfig {
    pub fn compression_config(&self) -> CompressionConfig {
        self.compression.clone().unwrap_or_default()
    }

    pub fn broadcast_method(&self) -> BroadcastMethod {
        self.broadcast.method()
    }

    pub fn plumtree(&self) -> Option<&PlumtreeConfig> {
        self.broadcast.plumtree()
    }

    /// Resolve outgoing QUIC binds.
    ///
    /// `client_addr` is the legacy single-socket mode. `client_addr_v4` /
    /// `client_addr_v6` opt into per-family sockets. The two styles cannot
    /// be combined. When nothing is set, binds `[::]:0`.
    pub fn client_binds(&self) -> Result<ClientBinds, ClientBindsError> {
        match (self.client_addr, self.client_addr_v4, self.client_addr_v6) {
            (None, None, None) => Ok(ClientBinds::Single(DEFAULT_GOSSIP_CLIENT_ADDR)),
            (Some(addr), None, None) => Ok(ClientBinds::Single(addr)),
            (None, v4, v6) if v4.is_some() || v6.is_some() => {
                if let Some(addr) = v4 {
                    if !addr.is_ipv4() {
                        return Err(ClientBindsError::FamilyMismatch {
                            field: "client_addr_v4",
                            addr,
                            expected: "IPv4",
                        });
                    }
                }
                if let Some(addr) = v6 {
                    if !addr.is_ipv6() {
                        return Err(ClientBindsError::FamilyMismatch {
                            field: "client_addr_v6",
                            addr,
                            expected: "IPv6",
                        });
                    }
                }
                Ok(ClientBinds::ByFamily { v4, v6 })
            }
            _ => Err(ClientBindsError::MixedModes),
        }
    }
}

/// Outgoing QUIC client binds, after resolving `client_addr` vs family fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientBinds {
    /// One outgoing bind. `[::]:0` may still dual-stack map IPv4 destinations.
    Single(SocketAddr),
    /// Separate outgoing binds selected by destination address family.
    ByFamily {
        v4: Option<SocketAddr>,
        v6: Option<SocketAddr>,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ClientBindsError {
    #[error(
        "gossip.client_addr is mutually exclusive with gossip.client_addr_v4 and gossip.client_addr_v6"
    )]
    MixedModes,
    #[error("gossip.{field} must be an {expected} address, got {addr}")]
    FamilyMismatch {
        field: &'static str,
        addr: SocketAddr,
        expected: &'static str,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Whether to zstd-compress broadcast and sync changeset payloads.
    /// Safe to flip cluster-wide at any time: uncompressed and compressed
    /// nodes interoperate
    #[serde(default)]
    pub enabled: bool,
    /// zstd compression level for wire payloads (1–22). Ignored when
    /// `enabled` is false.
    #[serde(default = "default_compression_level")]
    pub level: i32,
    /// Directory of trained zstd dictionaries. Every valid dictionary file in this directory
    /// is loaded at startup (and on `corrosion reload-dicts`) and indexed by its
    /// embedded zstd dictionary id for decoding, so broadcasts from peers still
    /// on an older (or newer) encoding dictionary can be understood during a
    /// gradual rollout.
    #[serde(default)]
    pub dict_dir: Option<Utf8PathBuf>,
    /// Filename within `dict_dir` used to compress outgoing broadcasts.
    /// Omit to compress without a dictionary while still decoding
    /// dictionary-compressed peers.
    #[serde(default)]
    pub dict_file: Option<String>,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: default_compression_level(),
            dict_dir: None,
            dict_file: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfConfig {
    #[serde(default = "default_huge_channel")]
    pub apply_channel_len: usize,
    #[serde(default = "default_big_channel")]
    pub changes_channel_len: usize,
    #[serde(default = "default_big_channel")]
    pub empties_channel_len: usize,
    #[serde(default = "default_mid_channel")]
    pub to_send_channel_len: usize,
    #[serde(default = "default_mid_channel")]
    pub notifications_channel_len: usize,
    #[serde(default = "default_mid_channel")]
    pub schedule_channel_len: usize,
    #[serde(default = "default_mid_channel")]
    pub clearbuf_channel_len: usize,
    #[serde(default = "default_mid_channel")]
    pub bcast_channel_len: usize,
    #[serde(default = "default_small_channel")]
    pub foca_channel_len: usize,
    #[serde(default = "default_wal_threshold")]
    pub wal_threshold_mb: usize,
    #[serde(default = "default_sql_tx_timeout")]
    pub sql_tx_timeout: usize,
    #[serde(default = "default_min_sync_backoff")]
    pub min_sync_backoff: u32,
    #[serde(default = "default_max_sync_backoff")]
    pub max_sync_backoff: u32,
    #[serde(default = "default_partial_retry_backoff")]
    pub partial_retry_backoff: u32,
    // How many unapplied changesets corrosion will buffer before starting to drop them
    #[serde(default = "default_processing_queue")]
    pub processing_queue_len: usize,
    // How many outgoing plumtree messages to buffer before shedding.
    // Kept separate from processing_queue_len so the send queue can be sized
    // without also growing the plumtree payload and seen caches.
    #[serde(default = "default_plumtree_send_queue")]
    pub plumtree_send_queue_len: usize,
    // How many ms corrosion will wait before proceeding to apply a batch of changes
    // We wait either for apply_queue_timeout or untill at least apply_queue_min_batch_size changes accumulate
    #[serde(default = "default_apply_timeout")]
    pub apply_queue_timeout: usize,
    // Minimum amount of changes corrosion will try to apply at once in the same transaction
    #[serde(default = "default_apply_batch_min")]
    pub apply_queue_min_batch_size: usize,
    // batch_size = clamp(min_batch_size, step_base * 2 ** floor(log2(x/step_base)), max_batch_size)
    #[serde(default = "default_apply_batch_step")]
    pub apply_queue_step_base: usize,
    // Maximum amount of changes corrosion will try to apply at once in the same transaction
    #[serde(default = "default_apply_batch_max")]
    pub apply_queue_max_batch_size: usize,
    // Threshold ratio (0.0-1.0) for immediate batch spawning when queue reaches this fraction of batch size
    // It's used to decide whether to wait for more changes for apply_queue_timeout ms or spawn a batch immediately
    #[serde(default = "default_batch_threshold_ratio")]
    pub apply_queue_batch_threshold_ratio: f64,
}

impl Default for PerfConfig {
    fn default() -> Self {
        Self {
            apply_channel_len: default_huge_channel(),
            changes_channel_len: default_big_channel(),
            empties_channel_len: default_big_channel(),
            to_send_channel_len: default_mid_channel(),
            notifications_channel_len: default_mid_channel(),
            schedule_channel_len: default_mid_channel(),
            clearbuf_channel_len: default_mid_channel(),
            bcast_channel_len: default_mid_channel(),
            foca_channel_len: default_small_channel(),
            wal_threshold_mb: default_wal_threshold(),
            sql_tx_timeout: default_sql_tx_timeout(),
            min_sync_backoff: default_min_sync_backoff(),
            max_sync_backoff: default_max_sync_backoff(),
            partial_retry_backoff: default_partial_retry_backoff(),
            processing_queue_len: default_processing_queue(),
            plumtree_send_queue_len: default_plumtree_send_queue(),
            apply_queue_timeout: default_apply_timeout(),
            apply_queue_min_batch_size: default_apply_batch_min(),
            apply_queue_step_base: default_apply_batch_step(),
            apply_queue_max_batch_size: default_apply_batch_max(),
            apply_queue_batch_threshold_ratio: default_batch_threshold_ratio(),
        }
    }
}

fn default_gossip_idle_timeout() -> u32 {
    DEFAULT_GOSSIP_IDLE_TIMEOUT
}

pub const DEFAULT_GOSSIP_CLIENT_ADDR: SocketAddr =
    SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0u16, 0, 0));

fn default_compression_level() -> i32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Certificate file
    pub cert_file: Utf8PathBuf,
    /// Private key file
    pub key_file: Utf8PathBuf,

    /// CA (Certificate Authority) file
    #[serde(default)]
    pub ca_file: Option<Utf8PathBuf>,

    #[serde(default)]
    pub insecure: bool,

    /// Mutual TLS configuration
    #[serde(default)]
    pub client: Option<TlsClientConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsClientConfig {
    /// Certificate file
    pub cert_file: Utf8PathBuf,
    /// Private key file
    pub key_file: Utf8PathBuf,
}

pub fn default_admin_path() -> Utf8PathBuf {
    "/var/run/corrosion/admin.sock".into()
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogConfig {
    #[serde(default)]
    pub format: LogFormat,
    #[serde(default = "default_as_true")]
    pub colors: bool,
}

fn default_as_true() -> bool {
    true
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error("gossip.max_mtu value {value} is below the QUIC minimum of 1200 (RFC 9000)")]
    InvalidMaxMtu { value: u16 },
    #[error("gossip.compression_level value {value} is outside the supported zstd range 1..=22")]
    InvalidCompressionLevel { value: i32 },
    #[error("gossip.compression.dict_file requires gossip.compression.dict_dir to be set")]
    DictFileWithoutDictDir,
    #[error("{0}")]
    InvalidEagerRatios(#[from] plum_foca::EagerRatiosError),
    #[error(transparent)]
    ClientBinds(#[from] ClientBindsError),
    #[error("invalid highlow configuration: {0}")]
    HighLow(String),
    #[error("invalid admin control API configuration: {0}")]
    AdminControlApi(String),
}

impl Config {
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    pub fn files_path(&self) -> Utf8PathBuf {
        if let Some(p) = &self.files.storage_path {
            p.clone()
        } else if let Some(parent) = self.db.path.parent() {
            parent.join("files")
        } else {
            Utf8PathBuf::from("files")
        }
    }

    /// Reads configuration from a TOML file, given its path. Environment
    /// variables can override whatever is set in the config file.
    pub fn load(config_path: &str) -> Result<Self, ConfigError> {
        let config = config::Config::builder()
            .add_source(config::File::new(config_path, config::FileFormat::Toml))
            .add_source(config::Environment::default().separator("__"))
            .build()?;
        let cfg: Self = config.try_deserialize()?;
        cfg.validate()?;
        Ok(cfg)
    }
    fn validate(&self) -> Result<(), ConfigError> {
        if let Some(mtu) = self.gossip.max_mtu {
            if mtu < 1200 {
                return Err(ConfigError::InvalidMaxMtu { value: mtu });
            }
        }
        let compression = self.gossip.compression_config();
        if !(1..=22).contains(&compression.level) {
            return Err(ConfigError::InvalidCompressionLevel {
                value: compression.level,
            });
        }
        if compression.dict_file.is_some() && compression.dict_dir.is_none() {
            return Err(ConfigError::DictFileWithoutDictDir);
        }
        if let Some(plumtree) = self.gossip.plumtree() {
            plumtree.eager_ratios.validate()?;
        }
        self.gossip.client_binds()?;
        self.highlow.validate()?;
        if let Some(control_api) = &self.admin.control_api {
            control_api.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ConfigBuilder {
    pub db_path: Option<Utf8PathBuf>,
    gossip_addr: Option<SocketAddr>,
    api_addr: Vec<SocketAddr>,
    endpoint_name: Option<String>,
    external_addr: Option<SocketAddr>,
    admin_path: Option<Utf8PathBuf>,
    prometheus_addr: Option<SocketAddr>,
    bootstrap: Option<Vec<String>>,
    log: Option<LogConfig>,
    schema_paths: Vec<Utf8PathBuf>,
    cache_size_kib: Option<i64>,
    mmap_size_bytes: Option<i64>,
    journal_size_limit_bytes: Option<i64>,
    max_change_size: Option<i64>,
    consul: Option<ConsulConfig>,
    reaper: Option<ReaperConfig>,
    tls: Option<TlsConfig>,
    perf: Option<PerfConfig>,
    member_id: Option<MemberId>,
    max_mtu: Option<u16>,
    disable_gso: bool,
    compression: Option<bool>,
    compression_level: Option<i32>,
    broadcast: Option<BroadcastConfig>,
    allow_list: Option<AllowList>,
    await_unlock: Option<bool>,
}

impl ConfigBuilder {
    pub fn allow_list(mut self, allow_list: AllowList) -> Self {
        self.allow_list = Some(allow_list);
        self
    }
    pub fn db_path<S: Into<Utf8PathBuf>>(mut self, db_path: S) -> Self {
        self.db_path = Some(db_path.into());
        self
    }

    pub fn gossip_addr(mut self, addr: SocketAddr) -> Self {
        self.gossip_addr = Some(addr);
        self
    }

    pub fn api_addr(mut self, addr: SocketAddr) -> Self {
        self.api_addr.push(addr);
        self
    }

    pub fn endpoint_name<S: Into<String>>(mut self, endpoint_name: S) -> Self {
        self.endpoint_name = Some(endpoint_name.into());
        self
    }

    pub fn prometheus_addr(mut self, addr: SocketAddr) -> Self {
        self.prometheus_addr = Some(addr);
        self
    }

    pub fn bootstrap<V: Into<Vec<String>>>(mut self, bootstrap: V) -> Self {
        self.bootstrap = Some(bootstrap.into());
        self
    }

    pub fn log(mut self, log: LogConfig) -> Self {
        self.log = Some(log);
        self
    }

    pub fn add_schema_path<S: Into<Utf8PathBuf>>(mut self, path: S) -> Self {
        self.schema_paths.push(path.into());
        self
    }

    pub fn admin_path<S: Into<Utf8PathBuf>>(mut self, path: S) -> Self {
        self.admin_path = Some(path.into());
        self
    }

    pub fn max_change_size(mut self, size: i64) -> Self {
        self.max_change_size = Some(size);
        self
    }

    pub fn consul(mut self, config: ConsulConfig) -> Self {
        self.consul = Some(config);
        self
    }

    pub fn reaper(mut self, config: ReaperConfig) -> Self {
        self.reaper = Some(config);
        self
    }

    pub fn tls_config(mut self, config: TlsConfig) -> Self {
        self.tls = Some(config);
        self
    }

    pub fn member_id(mut self, member_id: MemberId) -> Self {
        self.member_id = Some(member_id);
        self
    }

    /// Set the maximum MTU for the QUIC gossip transport.
    pub fn max_mtu(mut self, mtu: u16) -> Self {
        self.max_mtu = Some(mtu);
        self
    }

    pub fn broadcast_method(mut self, method: BroadcastMethod) -> Self {
        self.broadcast = Some(BroadcastConfig::from_method(method));
        self
    }

    pub fn broadcast(mut self, broadcast: BroadcastConfig) -> Self {
        self.broadcast = Some(broadcast);
        self
    }

    pub fn plumtree(mut self, plumtree: PlumtreeConfig) -> Self {
        self.broadcast = Some(BroadcastConfig::Plumtree(plumtree));
        self
    }

    /// Disable Generic Segmentation Offload (GSO) for the QUIC gossip transport.
    pub fn disable_gso(mut self, disable: bool) -> Self {
        self.disable_gso = disable;
        self
    }

    /// Enable or disable zstd compression of broadcast and sync changeset payloads.
    pub fn compression(mut self, enabled: bool) -> Self {
        self.compression = Some(enabled);
        self
    }

    /// Set the zstd compression level (1–22) for wire payloads.
    pub fn compression_level(mut self, level: i32) -> Self {
        self.compression_level = Some(level);
        self
    }

    /// Set the SQLite write page cache size in KiB.
    pub fn cache_size_kib(mut self, size: i64) -> Self {
        self.cache_size_kib = Some(size);
        self
    }

    /// Set the SQLite memory-mapped I/O limit in bytes (PRAGMA mmap_size).
    pub fn mmap_size_bytes(mut self, bytes: i64) -> Self {
        self.mmap_size_bytes = Some(bytes);
        self
    }

    /// Set the SQLite WAL journal size limit in bytes (PRAGMA journal_size_limit).
    pub fn journal_size_limit_bytes(mut self, bytes: i64) -> Self {
        self.journal_size_limit_bytes = Some(bytes);
        self
    }

    /// Set whether the database awaits remote unlock via HTTP API.
    pub fn await_unlock(mut self, await_unlock: bool) -> Self {
        self.await_unlock = Some(await_unlock);
        self
    }

    pub fn build(self) -> Result<Config, ConfigBuilderError> {
        let db_path = self.db_path.ok_or(ConfigBuilderError::DbPathRequired)?;

        let telemetry = TelemetryConfig {
            prometheus: self
                .prometheus_addr
                .map(|bind_addr| PrometheusConfig { bind_addr }),
            open_telemetry: None,
        };

        if self.api_addr.is_empty() {
            return Err(ConfigBuilderError::ApiAddrRequired);
        }

        Ok(Config {
            db: DbConfig {
                path: db_path,
                schema_paths: self.schema_paths,
                subscriptions_path: None,
                await_unlock: self.await_unlock.unwrap_or(false),
                cache_size_kib: self.cache_size_kib.unwrap_or_else(default_cache_size_kib),
                mmap_size_bytes: self.mmap_size_bytes.unwrap_or_else(default_mmap_size_bytes),
                journal_size_limit_bytes: self
                    .journal_size_limit_bytes
                    .unwrap_or_else(default_journal_size_limit_bytes),
            },
            api: ApiConfig {
                bind_addr: self.api_addr,
                endpoint_name: self.endpoint_name,
                authorization: None,
                pg: None,
            },
            gossip: GossipConfig {
                bind_addr: self
                    .gossip_addr
                    .ok_or(ConfigBuilderError::GossipAddrRequired)?,
                external_addr: self.external_addr,
                client_addr: None,
                client_addr_v4: None,
                client_addr_v6: None,
                bootstrap: self.bootstrap.unwrap_or_default(),
                allow_mixed_ip: false,
                plaintext: self.tls.is_none(),
                tls: self.tls,
                idle_timeout_secs: default_gossip_idle_timeout(),
                max_mtu: self.max_mtu,
                disable_gso: self.disable_gso,
                member_id: self.member_id,
                compression: Some(CompressionConfig {
                    enabled: self.compression.unwrap_or_default(),
                    level: self
                        .compression_level
                        .unwrap_or_else(default_compression_level),
                    dict_dir: None,
                    dict_file: None,
                }),
                broadcast: self.broadcast.unwrap_or_else(default_broadcast_config),
                allow_list: self.allow_list.unwrap_or_default(),
            },
            perf: self.perf.unwrap_or_default(),
            admin: AdminConfig {
                uds_path: self.admin_path.unwrap_or_else(default_admin_path),
                control_api: None,
            },
            telemetry,
            log: self.log.unwrap_or_default(),

            consul: self.consul,
            reaper: self.reaper,
            highlow: HighLowConfig::default(),
            files: FilesConfig::default(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigBuilderError {
    #[error("db.path required")]
    DbPathRequired,
    #[error("gossip.addr required")]
    GossipAddrRequired,
    #[error("api.addr required")]
    ApiAddrRequired,
}

/// Log format (JSON only)
#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum LogFormat {
    #[default]
    Plaintext,
    Json,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConsulConfig {
    pub client: consul_client::Config,
}

// ReaperConfig specifies tables and the duration after which clock and pk records for deleted
// primary keys can be deleted (i.e data in <table>__crsql_pks and <table>__crsql_clock).
// WARNING: Specifying table to be reaped can cause inconsistencies if old primary keys come back
// after specified duration. Use with caution.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReaperConfig {
    pub tables: HashMap<String, TableReapConfig>,
    #[serde(default = "default_reaper_interval")]
    pub check_interval: usize,
    /// When false, skip scanning for PK rows with no clock entries.
    #[serde(default)]
    pub check_orphaned_pks: bool,
    /// Max orphaned PK rows to reap per table per tick.
    #[serde(default = "default_reaper_limit")]
    pub row_limit: usize,
}

/// Per-table reaper config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TableReapConfig {
    pub retention: String,
    /// Optional WHERE clause fragment (without "WHERE") to only delete pks
    /// that match the filter e.g "id LIKE 'throwaway-%'"
    #[serde(default)]
    pub match_filter: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlow_requires_one_role_and_pinned_sftp() {
        let empty = HighLowConfig {
            enabled: true,
            ..Default::default()
        };
        assert!(empty.validate().is_err());

        let config = HighLowConfig {
            enabled: true,
            transport: Some(HighLowTransportConfig {
                kind: HighLowTransportKind::Sftp,
                endpoint: "sftp://relay.example.invalid/drop".into(),
                username: None,
                password_env: None,
                private_key_env: None,
                private_key_passphrase_env: None,
                host_key_sha256: None,
                ca_file: None,
                bearer_token_env: None,
                domain: None,
            }),
            low: Some(HighLowLowConfig {
                stream_id: "low-a".into(),
                network_name: "network-a".into(),
                upload_interval_seconds: 60,
                recipient_key_id: "high-key".into(),
                recipient_rsa_public_key_env: "HIGH_KEY".into(),
                sender_signing_key_env: "LOW_KEY".into(),
            }),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn broadcast_config_defaults_to_gossip() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:4001",
        }))
        .unwrap();
        assert_eq!(cfg.broadcast.method(), BroadcastMethod::Gossip);
        assert!(cfg.plumtree().is_none());
        assert!(!cfg.allow_mixed_ip);
    }

    #[test]
    fn broadcast_config_plumtree() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:4001",
            "broadcast": {
                "plumtree": {
                    "prune_threshold": 7
                }
            }
        }))
        .unwrap();
        assert_eq!(cfg.broadcast.method(), BroadcastMethod::Plumtree);
        assert_eq!(cfg.plumtree().unwrap().prune_threshold, 7);
        assert_eq!(cfg.plumtree().unwrap().optimization_threshold, None);
        assert_eq!(cfg.plumtree().unwrap().ring_locked_radius, 1);
    }

    #[test]
    fn client_binds_default_is_unspecified_v6() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:4001",
        }))
        .unwrap();
        assert_eq!(
            cfg.client_binds().unwrap(),
            ClientBinds::Single(DEFAULT_GOSSIP_CLIENT_ADDR)
        );
    }

    #[test]
    fn client_binds_legacy_client_addr() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:4001",
            "client_addr": "0.0.0.0:0",
        }))
        .unwrap();
        assert_eq!(
            cfg.client_binds().unwrap(),
            ClientBinds::Single("0.0.0.0:0".parse().unwrap())
        );
    }

    #[test]
    fn client_binds_by_family() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:4001",
            "client_addr_v4": "0.0.0.0:0",
            "client_addr_v6": "[::]:0",
        }))
        .unwrap();
        assert_eq!(
            cfg.client_binds().unwrap(),
            ClientBinds::ByFamily {
                v4: Some("0.0.0.0:0".parse().unwrap()),
                v6: Some("[::]:0".parse().unwrap()),
            }
        );
    }

    #[test]
    fn client_binds_v4_only() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:4001",
            "client_addr_v4": "127.0.0.1:0",
        }))
        .unwrap();
        assert_eq!(
            cfg.client_binds().unwrap(),
            ClientBinds::ByFamily {
                v4: Some("127.0.0.1:0".parse().unwrap()),
                v6: None,
            }
        );
    }

    #[test]
    fn client_binds_rejects_mixed_modes() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:4001",
            "client_addr": "[::]:0",
            "client_addr_v4": "0.0.0.0:0",
        }))
        .unwrap();
        assert_eq!(
            cfg.client_binds().unwrap_err(),
            ClientBindsError::MixedModes
        );
    }

    #[test]
    fn client_binds_rejects_wrong_family() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:4001",
            "client_addr_v4": "[::]:0",
        }))
        .unwrap();
        assert!(matches!(
            cfg.client_binds().unwrap_err(),
            ClientBindsError::FamilyMismatch {
                field: "client_addr_v4",
                expected: "IPv4",
                ..
            }
        ));
    }

    #[test]
    fn plumtree_eager_ratios_must_sum_to_100() {
        let ratios = plum_foca::EagerRatios {
            near_pct: 60,
            mid_pct: 30,
            far_pct: 20,
        };
        assert!(ratios.validate().is_err());
        assert!(plum_foca::EagerRatios::default().validate().is_ok());
    }

    #[test]
    fn test_allow_list_default_wildcard() {
        let allow = AllowList::default();
        assert!(allow.is_allowed(&"10.0.0.1:8787".parse().unwrap()));
        assert!(allow.is_allowed(&"192.168.1.1:8787".parse().unwrap()));
        assert!(allow.is_allowed(&"[::1]:8787".parse().unwrap()));
    }

    #[test]
    fn test_allow_list_specific_ips_and_cidrs() {
        let list: AllowList = vec![
            "10.0.0.1".to_string(),
            "192.168.1.0/24".to_string(),
            "::1".to_string(),
        ]
        .try_into()
        .unwrap();

        // Specific IP allowed
        assert!(list.is_allowed(&"10.0.0.1:8787".parse().unwrap()));
        assert!(!list.is_allowed(&"10.0.0.2:8787".parse().unwrap()));

        // CIDR subnet allowed
        assert!(list.is_allowed(&"192.168.1.1:8787".parse().unwrap()));
        assert!(list.is_allowed(&"192.168.1.254:8787".parse().unwrap()));
        assert!(!list.is_allowed(&"192.168.2.1:8787".parse().unwrap()));

        // IPv6 allowed
        assert!(list.is_allowed(&"[::1]:8787".parse().unwrap()));
        assert!(!list.is_allowed(&"[::2]:8787".parse().unwrap()));
    }

    #[test]
    fn test_allow_list_deserialization() {
        let cfg: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:8787",
            "allow-list": ["10.0.0.1", "10.0.0.2"]
        }))
        .unwrap();
        assert_eq!(cfg.allow_list.raw(), &["10.0.0.1", "10.0.0.2"]);
        assert!(cfg.allow_list.is_allowed(&"10.0.0.1:8787".parse().unwrap()));
        assert!(!cfg.allow_list.is_allowed(&"10.0.0.3:8787".parse().unwrap()));

        // test alias allow_list
        let cfg2: GossipConfig = serde_json::from_value(serde_json::json!({
            "bind_addr": "127.0.0.1:8787",
            "allow_list": ["192.168.1.0/24"]
        }))
        .unwrap();
        assert!(cfg2
            .allow_list
            .is_allowed(&"192.168.1.50:8787".parse().unwrap()));
        assert!(!cfg2
            .allow_list
            .is_allowed(&"10.0.0.1:8787".parse().unwrap()));
    }

    #[test]
    fn test_db_config_defaults_and_custom() {
        // Default JSON / config
        let db: DbConfig = serde_json::from_value(serde_json::json!({
            "path": "/var/lib/corrosion/corrosion.db"
        }))
        .unwrap();
        assert_eq!(db.path, "/var/lib/corrosion/corrosion.db");
        assert_eq!(db.cache_size_kib, DEFAULT_CACHE_SIZE_KIB);
        assert_eq!(db.mmap_size_bytes, DEFAULT_MMAP_SIZE_BYTES);
        assert_eq!(
            db.journal_size_limit_bytes,
            DEFAULT_JOURNAL_SIZE_LIMIT_BYTES
        );

        // Custom values
        let db_custom: DbConfig = serde_json::from_value(serde_json::json!({
            "path": "/tmp/test.db",
            "cache_size_kib": -2097152,
            "mmap_size_bytes": 268435456,
            "journal_size_limit_bytes": 536870912
        }))
        .unwrap();
        assert_eq!(db_custom.path, "/tmp/test.db");
        assert_eq!(db_custom.cache_size_kib, -2097152);
        assert_eq!(db_custom.mmap_size_bytes, 268435456);
        assert_eq!(db_custom.journal_size_limit_bytes, 536870912);
    }
}
