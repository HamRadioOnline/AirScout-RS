//! HTTP helpers shared by the updaters: User-Agent and conditional downloads.
#![forbid(unsafe_code)]

use std::path::Path;

/// Errors of a download.
#[derive(thiserror::Error, Debug)]
pub enum NetError {
    #[error("http error: {0}")]
    Http(String),
    /// HTTP 404: the server has no such file.
    #[error("not found on server: {0}")]
    NotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// User-Agent for all downloads.
pub fn user_agent() -> String {
    format!("AirScout-RS/{}", env!("CARGO_PKG_VERSION"))
}

/// GET `url` into `path` unless the server answers 304 to our `If-Modified-Since`.
/// Writes to a temp file first, so an interrupted download never replaces a good copy.
/// Returns `true` if a new file was written, `false` if the local copy is current.
pub fn download_if_newer(
    client: &reqwest::blocking::Client,
    url: &str,
    path: &Path,
) -> Result<bool, NetError> {
    let mut req = client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent());
    if let Ok(mtime) = std::fs::metadata(path).and_then(|m| m.modified()) {
        req = req.header(
            reqwest::header::IF_MODIFIED_SINCE,
            httpdate::fmt_http_date(mtime),
        );
    }
    let resp = req
        .send()
        .map_err(|e| NetError::Http(format!("{url}: {e}")))?;
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED && path.exists() {
        return Ok(false);
    }
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(NetError::NotFound(url.to_string()));
    }
    if !resp.status().is_success() {
        return Err(NetError::Http(format!("{url}: HTTP {}", resp.status())));
    }
    let bytes = resp
        .bytes()
        .map_err(|e| NetError::Http(format!("{url}: {e}")))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(true)
}
