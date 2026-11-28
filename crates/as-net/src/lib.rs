//! HTTP helpers shared by the updaters: User-Agent and conditional downloads.
#![forbid(unsafe_code)]

pub use httpdate::parse_http_date;

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// A body could not be collected within its byte budget.
#[derive(thiserror::Error, Debug)]
pub enum BodyError {
    #[error("response exceeds {limit} bytes")]
    TooLarge { limit: u64 },
    #[error("response body read failed")]
    Read,
}

pub fn read_bounded(body: impl Read, limit: u64) -> Result<Vec<u8>, BodyError> {
    let mut bytes = Vec::new();
    body.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| BodyError::Read)?;
    if bytes.len() as u64 > limit {
        return Err(BodyError::TooLarge { limit });
    }
    Ok(bytes)
}

pub fn read_response(
    response: reqwest::blocking::Response,
    limit: u64,
) -> Result<Vec<u8>, BodyError> {
    if response.content_length().is_some_and(|size| size > limit) {
        return Err(BodyError::TooLarge { limit });
    }
    read_bounded(response, limit)
}

/// User-Agent for all downloads.
pub fn user_agent() -> String {
    format!("AirScout-RS/{}", env!("CARGO_PKG_VERSION"))
}

/// Conditional selectors supplied by a server for a specific archived response.
#[derive(Clone, Default)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// An open handle to the exact response bytes, even after a concurrent replacement.
pub struct ArchiveSnapshot {
    pub file: File,
    pub validators: Validators,
    pub downloaded: bool,
}

/// Publish an archive atomically and return the immutable snapshot that was selected.
/// A cached handle must already match its source and validators.
pub fn download_snapshot(
    client: &reqwest::blocking::Client,
    url: &str,
    path: &Path,
    max_bytes: u64,
    cached: Option<(File, Validators)>,
    cache_tag: &[u8],
) -> Result<ArchiveSnapshot, NetError> {
    use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, USER_AGENT};

    let mut req = client.get(url).header(USER_AGENT, user_agent());
    if let Some((_, validators)) = &cached {
        if let Some(etag) = &validators.etag {
            req = req.header(IF_NONE_MATCH, etag);
        }
        if let Some(modified) = &validators.last_modified {
            req = req.header(IF_MODIFIED_SINCE, modified);
        }
    }
    let response = req.send().map_err(|error| {
        NetError::Http(if error.is_timeout() {
            "request timed out".into()
        } else {
            "request failed".into()
        })
    })?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        if let Some((file, validators)) = cached {
            return Ok(ArchiveSnapshot {
                file,
                validators,
                downloaded: false,
            });
        }
        return Err(NetError::Http(
            "HTTP 304 without a verified cached response".into(),
        ));
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(NetError::NotFound("HTTP 404".into()));
    }
    if !status.is_success() {
        return Err(NetError::Http(format!("HTTP {}", status.as_u16())));
    }
    if response
        .content_length()
        .is_some_and(|size| size > max_bytes)
    {
        return Err(size_error(max_bytes));
    }
    drop(cached);
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let validators = Validators {
        etag: header(ETAG),
        last_modified: header(LAST_MODIFIED),
    };
    let file = publish_download(path, response, max_bytes, cache_tag)?;
    Ok(ArchiveSnapshot {
        file,
        validators,
        downloaded: true,
    })
}

/// Stream a response into a private staging file and publish it when complete.
/// Existing callers use the local modification time as their legacy validator.
pub fn download_if_newer(
    client: &reqwest::blocking::Client,
    url: &str,
    path: &Path,
    max_bytes: u64,
) -> Result<bool, NetError> {
    let cached = File::open(path).ok().and_then(|file| {
        let modified = file.metadata().ok()?.modified().ok()?;
        let validators = Validators {
            etag: None,
            last_modified: Some(httpdate::fmt_http_date(modified)),
        };
        Some((file, validators))
    });
    Ok(download_snapshot(client, url, path, max_bytes, cached, &[])?.downloaded)
}

fn size_error(max_bytes: u64) -> NetError {
    NetError::Http(format!("response exceeds {max_bytes} bytes"))
}

fn publish_download(
    path: &Path,
    mut body: impl Read,
    max_bytes: u64,
    cache_tag: &[u8],
) -> Result<File, NetError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let (staging, mut file) = open_staging(parent)?;
    let written = (|| {
        file.write_all(cache_tag)?;
        let mut buffer = [0; 16 * 1024];
        let mut remaining = max_bytes;
        loop {
            let limit = remaining.saturating_add(1).min(buffer.len() as u64) as usize;
            let count = body
                .read(&mut buffer[..limit])
                .map_err(|_| NetError::Http("response body read failed".into()))?;
            if count == 0 {
                break;
            }
            remaining = remaining
                .checked_sub(count as u64)
                .ok_or_else(|| size_error(max_bytes))?;
            file.write_all(&buffer[..count])?;
        }
        file.sync_all()?;
        file.rewind()?;
        Ok::<_, NetError>(())
    })();
    let result = written.and_then(|()| Ok(fs::rename(&staging, path)?));
    if let Err(error) = result {
        drop(file);
        let _ = fs::remove_file(staging);
        return Err(error);
    }
    Ok(file)
}

fn open_staging(parent: &Path) -> io::Result<(PathBuf, File)> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    loop {
        let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".airscout-download.{}.{serial}.part",
            std::process::id()
        ));
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    struct PausedRead<F>(F);

    impl<F: FnMut() -> io::Result<usize>> Read for PausedRead<F> {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            (self.0)()
        }
    }

    #[test]
    fn finding_305_concurrent_staging_never_publishes_partial_bytes() {
        for fail in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("archive.zip");
            fs::write(&path, b"previous").unwrap();

            let (staged, staged_rx) = mpsc::channel();
            let (finish, finish_rx) = mpsc::channel();
            let writer_path = path.clone();
            let writer = std::thread::spawn(move || {
                let tail = PausedRead(|| {
                    staged.send(()).unwrap();
                    finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    if fail {
                        return Err(io::Error::other("synthetic interruption"));
                    }
                    Ok(0)
                });
                let body = b"first".as_slice().chain(tail);
                publish_download(&writer_path, body, 8, &[])
            });

            staged_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"previous");
            publish_download(&path, b"second".as_slice(), 8, &[]).unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"second");
            finish.send(()).unwrap();
            assert_eq!(writer.join().unwrap().is_err(), fail);
            let expected: &[u8] = if fail { b"second" } else { b"first" };
            assert_eq!(fs::read(&path).unwrap(), expected);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}
