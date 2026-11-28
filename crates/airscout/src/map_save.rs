//! Map screenshot filename and dependency-free PNG encoding.

use as_geo::band::Band;
use chrono::{DateTime, Utc};
use eframe::egui::ColorImage;

/// The original `MapSave` filename (MapDlg.cs:3506-3513), with D-009's extension.
pub fn map_save_file_name(band: Band, my_call: &str, dx_call: &str, t: DateTime<Utc>) -> String {
    let band = match band {
        Band::None => "BNONE".to_owned(),
        Band::All => "BALL".to_owned(),
        _ => format!("{band:?}"),
    };
    let my_call = my_call.replace('/', "_");
    let dx_call = dx_call.replace('/', "_");
    let time = t.format("%Y%m%d_%H%M%S");
    format!("{band}_{my_call}_{dx_call}_{time}.png")
}

/// Encode a nonempty image as RGBA8 PNG with filter None and stored DEFLATE blocks.
pub fn encode_png(image: &ColorImage) -> Vec<u8> {
    let [width, height] = image.size;
    let mut scanlines = Vec::with_capacity(height * (1 + 4 * width));
    if width != 0 {
        for row in image.pixels.chunks_exact(width) {
            scanlines.push(0); // PNG filter None.
            for pixel in row {
                scanlines.extend_from_slice(&pixel.to_srgba_unmultiplied());
            }
        }
    }

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = [0; 13];
    header[..4].copy_from_slice(&(width as u32).to_be_bytes());
    header[4..8].copy_from_slice(&(height as u32).to_be_bytes());
    header[8] = 8; // Bits per channel.
    header[9] = 6; // Truecolor with alpha.
    write_chunk(&mut png, b"IHDR", &header);

    let mut zlib = vec![0x78, 0x01]; // DEFLATE, 32 KiB window, no dictionary.
    if scanlines.is_empty() {
        zlib.extend_from_slice(&[0x01, 0x00, 0x00, 0xff, 0xff]); // Empty final stored block.
    }
    let mut blocks = scanlines.chunks(usize::from(u16::MAX)).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(u8::from(blocks.peek().is_none())); // BFINAL, BTYPE=00, byte padding.
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&scanlines).to_be_bytes());
    write_chunk(&mut png, b"IDAT", &zlib);
    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    png.extend_from_slice(&crc32(&png[start..]).to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let polynomial = if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
            crc = (crc >> 1) ^ polynomial;
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    const MODULUS: u32 = 65_521;
    let mut a = 1;
    let mut b = 0;
    for &byte in bytes {
        a = (a + u32::from(byte)) % MODULUS;
        b = (b + a) % MODULUS;
    }
    (b << 16) | a
}
