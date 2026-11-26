//! `elevation-tile-gen`: the C# `ElevationTileGenerator` as a command line tool.
#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand};
use elevation_tile_gen::hgt::{
    diff_bytes, diff_stats, downsample, histogram_csv, read_file, write_file,
};
use elevation_tile_gen::preview::{read_tiles, square_rgba, write_png};
use elevation_tile_gen::{
    catalogue_text, globe_block_square, globe_indices, globe_tile, globe_tile_index, identify,
    loc_path, scan_catalogue, srtm_tiles, write_catalogue, write_loc, zip_squares, CatFrom,
    Outcome, Source, SourceKind,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "elevation-tile-gen",
    version,
    about = "Build .loc elevation tiles from SRTM and GLOBE files (local files only)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build .loc tiles from SRTM3, SRTM1 or GLOBE source files.
    Tiles(TilesArgs),
    /// Zip the .loc files of every square folder as `<out>/AB/ABCD.zip`.
    Zip(ZipArgs),
    /// Write the tile catalogue `<out>/files.cat` and `<out>/files.zip`.
    Catalogue(CatalogueArgs),
    /// Difference of two SRTM files (`A - B`, little-endian) and its statistics.
    Compare(CompareArgs),
    /// An SRTM1 file as an SRTM3 file (3 x 3 means).
    Downsample(DownsampleArgs),
    /// A PNG picture of one square from its tiles.
    Preview(PreviewArgs),
}

#[derive(Args)]
struct CompareArgs {
    /// First SRTM file (big-endian).
    a: PathBuf,
    /// Second SRTM file of the same length.
    b: PathBuf,
    /// The difference file to write.
    #[arg(long)]
    out: PathBuf,
    /// Also write the histogram as CSV (`Difference[m];Count`) to this file.
    #[arg(long)]
    histogram: Option<PathBuf>,
    /// Replace an existing output file.
    #[arg(long)]
    overwrite: bool,
}

#[derive(Args)]
struct DownsampleArgs {
    /// SRTM1 source file (for example `N50E010.hgt`).
    input: PathBuf,
    /// The SRTM3 file to write.
    #[arg(long)]
    out: PathBuf,
    /// Replace an existing output file.
    #[arg(long)]
    overwrite: bool,
}

#[derive(Args)]
struct PreviewArgs {
    /// A square zip (`JO50.zip`) or a directory of `.loc` files.
    input: PathBuf,
    /// The PNG file to write.
    #[arg(long)]
    out: PathBuf,
    /// Replace an existing output file.
    #[arg(long)]
    overwrite: bool,
}

#[derive(Args)]
struct ZipArgs {
    /// Root of the tile tree.
    #[arg(long)]
    out: PathBuf,
    /// Delete the .loc files of a square after its new zip is read back and compared.
    #[arg(long)]
    remove_loc: bool,
}

#[derive(Args)]
struct CatalogueArgs {
    /// Root of the tile tree.
    #[arg(long)]
    out: PathBuf,
    /// Read the tiles from the square zips or from the .loc files.
    #[arg(long, value_enum, default_value = "zip")]
    from: CatFrom,
    /// Creation date in the header, RFC 3339 (default: now, UTC).
    #[arg(long)]
    time: Option<String>,
}

#[derive(Args)]
struct TilesArgs {
    /// Source files, or directories whose files are used (sorted by name).
    #[arg(required = true)]
    sources: Vec<PathBuf>,
    /// Root of the tile tree (`<out>/JO/50/JO50MB.loc`); one model per tree.
    #[arg(long)]
    out: PathBuf,
    /// Replace existing .loc files (default: keep them, as the C# tool does).
    #[arg(long)]
    overwrite: bool,
    /// Only these squares, comma separated (for example `JO50,JO61`).
    #[arg(long, value_delimiter = ',')]
    squares: Vec<String>,
    /// LastUpdated of every tile, RFC 3339 (default: now, UTC).
    #[arg(long)]
    time: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Tiles(a) => run_tiles(&a),
        Cmd::Zip(a) => run_zip(&a),
        Cmd::Catalogue(a) => run_catalogue(&a),
        Cmd::Compare(a) => run_compare(&a),
        Cmd::Downsample(a) => run_downsample(&a),
        Cmd::Preview(a) => run_preview(&a),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

/// `--squares`: upper-cased 4-character squares such as `JO50`.
fn parse_squares(list: &[String]) -> Result<Vec<String>, String> {
    list.iter()
        .map(|s| {
            let u = s.trim().to_ascii_uppercase();
            let b = u.as_bytes();
            let ok = b.len() == 4
                && b[..2].iter().all(|c| (b'A'..=b'R').contains(c))
                && b[2..].iter().all(u8::is_ascii_digit);
            if ok {
                Ok(u)
            } else {
                Err(format!(
                    "invalid square {s:?}: expected a square such as JO50"
                ))
            }
        })
        .collect()
}

/// The file arguments, with each directory replaced by its entries sorted by name.
fn expand(args: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for a in args {
        if a.is_dir() {
            let mut entries = std::fs::read_dir(a)
                .and_then(|rd| {
                    rd.map(|e| e.map(|e| e.path()))
                        .collect::<Result<Vec<_>, _>>()
                })
                .map_err(|e| format!("{}: {e}", a.display()))?;
            entries.sort_by(|x, y| x.file_name().cmp(&y.file_name()));
            out.extend(entries);
        } else {
            out.push(a.clone());
        }
    }
    Ok(out)
}

/// The sources among the inputs; the others are reported on stderr and skipped.
fn sources(inputs: &[PathBuf]) -> Result<Vec<Source>, String> {
    let mut found = Vec::new();
    for p in inputs {
        match identify(p).map_err(|e| format!("{}: {e}", p.display()))? {
            Some(s) => found.push(s),
            None => eprintln!("not an SRTM1, SRTM3 or GLOBE file: {}", p.display()),
        }
    }
    if found.is_empty() {
        return Err("no source file".into());
    }
    let first = found[0].kind;
    if let Some(other) = found.iter().map(|s| s.kind).find(|&k| k != first) {
        return Err(format!(
            "inputs mix {} and {}: run once per model with its own --out",
            first.name(),
            other.name()
        ));
    }
    Ok(found)
}

#[derive(Default)]
struct Counts {
    written: u64,
    void: u64,
    kept: u64,
}

struct Run<'a> {
    out: &'a Path,
    overwrite: bool,
    squares: &'a [String],
    now: DateTime<Utc>,
}

impl Run<'_> {
    fn exists(&self, index: &str) -> bool {
        !self.overwrite && loc_path(self.out, index).exists()
    }

    fn wanted(&self, index: &str) -> bool {
        self.squares.is_empty() || self.squares.iter().any(|s| index.get(..4) == Some(s))
    }

    fn write(&self, tile: &as_elevation::ElevationTile, n: &mut Counts) -> Result<(), String> {
        write_loc(self.out, tile).map_err(|e| e.to_string())?;
        n.written += 1;
        Ok(())
    }

    fn srtm(&self, src: &Source, data: &[u8], n: &mut Counts) -> Result<(), String> {
        let outcomes = srtm_tiles(src, data, self.now, &|i| self.exists(i))
            .map_err(|e| format!("{}: {e}", src.path.display()))?;
        // filtered after the whole file: `srtm_tiles` decides void per tile (#293)
        for o in outcomes.iter().filter(|o| self.wanted(o.index())) {
            match o {
                Outcome::Tile(t) => self.write(t, n)?,
                Outcome::Void(_) => n.void += 1,
                Outcome::Exists(_) => n.kept += 1,
            }
        }
        Ok(())
    }

    fn globe(&self, src: &Source, data: &[u8], n: &mut Counts) -> Result<(), String> {
        let mut block = None;
        let mut block_wanted = true;
        for g in globe_indices(src) {
            let key = (g.i0, g.j0, g.i1, g.j1);
            if block != Some(key) {
                block = Some(key);
                block_wanted = self.wanted(&globe_block_square(src, g));
            }
            if !block_wanted {
                continue;
            }
            let index = globe_tile_index(src, g);
            if !self.wanted(&index) {
                continue;
            }
            if self.exists(&index) {
                n.kept += 1;
                continue;
            }
            match globe_tile(src, data, g, self.now)
                .map_err(|e| format!("{}: {e}", src.path.display()))?
            {
                Some(t) => self.write(&t, n)?,
                None => n.void += 1,
            }
        }
        Ok(())
    }
}

/// `--time`, or now.
fn parse_time(time: &Option<String>) -> Result<DateTime<Utc>, String> {
    match time {
        Some(t) => Ok(DateTime::parse_from_rfc3339(t)
            .map_err(|e| format!("invalid --time {t:?}: {e}"))?
            .with_timezone(&Utc)),
        None => Ok(Utc::now()),
    }
}

fn print_json(v: &serde_json::Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(v).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn run_zip(a: &ZipArgs) -> Result<(), String> {
    let r = zip_squares(&a.out, a.remove_loc).map_err(|e| e.to_string())?;
    print_json(&serde_json::json!({
        "squares": r.squares,
        "tiles": r.tiles,
        "removed": r.removed,
    }))
}

fn run_catalogue(a: &CatalogueArgs) -> Result<(), String> {
    let created = parse_time(&a.time)?;
    let files = scan_catalogue(&a.out, a.from).map_err(|e| e.to_string())?;
    let text = catalogue_text(&files, created);
    write_catalogue(&a.out, &text).map_err(|e| format!("{}: {e}", a.out.display()))?;
    print_json(&serde_json::json!({
        "tiles": files.len(),
        "from": match a.from {
            CatFrom::Zip => "zip",
            CatFrom::Loc => "loc",
        },
        "files": ["files.cat", "files.zip"],
    }))
}

/// Refuse an existing `path` unless `overwrite`. The C# tools write fixed file names
/// (`MainDlg.cs:1094`, `:1075`, `:843`), never a path the user typed.
fn refuse_existing(path: &Path, overwrite: bool) -> Result<(), String> {
    if !overwrite && path.exists() {
        return Err(format!(
            "{} exists: use --overwrite to replace it",
            path.display()
        ));
    }
    Ok(())
}

fn run_compare(a: &CompareArgs) -> Result<(), String> {
    refuse_existing(&a.out, a.overwrite)?;
    if let Some(csv) = &a.histogram {
        refuse_existing(csv, a.overwrite)?;
    }
    let (x, y) = (read_file(&a.a), read_file(&a.b));
    let diff = diff_bytes(
        &x.map_err(|e| e.to_string())?,
        &y.map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{} and {}: {e}", a.a.display(), a.b.display()))?;
    let (stats, hist) = diff_stats(&diff);
    write_file(&a.out, &diff).map_err(|e| e.to_string())?;
    if let Some(csv) = &a.histogram {
        let text = match (stats.min, stats.max) {
            (Some(min), Some(max)) => histogram_csv(&hist, min, max),
            _ => histogram_csv(&hist, 0, -1),
        };
        write_file(csv, text.as_bytes()).map_err(|e| e.to_string())?;
    }
    print_json(&serde_json::to_value(&stats).map_err(|e| e.to_string())?)
}

fn run_downsample(a: &DownsampleArgs) -> Result<(), String> {
    let srtm1 = identify(&a.input)
        .map_err(|e| format!("{}: {e}", a.input.display()))?
        .filter(|s| s.kind == SourceKind::Srtm1)
        .ok_or_else(|| format!("not an SRTM1 file: {}", a.input.display()))?;
    refuse_existing(&a.out, a.overwrite)?;
    let data = read_file(&srtm1.path).map_err(|e| e.to_string())?;
    let out = downsample(&data).map_err(|e| format!("{}: {e}", srtm1.path.display()))?;
    write_file(&a.out, &out).map_err(|e| e.to_string())?;
    print_json(&serde_json::json!({
        "out": a.out.display().to_string(),
        "bytes": out.len(),
    }))
}

fn run_preview(a: &PreviewArgs) -> Result<(), String> {
    refuse_existing(&a.out, a.overwrite)?;
    let tiles = read_tiles(&a.input).map_err(|e| e.to_string())?;
    let (width, height, rgba) =
        square_rgba(&tiles).map_err(|e| format!("{}: {e}", a.input.display()))?;
    write_png(&a.out, width, height, &rgba).map_err(|e| format!("{}: {e}", a.out.display()))?;
    print_json(&serde_json::json!({
        "width": width,
        "height": height,
        "tiles": tiles.len(),
    }))
}

fn run_tiles(a: &TilesArgs) -> Result<(), String> {
    let now = parse_time(&a.time)?;
    let squares = parse_squares(&a.squares)?;
    let found = sources(&expand(&a.sources)?)?;
    let run = Run {
        out: &a.out,
        overwrite: a.overwrite,
        squares: &squares,
        now,
    };
    let mut total = Counts::default();
    for src in &found {
        // the whole file in memory, as the C# tool reads it (`MainDlg.cs:222-225`)
        let data = std::fs::read(&src.path).map_err(|e| format!("{}: {e}", src.path.display()))?;
        let mut n = Counts::default();
        match src.kind {
            SourceKind::Srtm3 | SourceKind::Srtm1 => run.srtm(src, &data, &mut n)?,
            SourceKind::Globe => run.globe(src, &data, &mut n)?,
        }
        eprintln!(
            "{}: {} written, {} void, {} kept",
            src.path.display(),
            n.written,
            n.void,
            n.kept
        );
        total.written += n.written;
        total.void += n.void;
        total.kept += n.kept;
    }
    let v = serde_json::json!({
        "model": found[0].kind.name(),
        "out": a.out.display().to_string(),
        "sources": found.len(),
        "written": total.written,
        "void": total.void,
        "kept": total.kept,
    });
    print_json(&v)
}
