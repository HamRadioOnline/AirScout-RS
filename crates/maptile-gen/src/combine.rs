//! `combine`: the PNG files of a directory side by side in one PNG (`CombineImages`,
//! `MapTileGenerator/MainDlg.cs:96-137`, called by `btn_CreatePNG_Click` at `:145-147`).
//!
//! Deviations from the C#: the output goes to `--out` (C# writes `zoom.png` under the path of
//! the exe), an existing output is replaced only with `--overwrite`, an empty directory is an
//! error (C# returns silently).

use crate::{write_via_part, Error};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// `SystemColors.AppWorkspace`, the background `g.Clear` paints (`MainDlg.cs:116`).
const BACKGROUND: u8 = 171;
/// Decoder limit for one input.
const INPUT_LIMIT: usize = 64 * 1024 * 1024;
/// Largest width (and height) of the output.
const MAX_SIDE: u64 = 65_535;
/// Largest output canvas (8-bit RGB), so a very tall input cannot force a huge allocation.
const MAX_CANVAS_BYTES: u64 = 256 * 1024 * 1024;

type PngReader = png::Reader<BufReader<File>>;

fn reader(path: &Path) -> Result<PngReader, Error> {
    let ctx = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let f = File::open(path).map_err(|e| ctx(&e))?;
    let mut d =
        png::Decoder::new_with_limits(BufReader::new(f), png::Limits { bytes: INPUT_LIMIT });
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let r = d.read_info().map_err(|e| ctx(&e))?;
    match r.output_buffer_size() {
        Some(n) if n <= INPUT_LIMIT => Ok(r),
        _ => Err(ctx(&format!("decoded image larger than {INPUT_LIMIT} bytes")).into()),
    }
}

/// Every `*.png` of `dir` (not recursive), ordered by file name, ordinal (`Array.Sort` at
/// `:146`; .NET's default string order is culture-aware, the same for the digit names of tiles).
fn png_files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    for e in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        let is_png = p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("png"));
        if is_png && p.is_file() {
            files.push(p);
        }
    }
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(files)
}

/// Draws one decoded image at column `x0`, alpha over what is there (`g.DrawImage`).
fn draw(canvas: &mut [u8], width: usize, x0: usize, info: &png::OutputInfo, buf: &[u8]) {
    let ch = info.color_type.samples();
    for y in 0..info.height as usize {
        let row = &buf[y * info.line_size..][..info.width as usize * ch];
        for (x, p) in row.chunks_exact(ch).enumerate() {
            let (rgb, a) = match info.color_type {
                png::ColorType::Grayscale => ([p[0]; 3], 255),
                png::ColorType::GrayscaleAlpha => ([p[0]; 3], p[1]),
                png::ColorType::Rgb => ([p[0], p[1], p[2]], 255),
                // Rgba; Indexed is expanded by the decoder.
                _ => ([p[0], p[1], p[2]], p[3]),
            };
            let dst = &mut canvas[(y * width + x0 + x) * 3..][..3];
            for (d, s) in dst.iter_mut().zip(rgb) {
                let v = u32::from(s) * u32::from(a) + u32::from(*d) * (255 - u32::from(a));
                *d = ((v + 127) / 255) as u8;
            }
        }
    }
}

pub(crate) fn run(dir: &Path, out: &Path, overwrite: bool) -> Result<(), Error> {
    if !overwrite && fs::symlink_metadata(out).is_ok() {
        return Err(format!("{} exists; use --overwrite to replace it", out.display()).into());
    }
    let files = png_files(dir)?;
    if files.is_empty() {
        return Err(format!("no PNG files in {}", dir.display()).into());
    }
    // MainDlg.cs:105-113: the width is the sum, the height the tallest.
    let (mut width, mut height) = (0u64, 0u64);
    for f in &files {
        let (w, h) = reader(f)?.info().size();
        width += u64::from(w);
        height = height.max(u64::from(h));
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!(
            "the combined image would be {width} x {height} px; at most {MAX_SIDE} px per side"
        )
        .into());
    }
    if width * height * 3 > MAX_CANVAS_BYTES {
        return Err(format!(
            "the combined image would be {width} x {height} px, more than {MAX_CANVAS_BYTES} bytes"
        )
        .into());
    }
    let (width, height) = (width as usize, height as usize);
    let mut canvas = vec![BACKGROUND; width * height * 3];
    // MainDlg.cs:117-132: side by side from x = 0.
    let mut x0 = 0usize;
    for f in &files {
        let mut r = reader(f)?;
        let mut buf = vec![0; r.output_buffer_size().unwrap_or(0)];
        let info = r
            .next_frame(&mut buf)
            .map_err(|e| format!("{}: {e}", f.display()))?;
        let (w, h) = (info.width as usize, info.height as usize);
        if x0 + w > width || h > height || info.bit_depth != png::BitDepth::Eight {
            return Err(format!("{}: changed while it was read", f.display()).into());
        }
        draw(&mut canvas, width, x0, &info, &buf);
        x0 += w;
    }
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, width as u32, height as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header()?;
        w.write_image_data(&canvas)?;
        w.finish()?;
    }
    write_via_part(out, &png_bytes, None).map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "{}: {width} x {height} px from {} files",
        out.display(),
        files.len()
    );
    Ok(())
}
