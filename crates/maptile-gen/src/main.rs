//! `maptile-gen`: the C# `MapTileGenerator` as a command line tool. Local files only, no network.
#![forbid(unsafe_code)]

mod combine;
mod export;
mod import;

use clap::{Args, Parser, Subcommand};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::SystemTime;

pub(crate) type Error = Box<dyn std::error::Error>;

#[derive(Parser)]
#[command(
    name = "maptile-gen",
    version,
    about = "Export, import and combine GMap .gmdb map tiles (local files only)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write every tile of a .gmdb as `<out>/<zoom>/<x>/<y>.png`, dated by its cache time.
    Export(ExportArgs),
    /// Copy tiles from another .gmdb or a `<z>/<x>/<y>.png` tree into a .gmdb.
    Import(ImportArgs),
    /// Put the PNG files of a directory side by side into one PNG.
    Combine(CombineArgs),
}

/// The provider names `--type` takes besides a number.
const PROVIDERS: [&str; 3] = ["OpenStreetMap", "OpenTopoMap", "OCM Landscape"];

/// `--type`: a GMap `DbId` or a provider name.
fn parse_type(s: &str) -> Result<i32, String> {
    s.parse::<i32>()
        .ok()
        .or_else(|| as_maps::db_id(s))
        .ok_or_else(|| {
            format!(
                "not a number or a provider name (one of {})",
                PROVIDERS.join(", ")
            )
        })
}

/// `"1492776782 (OpenStreetMap)"`, or the number alone for an unknown `DbId`.
pub(crate) fn type_name(db_id: i32) -> String {
    match PROVIDERS.iter().find(|p| as_maps::db_id(p) == Some(db_id)) {
        Some(p) => format!("{db_id} ({p})"),
        None => db_id.to_string(),
    }
}

#[derive(Args)]
struct ExportArgs {
    /// The .gmdb to read (opened read-only).
    #[arg(long)]
    db: PathBuf,
    /// Root of the tile tree to write.
    #[arg(long)]
    out: PathBuf,
    /// Only tiles of this provider (`DbId` or name); needed when the db holds several.
    #[arg(long = "type", value_parser = parse_type)]
    db_id: Option<i32>,
}

#[derive(Args)]
struct ImportArgs {
    /// The .gmdb to write (created if missing).
    #[arg(long)]
    db: PathBuf,
    /// A .gmdb or a `<z>/<x>/<y>.png` directory tree.
    source: PathBuf,
    /// Provider (`DbId` or name): the tiles to copy from a .gmdb (default all), the type of the
    /// tiles of a tree (default OpenStreetMap).
    #[arg(long = "type", value_parser = parse_type)]
    db_id: Option<i32>,
}

#[derive(Args)]
struct CombineArgs {
    /// Directory whose `*.png` files are combined (not recursive).
    dir: PathBuf,
    /// The PNG file to write.
    #[arg(long)]
    out: PathBuf,
    /// Replace an existing output file.
    #[arg(long)]
    overwrite: bool,
}

/// `<path>.part`.
fn part_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".part");
    PathBuf::from(p)
}

/// Write `bytes` to `<path>.part`, set its mtime if given, close it, then rename it over `path`.
/// On failure only the `.part` file is removed.
pub(crate) fn write_via_part(
    path: &Path,
    bytes: &[u8],
    mtime: Option<SystemTime>,
) -> std::io::Result<()> {
    let part = part_path(path);
    let done = (|| {
        let mut f = File::create(&part)?;
        f.write_all(bytes)?;
        if let Some(t) = mtime {
            f.set_modified(t)?;
        }
        f.sync_all()?;
        // Closed before the rename (Windows refuses to rename an open file).
        drop(f);
        std::fs::rename(&part, path)
    })();
    if done.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    done
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let done = match cli.cmd {
        Cmd::Export(a) => export::run(&a.db, &a.out, a.db_id),
        Cmd::Import(a) => import::run(&a.db, &a.source, a.db_id),
        Cmd::Combine(a) => combine::run(&a.dir, &a.out, a.overwrite),
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
