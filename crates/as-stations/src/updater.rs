//! Port of `StationDatabaseUpdater.cs` (one update run; scheduling is the caller's job).

use crate::{StationDb, StationError};
use as_geo::latlon::GRect;
use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{Read, Seek};
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// `Stations_UpdateURL` default.
pub const DEFAULT_UPDATE_URL: &str = "http://www.airscout.eu/downloads/ScoutBase/1/StationData/";
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_MEMBER_BYTES: u64 = 256 * 1024 * 1024;
const CACHE_TAG_LENGTH: usize = b"AirScout station archive v1:0000000000000000\n".len();

/// Rows written by one update run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateStats {
    pub locations: usize,
    pub qrvs: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum ArchiveKind {
    Location,
    Qrv,
}

impl ArchiveKind {
    pub(crate) fn table(self) -> &'static str {
        match self {
            Self::Location => "Location",
            Self::Qrv => "QRV",
        }
    }

    fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Location => ("locations.zip", "locations.json"),
            Self::Qrv => ("qrv.zip", "qrv.json"),
        }
    }
}

pub(crate) struct ArchiveId {
    pub(crate) source: String,
    pub(crate) fingerprint: String,
}

fn read_zip_entry(file: File, name: &str, limit: u64) -> Result<String, StationError> {
    let mut zip = zip::ZipArchive::new(file).map_err(|_| StationError::ArchiveMalformed)?;
    let entry = zip
        .by_name(name)
        .map_err(|_| StationError::ArchiveMalformed)?;
    if entry.size() > limit {
        return Err(StationError::ArchiveTooLarge { limit });
    }
    let bytes = as_net::read_bounded(entry, limit).map_err(|error| match error {
        as_net::BodyError::TooLarge { limit } => StationError::ArchiveTooLarge { limit },
        as_net::BodyError::Read => StationError::ArchiveMalformed,
    })?;
    String::from_utf8(bytes).map_err(|_| StationError::ArchiveMalformed)
}

fn source_identity(url: &str, salt: &[u8]) -> String {
    let mut hash = DefaultHasher::new();
    (salt, "station source", url).hash(&mut hash);
    format!("{:016x}", hash.finish())
}

/// Verify the complete file through the same open handle that will be imported.
fn fingerprint(file: &mut File, salt: &[u8]) -> Result<String, StationError> {
    let len = file.metadata()?.len();
    if len > MAX_ARCHIVE_BYTES + CACHE_TAG_LENGTH as u64 {
        return Err(StationError::ArchiveMalformed);
    }
    file.rewind()?;
    let mut hash = DefaultHasher::new();
    (salt, "station archive", len).hash(&mut hash);
    let mut buffer = [0; 32 * 1024];
    let mut left = len;
    while left != 0 {
        let size = left.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..size])?;
        hash.write(&buffer[..size]);
        left -= size as u64;
    }
    file.rewind()?;
    Ok(format!("{:016x}-{len}", hash.finish()))
}

fn download(
    client: &reqwest::blocking::Client,
    url: &str,
    path: &Path,
    db: &StationDb,
    salt: &[u8],
) -> Result<(File, ArchiveId), StationError> {
    let source = source_identity(url, salt);
    let cache_tag = format!("AirScout station archive v1:{source}\n");
    // This tag is published as part of the archive, so competing downloads
    // cannot attach their validators to a different source's cached snapshot.
    let cached = match File::open(path) {
        Ok(mut file) => {
            let mut tag = [0; CACHE_TAG_LENGTH];
            match file.read_exact(&mut tag) {
                Ok(()) if tag.as_slice() == cache_tag.as_bytes() => {
                    let id = ArchiveId {
                        source: source.clone(),
                        fingerprint: fingerprint(&mut file, salt)?,
                    };
                    db.archive_validators(&id)?
                        .map(|validators| (file, validators))
                }
                Ok(()) => None,
                Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => None,
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let mut archive = as_net::download_snapshot(
        client,
        url,
        path,
        MAX_ARCHIVE_BYTES,
        cached,
        cache_tag.as_bytes(),
    )?;
    let id = ArchiveId {
        source,
        fingerprint: fingerprint(&mut archive.file, salt)?,
    };
    if archive.downloaded {
        db.save_archive_validators(&id, &archive.validators)?;
    }
    Ok((archive.file, id))
}

/// Merge each archive with its source, content and scope marker in one transaction.
/// A cached archive can still add rows when the selection scope changes.
pub fn update_stations(
    client: &reqwest::blocking::Client,
    base_url: &str,
    cache_dir: &Path,
    db: &StationDb,
    area: Option<GRect>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(&str),
) -> Result<UpdateStats, StationError> {
    let salt = db.archive_salt()?;
    let mut import = |kind: ArchiveKind, label| {
        crate::db::check_cancel(cancel)?;
        progress(&format!("Updating {label} from web database..."));
        let (archive, member) = kind.names();
        let (file, id) = download(
            client,
            &format!("{base_url}{archive}"),
            &cache_dir.join(archive),
            db,
            &salt,
        )?;
        let rows = db.import_archive(kind, &id, area, cancel, || {
            read_zip_entry(file, member, MAX_MEMBER_BYTES)
        })?;
        progress(&format!("{rows} {label} updated."));
        Ok::<_, StationError>(rows)
    };
    let locations = import(ArchiveKind::Location, "locations")?;
    let qrvs = import(ArchiveKind::Qrv, "QRV entries")?;
    progress("Station database update completed.");
    Ok(UpdateStats { locations, qrvs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn finding_382_rejects_decoded_member_beyond_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locations.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file("locations.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"[ ]").unwrap();
        zip.finish().unwrap();
        assert_eq!(
            read_zip_entry(File::open(&path).unwrap(), "locations.json", 3).unwrap(),
            "[ ]"
        );
        assert!(read_zip_entry(File::open(&path).unwrap(), "locations.json", 2).is_err());
    }
}
