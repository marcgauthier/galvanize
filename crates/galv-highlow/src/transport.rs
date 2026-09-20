use std::{
    fs,
    io::Cursor,
    path::PathBuf,
};

use crate::{
    manifest_filename, validate_filename, Artifacts, Error, Manifest, Result,
};
use serde::{Deserialize, Serialize};
use suppaftp::{FtpStream, NativeTlsConnector, NativeTlsFtpStream};
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransportKind {
    Directory,
    Http,
    Https,
    Ftp,
    Ftps,
    Sftp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransportConfig {
    pub kind: TransportKind,
    pub endpoint: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bearer_token: Option<String>,
}

pub fn publish(config: &TransportConfig, artifacts: &Artifacts) -> Result<()> {
    match config.kind {
        TransportKind::Directory => directory_publish(&config.endpoint, artifacts),
        TransportKind::Http => http_publish(&config.endpoint, artifacts, false, config.bearer_token.as_deref()),
        TransportKind::Https => http_publish(&config.endpoint, artifacts, true, config.bearer_token.as_deref()),
        TransportKind::Ftp => ftp_publish(&config.endpoint, artifacts, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Ftps => ftps_publish(&config.endpoint, artifacts, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Sftp => sftp_publish(&config.endpoint, artifacts, config.username.as_deref(), config.password.as_deref()),
    }
}

pub fn list_manifests(config: &TransportConfig) -> Result<Vec<String>> {
    match config.kind {
        TransportKind::Directory => directory_list_manifests(&config.endpoint),
        TransportKind::Http => http_list_manifests(&config.endpoint, false, config.bearer_token.as_deref()),
        TransportKind::Https => http_list_manifests(&config.endpoint, true, config.bearer_token.as_deref()),
        TransportKind::Ftp => ftp_list_manifests(&config.endpoint, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Ftps => ftps_list_manifests(&config.endpoint, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Sftp => sftp_list_manifests(&config.endpoint, config.username.as_deref(), config.password.as_deref()),
    }
}

pub fn fetch_pair(config: &TransportConfig, manifest_filename_str: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    validate_filename(manifest_filename_str)?;
    match config.kind {
        TransportKind::Directory => directory_fetch_pair(&config.endpoint, manifest_filename_str),
        TransportKind::Http => http_fetch_pair(&config.endpoint, manifest_filename_str, false, config.bearer_token.as_deref()),
        TransportKind::Https => http_fetch_pair(&config.endpoint, manifest_filename_str, true, config.bearer_token.as_deref()),
        TransportKind::Ftp => ftp_fetch_pair(&config.endpoint, manifest_filename_str, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Ftps => ftps_fetch_pair(&config.endpoint, manifest_filename_str, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Sftp => sftp_fetch_pair(&config.endpoint, manifest_filename_str, config.username.as_deref(), config.password.as_deref()),
    }
}

// ---------------------------------------------------------------------------
// 1. Directory / Local Filesystem Adapter
// ---------------------------------------------------------------------------

fn directory_path(endpoint: &str) -> PathBuf {
    if let Some(stripped) = endpoint.strip_prefix("dir://") {
        PathBuf::from(stripped)
    } else if let Some(stripped) = endpoint.strip_prefix("file://") {
        PathBuf::from(stripped)
    } else {
        PathBuf::from(endpoint)
    }
}

fn directory_publish(endpoint: &str, artifacts: &Artifacts) -> Result<()> {
    let dir = directory_path(endpoint);
    fs::create_dir_all(&dir)?;

    let payload_name = &artifacts.manifest.payload_filename;
    validate_filename(payload_name)?;
    let manifest_name = manifest_filename(payload_name)?;
    let manifest_bytes = artifacts.manifest.to_bytes()?;

    let payload_temp = dir.join(format!(".{payload_name}.partial"));
    let payload_final = dir.join(payload_name);
    fs::write(&payload_temp, &artifacts.payload)?;
    fs::rename(&payload_temp, &payload_final)?;

    let manifest_temp = dir.join(format!(".{manifest_name}.partial"));
    let manifest_final = dir.join(&manifest_name);
    fs::write(&manifest_temp, &manifest_bytes)?;
    fs::rename(&manifest_temp, &manifest_final)?;

    Ok(())
}

fn directory_list_manifests(endpoint: &str) -> Result<Vec<String>> {
    let dir = directory_path(endpoint);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".json.galv") && !name.starts_with('.') && validate_filename(&name).is_ok() {
            manifests.push(name);
        }
    }
    manifests.sort();
    Ok(manifests)
}

fn directory_fetch_pair(endpoint: &str, manifest_filename_str: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let dir = directory_path(endpoint);
    let manifest_path = dir.join(manifest_filename_str);
    let manifest_bytes = fs::read(manifest_path)?;

    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| Error::Configuration(format!("invalid manifest JSON: {e}")))?;
    validate_filename(&manifest.payload_filename)?;

    let payload_path = dir.join(&manifest.payload_filename);
    let payload_bytes = fs::read(payload_path)?;

    Ok((manifest_bytes, payload_bytes))
}

// ---------------------------------------------------------------------------
// 2. HTTP / HTTPS Adapter
// ---------------------------------------------------------------------------

fn http_publish(endpoint: &str, artifacts: &Artifacts, _tls: bool, bearer_token: Option<&str>) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| Error::Configuration(format!("failed to build HTTP client: {e}")))?;

    let base = endpoint.trim_end_matches('/');
    let payload_name = &artifacts.manifest.payload_filename;
    validate_filename(payload_name)?;
    let manifest_name = manifest_filename(payload_name)?;
    let manifest_bytes = artifacts.manifest.to_bytes()?;

    let payload_url = format!("{base}/artifacts/{payload_name}");
    let mut req = client.post(&payload_url).body(artifacts.payload.clone()).header("Content-Type", "application/octet-stream");
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let res = req.send().map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("HTTP POST payload failed: {e}"))))?;
    if !res.status().is_success() {
        return Err(Error::Configuration(format!("HTTP POST payload returned status: {}", res.status())));
    }

    let manifest_url = format!("{base}/artifacts/{manifest_name}");
    let mut req = client.post(&manifest_url).body(manifest_bytes).header("Content-Type", "application/json");
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let res = req.send().map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("HTTP POST manifest failed: {e}"))))?;
    if !res.status().is_success() {
        return Err(Error::Configuration(format!("HTTP POST manifest returned status: {}", res.status())));
    }

    Ok(())
}

fn http_list_manifests(endpoint: &str, _tls: bool, bearer_token: Option<&str>) -> Result<Vec<String>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Configuration(format!("failed to build HTTP client: {e}")))?;

    let base = endpoint.trim_end_matches('/');
    let url = format!("{base}/manifests");
    let mut req = client.get(&url);
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let res = req.send().map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("HTTP GET manifests failed: {e}"))))?;
    if !res.status().is_success() {
        return Err(Error::Configuration(format!("HTTP GET manifests returned status: {}", res.status())));
    }

    let bytes = res.bytes().map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
    let mut names: Vec<String> = serde_json::from_slice(&bytes).map_err(|e| Error::Configuration(format!("HTTP response not JSON string array: {e}")))?;
    names.retain(|name| name.ends_with(".json.galv") && !name.starts_with('.') && validate_filename(name).is_ok());
    names.sort();
    Ok(names)
}

fn http_fetch_pair(endpoint: &str, manifest_filename_str: &str, _tls: bool, bearer_token: Option<&str>) -> Result<(Vec<u8>, Vec<u8>)> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| Error::Configuration(format!("failed to build HTTP client: {e}")))?;

    let base = endpoint.trim_end_matches('/');
    let manifest_url = format!("{base}/artifacts/{manifest_filename_str}");
    let mut req = client.get(&manifest_url);
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let res = req.send().map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("HTTP GET manifest failed: {e}"))))?;
    if !res.status().is_success() {
        return Err(Error::Configuration(format!("HTTP GET manifest returned status: {}", res.status())));
    }
    let manifest_bytes = res.bytes().map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?.to_vec();

    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| Error::Configuration(format!("invalid manifest JSON: {e}")))?;
    validate_filename(&manifest.payload_filename)?;

    let payload_url = format!("{base}/artifacts/{}", manifest.payload_filename);
    let mut req = client.get(&payload_url);
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let res = req.send().map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("HTTP GET payload failed: {e}"))))?;
    if !res.status().is_success() {
        return Err(Error::Configuration(format!("HTTP GET payload returned status: {}", res.status())));
    }
    let payload_bytes = res.bytes().map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?.to_vec();

    Ok((manifest_bytes, payload_bytes))
}

// ---------------------------------------------------------------------------
// 3. FTP / FTPS Adapter
// ---------------------------------------------------------------------------

fn connect_ftp(endpoint: &str, user: Option<&str>, pass: Option<&str>) -> Result<FtpStream> {
    let url = Url::parse(endpoint).map_err(|e| Error::Configuration(format!("invalid FTP endpoint: {e}")))?;
    let host = url.host_str().ok_or_else(|| Error::Configuration("FTP endpoint requires a host".into()))?;
    let port = url.port().unwrap_or(21);
    let address = format!("{host}:{port}");
    let username = user.or_else(|| if url.username().is_empty() { None } else { Some(url.username()) }).unwrap_or("anonymous");
    let password = pass.or_else(|| url.password()).unwrap_or("anonymous@galvanize.invalid");

    let mut ftp = FtpStream::connect(&address).map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP connect failed: {e}"))))?;
    ftp.login(username, password).map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP login failed: {e}"))))?;
    let path = url.path().trim_matches('/');
    if !path.is_empty() {
        ftp.cwd(path).map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP cwd failed: {e}"))))?;
    }
    Ok(ftp)
}

fn connect_ftps(endpoint: &str, user: Option<&str>, pass: Option<&str>) -> Result<NativeTlsFtpStream> {
    let url = Url::parse(endpoint).map_err(|e| Error::Configuration(format!("invalid FTPS endpoint: {e}")))?;
    let host = url.host_str().ok_or_else(|| Error::Configuration("FTPS endpoint requires a host".into()))?;
    let port = url.port().unwrap_or(21);
    let address = format!("{host}:{port}");
    let username = user.or_else(|| if url.username().is_empty() { None } else { Some(url.username()) }).unwrap_or("anonymous");
    let password = pass.or_else(|| url.password()).unwrap_or("anonymous@galvanize.invalid");

    let connector = NativeTlsConnector::from(
        suppaftp::native_tls::TlsConnector::new()
            .map_err(|e| Error::Configuration(format!("cannot create FTPS TLS connector: {e}")))?,
    );
    let ftp = NativeTlsFtpStream::connect(&address).map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS connect failed: {e}"))))?;
    let mut ftp = ftp.into_secure(connector, host).map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS handshake failed: {e}"))))?;
    ftp.login(username, password).map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS login failed: {e}"))))?;
    let path = url.path().trim_matches('/');
    if !path.is_empty() {
        ftp.cwd(path).map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS cwd failed: {e}"))))?;
    }
    Ok(ftp)
}

fn ftp_publish(endpoint: &str, artifacts: &Artifacts, user: Option<&str>, pass: Option<&str>) -> Result<()> {
    let payload_name = &artifacts.manifest.payload_filename;
    validate_filename(payload_name)?;
    let manifest_name = manifest_filename(payload_name)?;
    let manifest_bytes = artifacts.manifest.to_bytes()?;

    let payload_temp = format!(".{payload_name}.partial");
    let manifest_temp = format!(".{manifest_name}.partial");

    let mut ftp = connect_ftp(endpoint, user, pass)?;
    ftp.put_file(&payload_temp, &mut Cursor::new(&artifacts.payload))
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP put payload failed: {e}"))))?;
    ftp.rename(&payload_temp, payload_name)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP rename payload failed: {e}"))))?;

    ftp.put_file(&manifest_temp, &mut Cursor::new(&manifest_bytes))
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP put manifest failed: {e}"))))?;
    ftp.rename(&manifest_temp, &manifest_name)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP rename manifest failed: {e}"))))?;

    let _ = ftp.quit();
    Ok(())
}

fn ftps_publish(endpoint: &str, artifacts: &Artifacts, user: Option<&str>, pass: Option<&str>) -> Result<()> {
    let payload_name = &artifacts.manifest.payload_filename;
    validate_filename(payload_name)?;
    let manifest_name = manifest_filename(payload_name)?;
    let manifest_bytes = artifacts.manifest.to_bytes()?;

    let payload_temp = format!(".{payload_name}.partial");
    let manifest_temp = format!(".{manifest_name}.partial");

    let mut ftp = connect_ftps(endpoint, user, pass)?;
    ftp.put_file(&payload_temp, &mut Cursor::new(&artifacts.payload))
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS put payload failed: {e}"))))?;
    ftp.rename(&payload_temp, payload_name)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS rename payload failed: {e}"))))?;

    ftp.put_file(&manifest_temp, &mut Cursor::new(&manifest_bytes))
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS put manifest failed: {e}"))))?;
    ftp.rename(&manifest_temp, &manifest_name)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS rename manifest failed: {e}"))))?;

    let _ = ftp.quit();
    Ok(())
}

fn ftp_list_manifests(endpoint: &str, user: Option<&str>, pass: Option<&str>) -> Result<Vec<String>> {
    let mut ftp = connect_ftp(endpoint, user, pass)?;
    let names = ftp.nlst(None)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP nlst failed: {e}"))))?;
    let mut manifests = names
        .into_iter()
        .filter(|n| n.ends_with(".json.galv") && !n.starts_with('.') && validate_filename(n).is_ok())
        .collect::<Vec<_>>();
    manifests.sort();
    let _ = ftp.quit();
    Ok(manifests)
}

fn ftps_list_manifests(endpoint: &str, user: Option<&str>, pass: Option<&str>) -> Result<Vec<String>> {
    let mut ftp = connect_ftps(endpoint, user, pass)?;
    let names = ftp.nlst(None)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS nlst failed: {e}"))))?;
    let mut manifests = names
        .into_iter()
        .filter(|n| n.ends_with(".json.galv") && !n.starts_with('.') && validate_filename(n).is_ok())
        .collect::<Vec<_>>();
    manifests.sort();
    let _ = ftp.quit();
    Ok(manifests)
}

fn ftp_fetch_pair(endpoint: &str, manifest_filename_str: &str, user: Option<&str>, pass: Option<&str>) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut ftp = connect_ftp(endpoint, user, pass)?;
    let manifest_bytes = ftp.retr_as_buffer(manifest_filename_str)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP retr manifest failed: {e}"))))?
        .into_inner();

    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| Error::Configuration(format!("invalid manifest JSON: {e}")))?;
    validate_filename(&manifest.payload_filename)?;

    let payload_bytes = ftp.retr_as_buffer(&manifest.payload_filename)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTP retr payload failed: {e}"))))?
        .into_inner();

    let _ = ftp.quit();
    Ok((manifest_bytes, payload_bytes))
}

fn ftps_fetch_pair(endpoint: &str, manifest_filename_str: &str, user: Option<&str>, pass: Option<&str>) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut ftp = connect_ftps(endpoint, user, pass)?;
    let manifest_bytes = ftp.retr_as_buffer(manifest_filename_str)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS retr manifest failed: {e}"))))?
        .into_inner();

    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| Error::Configuration(format!("invalid manifest JSON: {e}")))?;
    validate_filename(&manifest.payload_filename)?;

    let payload_bytes = ftp.retr_as_buffer(&manifest.payload_filename)
        .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("FTPS retr payload failed: {e}"))))?
        .into_inner();

    let _ = ftp.quit();
    Ok((manifest_bytes, payload_bytes))
}

// ---------------------------------------------------------------------------
// 4. SFTP Adapter stub (or directory fallback for testing)
// ---------------------------------------------------------------------------

fn sftp_publish(_endpoint: &str, _artifacts: &Artifacts, _user: Option<&str>, _pass: Option<&str>) -> Result<()> {
    Err(Error::Configuration("SFTP direct transport requires ssh2 backend configuration".into()))
}

fn sftp_list_manifests(_endpoint: &str, _user: Option<&str>, _pass: Option<&str>) -> Result<Vec<String>> {
    Err(Error::Configuration("SFTP direct transport requires ssh2 backend configuration".into()))
}

fn sftp_fetch_pair(_endpoint: &str, _manifest_filename: &str, _user: Option<&str>, _pass: Option<&str>) -> Result<(Vec<u8>, Vec<u8>)> {
    Err(Error::Configuration("SFTP direct transport requires ssh2 backend configuration".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_directory_transport_publish_and_fetch() -> Result<()> {
        let dir = tempdir()?;
        let endpoint = dir.path().display().to_string();

        let config = TransportConfig {
            kind: TransportKind::Directory,
            endpoint,
            username: None,
            password: None,
            bearer_token: None,
        };

        let manifest = Manifest {
            format: crate::MANIFEST_FORMAT.into(),
            bundle_id: "test-bundle-123".into(),
            stream_id: "stream-1".into(),
            sequence_first: 1,
            sequence_last: 1,
            schema_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
            payload_filename: "test-bundle.zstd.galvh".into(),
            payload_size: 11,
            payload_sha256: "dummy".into(),
            content_sha256: "dummy".into(),
            compressed_sha256: "dummy".into(),
            recipient_key_id: "high-1".into(),
            sender_key_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
            signature: "dummy".into(),
        };

        let artifacts = Artifacts {
            payload: b"hello payload".to_vec(),
            manifest: manifest.clone(),
        };

        publish(&config, &artifacts)?;

        let manifests = list_manifests(&config)?;
        assert_eq!(manifests, vec!["test-bundle.json.galv"]);

        let (fetched_manifest_bytes, fetched_payload) = fetch_pair(&config, "test-bundle.json.galv")?;
        assert_eq!(fetched_payload, b"hello payload");

        let parsed: Manifest = serde_json::from_slice(&fetched_manifest_bytes).unwrap();
        assert_eq!(parsed.bundle_id, "test-bundle-123");

        Ok(())
    }
}
