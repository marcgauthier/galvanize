use std::{
    fs,
    io::Cursor,
    path::PathBuf,
};

use galv_highlow::transport::{TransportConfig, TransportKind};
use suppaftp::{FtpStream, NativeTlsConnector, NativeTlsFtpStream};
use tracing::debug;
use url::Url;

use crate::{validate_file_uuid, FileError};

pub fn file_transport_name(uuid_str: &str) -> String {
    format!("{uuid_str}.file")
}

pub fn publish_file_to_transport(
    config: &TransportConfig,
    uuid_str: &str,
    payload: &[u8],
) -> Result<(), FileError> {
    validate_file_uuid(uuid_str)?;
    let filename = file_transport_name(uuid_str);

    match config.kind {
        TransportKind::Directory => directory_publish_file(&config.endpoint, &filename, payload),
        TransportKind::Http => http_publish_file(&config.endpoint, &filename, payload, false, config.bearer_token.as_deref()),
        TransportKind::Https => http_publish_file(&config.endpoint, &filename, payload, true, config.bearer_token.as_deref()),
        TransportKind::Ftp => ftp_publish_file(&config.endpoint, &filename, payload, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Ftps => ftps_publish_file(&config.endpoint, &filename, payload, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Sftp => sftp_publish_file(&config.endpoint, &filename, payload, config.username.as_deref(), config.password.as_deref()),
    }
}

pub fn download_file_from_transport(
    config: &TransportConfig,
    uuid_str: &str,
) -> Result<Vec<u8>, FileError> {
    validate_file_uuid(uuid_str)?;
    let filename = file_transport_name(uuid_str);

    match config.kind {
        TransportKind::Directory => directory_download_file(&config.endpoint, &filename),
        TransportKind::Http => http_download_file(&config.endpoint, &filename, false, config.bearer_token.as_deref()),
        TransportKind::Https => http_download_file(&config.endpoint, &filename, true, config.bearer_token.as_deref()),
        TransportKind::Ftp => ftp_download_file(&config.endpoint, &filename, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Ftps => ftps_download_file(&config.endpoint, &filename, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Sftp => sftp_download_file(&config.endpoint, &filename, config.username.as_deref(), config.password.as_deref()),
    }
}

pub fn list_airgap_files(config: &TransportConfig) -> Result<Vec<String>, FileError> {
    match config.kind {
        TransportKind::Directory => directory_list_files(&config.endpoint),
        TransportKind::Http => http_list_files(&config.endpoint, false, config.bearer_token.as_deref()),
        TransportKind::Https => http_list_files(&config.endpoint, true, config.bearer_token.as_deref()),
        TransportKind::Ftp => ftp_list_files(&config.endpoint, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Ftps => ftps_list_files(&config.endpoint, config.username.as_deref(), config.password.as_deref()),
        TransportKind::Sftp => sftp_list_files(&config.endpoint, config.username.as_deref(), config.password.as_deref()),
    }
}

// ---------------------------------------------------------------------------
// 1. Directory Transport
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

fn directory_publish_file(endpoint: &str, filename: &str, payload: &[u8]) -> Result<(), FileError> {
    let dir = directory_path(endpoint);
    fs::create_dir_all(&dir)?;

    let temp_path = dir.join(format!(".{filename}.partial"));
    let final_path = dir.join(filename);

    fs::write(&temp_path, payload)?;
    fs::rename(&temp_path, &final_path)?;
    debug!(%filename, endpoint = %dir.display(), "published file to directory transport");
    Ok(())
}

fn directory_download_file(endpoint: &str, filename: &str) -> Result<Vec<u8>, FileError> {
    let dir = directory_path(endpoint);
    let path = dir.join(filename);
    if !path.exists() {
        return Err(FileError::NotFound(filename.to_string()));
    }
    let data = fs::read(path)?;
    Ok(data)
}

fn directory_list_files(endpoint: &str) -> Result<Vec<String>, FileError> {
    let dir = directory_path(endpoint);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".file") && !name.starts_with('.') {
            if let Some(uuid) = name.strip_suffix(".file") {
                files.push(uuid.to_string());
            }
        }
    }
    files.sort();
    Ok(files)
}

// ---------------------------------------------------------------------------
// 2. HTTP / HTTPS Transport
// ---------------------------------------------------------------------------

fn http_publish_file(
    endpoint: &str,
    filename: &str,
    payload: &[u8],
    _tls: bool,
    bearer_token: Option<&str>,
) -> Result<(), FileError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| FileError::Transport(format!("failed to build HTTP client: {e}")))?;

    let base = endpoint.trim_end_matches('/');
    let url = format!("{base}/{filename}");

    let mut req = client.post(&url).body(payload.to_vec());
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .map_err(|e| FileError::Transport(format!("HTTP publish failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(FileError::Transport(format!(
            "HTTP publish returned status {}",
            resp.status()
        )));
    }
    Ok(())
}

fn http_download_file(
    endpoint: &str,
    filename: &str,
    _tls: bool,
    bearer_token: Option<&str>,
) -> Result<Vec<u8>, FileError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| FileError::Transport(format!("failed to build HTTP client: {e}")))?;

    let base = endpoint.trim_end_matches('/');
    let url = format!("{base}/{filename}");

    let mut req = client.get(&url);
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .map_err(|e| FileError::Transport(format!("HTTP fetch failed: {e}")))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(FileError::NotFound(filename.to_string()));
    }
    if !resp.status().is_success() {
        return Err(FileError::Transport(format!(
            "HTTP fetch returned status {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .map_err(|e| FileError::Transport(format!("HTTP read body failed: {e}")))?;
    Ok(bytes.to_vec())
}

fn http_list_files(
    endpoint: &str,
    _tls: bool,
    bearer_token: Option<&str>,
) -> Result<Vec<String>, FileError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| FileError::Transport(format!("failed to build HTTP client: {e}")))?;

    let base = endpoint.trim_end_matches('/');
    let url = format!("{base}/files");

    let mut req = client.get(&url);
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .map_err(|e| FileError::Transport(format!("HTTP list files failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(FileError::Transport(format!(
            "HTTP list returned status {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .map_err(|e| FileError::Transport(format!("HTTP read body failed: {e}")))?;
    let files: Vec<String> = serde_json::from_slice(&bytes)
        .map_err(|e| FileError::Transport(format!("invalid JSON file list: {e}")))?;
    Ok(files)
}

// ---------------------------------------------------------------------------
// 3. FTP / FTPS Transport
// ---------------------------------------------------------------------------

fn connect_ftp(endpoint: &str, user: Option<&str>, pass: Option<&str>) -> Result<FtpStream, FileError> {
    let url = Url::parse(endpoint).map_err(|e| FileError::Transport(format!("invalid FTP endpoint: {e}")))?;
    let host = url.host_str().ok_or_else(|| FileError::Transport("FTP endpoint requires a host".into()))?;
    let port = url.port().unwrap_or(21);
    let address = format!("{host}:{port}");
    let username = user.or_else(|| if url.username().is_empty() { None } else { Some(url.username()) }).unwrap_or("anonymous");
    let password = pass.or_else(|| url.password()).unwrap_or("anonymous@galvanize.invalid");

    let mut ftp = FtpStream::connect(&address).map_err(|e| FileError::Transport(format!("FTP connect failed: {e}")))?;
    ftp.login(username, password).map_err(|e| FileError::Transport(format!("FTP login failed: {e}")))?;
    let path = url.path().trim_matches('/');
    if !path.is_empty() {
        ftp.cwd(path).map_err(|e| FileError::Transport(format!("FTP cwd failed: {e}")))?;
    }
    Ok(ftp)
}

fn connect_ftps(endpoint: &str, user: Option<&str>, pass: Option<&str>) -> Result<NativeTlsFtpStream, FileError> {
    let url = Url::parse(endpoint).map_err(|e| FileError::Transport(format!("invalid FTPS endpoint: {e}")))?;
    let host = url.host_str().ok_or_else(|| FileError::Transport("FTPS endpoint requires a host".into()))?;
    let port = url.port().unwrap_or(21);
    let address = format!("{host}:{port}");
    let username = user.or_else(|| if url.username().is_empty() { None } else { Some(url.username()) }).unwrap_or("anonymous");
    let password = pass.or_else(|| url.password()).unwrap_or("anonymous@galvanize.invalid");

    let connector = NativeTlsConnector::from(
        suppaftp::native_tls::TlsConnector::new()
            .map_err(|e| FileError::Transport(format!("cannot create FTPS TLS connector: {e}")))?,
    );
    let ftp = NativeTlsFtpStream::connect(&address).map_err(|e| FileError::Transport(format!("FTPS connect failed: {e}")))?;
    let mut ftp = ftp.into_secure(connector, host).map_err(|e| FileError::Transport(format!("FTPS handshake failed: {e}")))?;
    ftp.login(username, password).map_err(|e| FileError::Transport(format!("FTPS login failed: {e}")))?;
    let path = url.path().trim_matches('/');
    if !path.is_empty() {
        ftp.cwd(path).map_err(|e| FileError::Transport(format!("FTPS cwd failed: {e}")))?;
    }
    Ok(ftp)
}

fn ftp_publish_file(
    endpoint: &str,
    filename: &str,
    payload: &[u8],
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<(), FileError> {
    let mut ftp = connect_ftp(endpoint, user, pass)?;
    let temp_name = format!(".{filename}.partial");
    ftp.put_file(&temp_name, &mut Cursor::new(payload))
        .map_err(|e| FileError::Transport(format!("FTP put_file failed: {e}")))?;
    ftp.rename(temp_name.as_str(), filename)
        .map_err(|e| FileError::Transport(format!("FTP rename failed: {e}")))?;
    let _ = ftp.quit();
    Ok(())
}

fn ftps_publish_file(
    endpoint: &str,
    filename: &str,
    payload: &[u8],
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<(), FileError> {
    let mut ftp = connect_ftps(endpoint, user, pass)?;
    let temp_name = format!(".{filename}.partial");
    ftp.put_file(&temp_name, &mut Cursor::new(payload))
        .map_err(|e| FileError::Transport(format!("FTPS put_file failed: {e}")))?;
    ftp.rename(temp_name.as_str(), filename)
        .map_err(|e| FileError::Transport(format!("FTPS rename failed: {e}")))?;
    let _ = ftp.quit();
    Ok(())
}

fn ftp_download_file(
    endpoint: &str,
    filename: &str,
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<Vec<u8>, FileError> {
    let mut ftp = connect_ftp(endpoint, user, pass)?;
    let bytes = ftp.retr_as_buffer(filename)
        .map_err(|e| FileError::Transport(format!("FTP retr failed: {e}")))?
        .into_inner();
    let _ = ftp.quit();
    Ok(bytes)
}

fn ftps_download_file(
    endpoint: &str,
    filename: &str,
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<Vec<u8>, FileError> {
    let mut ftp = connect_ftps(endpoint, user, pass)?;
    let bytes = ftp.retr_as_buffer(filename)
        .map_err(|e| FileError::Transport(format!("FTPS retr failed: {e}")))?
        .into_inner();
    let _ = ftp.quit();
    Ok(bytes)
}

fn ftp_list_files(
    endpoint: &str,
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<Vec<String>, FileError> {
    let mut ftp = connect_ftp(endpoint, user, pass)?;
    let lines = ftp
        .nlst(None)
        .map_err(|e| FileError::Transport(format!("FTP nlst failed: {e}")))?;
    let _ = ftp.quit();
    let mut files = Vec::new();
    for line in lines {
        if line.ends_with(".file") {
            if let Some(uuid) = line.strip_suffix(".file") {
                files.push(uuid.to_string());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn ftps_list_files(
    endpoint: &str,
    user: Option<&str>,
    pass: Option<&str>,
) -> Result<Vec<String>, FileError> {
    let mut ftp = connect_ftps(endpoint, user, pass)?;
    let lines = ftp
        .nlst(None)
        .map_err(|e| FileError::Transport(format!("FTPS nlst failed: {e}")))?;
    let _ = ftp.quit();
    let mut files = Vec::new();
    for line in lines {
        if line.ends_with(".file") {
            if let Some(uuid) = line.strip_suffix(".file") {
                files.push(uuid.to_string());
            }
        }
    }
    files.sort();
    Ok(files)
}

// ---------------------------------------------------------------------------
// 4. SFTP Transport
// ---------------------------------------------------------------------------

fn sftp_publish_file(
    _endpoint: &str,
    _filename: &str,
    _payload: &[u8],
    _user: Option<&str>,
    _pass: Option<&str>,
) -> Result<(), FileError> {
    Err(FileError::Transport("SFTP direct transport requires ssh2 backend configuration".into()))
}

fn sftp_download_file(
    _endpoint: &str,
    _filename: &str,
    _user: Option<&str>,
    _pass: Option<&str>,
) -> Result<Vec<u8>, FileError> {
    Err(FileError::Transport("SFTP direct transport requires ssh2 backend configuration".into()))
}

fn sftp_list_files(
    _endpoint: &str,
    _user: Option<&str>,
    _pass: Option<&str>,
) -> Result<Vec<String>, FileError> {
    Err(FileError::Transport("SFTP direct transport requires ssh2 backend configuration".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_directory_airgap_publish_and_download() {
        let dir = tempdir().unwrap();
        let config = TransportConfig {
            kind: TransportKind::Directory,
            endpoint: dir.path().to_str().unwrap().to_string(),
            username: None,
            password: None,
            bearer_token: None,
        };
        let uuid = "c25c3870-761d-4eb0-a337-0ea876878b27";
        let content = b"Air-gap file replication payload test.";

        publish_file_to_transport(&config, uuid, content).unwrap();

        let list = list_airgap_files(&config).unwrap();
        assert_eq!(list, vec![uuid.to_string()]);

        let downloaded = download_file_from_transport(&config, uuid).unwrap();
        assert_eq!(downloaded, content);
    }
}
