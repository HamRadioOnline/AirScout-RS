//! One named plane feed settings table per file, using the same TOML shape
//! as `Settings::feed_settings`. Files may contain credentials.

use as_settings::FeedSettings;
use std::collections::BTreeMap;
use std::path::Path;
use toml::Table;

/// Suggested export name or import glob, matching the upstream `.feed` extension.
pub fn feed_file_name(name: &str, export: bool) -> String {
    if export {
        format!("{name}.feed")
    } else {
        "*.feed".into()
    }
}

pub fn export_feed(name: &str, table: &Table, path: &Path) -> Result<(), String> {
    if name.trim().is_empty() || table.is_empty() {
        return Err("Cannot export empty plane feed settings.".into());
    }
    let feeds = BTreeMap::from([(name, table)]);
    let text = toml::to_string_pretty(&feeds).map_err(|_| "Cannot encode plane feed settings.")?;
    as_settings::write_private_atomic(path, &text)
        .map_err(|_| "Cannot write the plane feed settings file.".into())
}

pub fn import_feed(path: &Path) -> Result<(String, Table), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| "Cannot import plane feed settings: could not read the file.")?;
    // TOML errors can include source lines, so never display or log them.
    let mut feeds: BTreeMap<String, FeedSettings> = toml::from_str(&text)
        .map_err(|_| "Cannot import plane feed settings: invalid feed file.")?;
    match (feeds.len(), feeds.pop_first()) {
        (1, Some((name, table))) if !name.trim().is_empty() && !table.is_empty() => {
            Ok((name, table.0))
        }
        _ => Err("Cannot import plane feed settings: expected one non-empty feed table.".into()),
    }
}
