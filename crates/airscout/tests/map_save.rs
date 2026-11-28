use airscout::app::{AppState, FailedServices};
use airscout::map_save::{encode_png, map_save_file_name};
use as_aircraft::PlaneInfoCache;
use as_geo::band::Band;
use as_settings::Settings;
use chrono::{DateTime, TimeZone, Utc};
use eframe::egui::{Color32, ColorImage};
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn timestamp() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 27, 12, 34, 56).unwrap()
}

fn image() -> ColorImage {
    ColorImage::new(
        [2, 1],
        vec![
            Color32::RED,
            Color32::from_rgba_premultiplied(0, 128, 0, 128),
        ],
    )
}

fn app(data_dir: &Path) -> AppState {
    let settings = Settings {
        band: Band::B1_2G,
        my_call: "TE1ST/P".into(),
        dx_call: "TE2ST".into(),
        ..Settings::default()
    };
    let mut app = AppState::new(
        settings,
        None,
        Box::new(FailedServices(String::new())),
        Arc::new(PlaneInfoCache::new()),
    );
    app.data_dir = Some(data_dir.to_path_buf());
    app
}

#[test]
fn file_name_replaces_slashes() {
    assert_eq!(
        map_save_file_name(Band::B1_2G, "TE1ST/P", "TE2ST", timestamp()),
        "B1_2G_TE1ST_P_TE2ST_20261127_123456.png"
    );
    assert_eq!(
        map_save_file_name(Band::None, "TE1ST", "TE2ST/P", timestamp()),
        "BNONE_TE1ST_TE2ST_P_20261127_123456.png"
    );
    assert_eq!(
        map_save_file_name(Band::All, "TE1ST", "TE2ST", timestamp()),
        "BALL_TE1ST_TE2ST_20261127_123456.png"
    );
}

fn chunk<'a>(png: &mut &'a [u8], kind: &[u8; 4]) -> (&'a [u8], &'a [u8]) {
    let len = u32::from_be_bytes(png[..4].try_into().unwrap()) as usize;
    assert_eq!(&png[4..8], kind);
    let (data, rest) = png[8..].split_at(len);
    let (crc, rest) = rest.split_at(4);
    *png = rest;
    (data, crc)
}

#[test]
fn png_roundtrip_header_and_crc() {
    let png = encode_png(&image());
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let mut chunks = &png[8..];
    let (ihdr, crc) = chunk(&mut chunks, b"IHDR");
    assert_eq!(ihdr, &[0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
    assert_eq!(crc, &0xf422_7f8au32.to_be_bytes());
    let (idat, crc) = chunk(&mut chunks, b"IDAT");
    assert_eq!(crc, &0x36a0_6860u32.to_be_bytes());
    assert_eq!(&idat[..2], &[0x78, 0x01]);
    assert_eq!(idat[2], 1); // BFINAL = 1, BTYPE = 0
    let len = u16::from_le_bytes(idat[3..5].try_into().unwrap());
    let nlen = u16::from_le_bytes(idat[5..7].try_into().unwrap());
    assert_eq!(len, 9);
    assert_eq!(nlen, !len);
    let (scanline, adler) = idat[7..].split_at(usize::from(len));
    assert_eq!(scanline, &[0, 255, 0, 0, 255, 0, 255, 0, 128]);
    assert_eq!(adler, &0x1079_037eu32.to_be_bytes());
    let (iend, crc) = chunk(&mut chunks, b"IEND");
    assert!(iend.is_empty());
    assert_eq!(crc, &[0xae, 0x42, 0x60, 0x82]);
    assert!(chunks.is_empty());
}

#[test]
fn png_splits_large_scanline_into_stored_blocks() {
    let png = encode_png(&ColorImage::filled([16_384, 1], Color32::BLACK));
    let mut chunks = &png[8..];
    chunk(&mut chunks, b"IHDR");
    let (idat, _) = chunk(&mut chunks, b"IDAT");
    assert_eq!(&idat[2..7], &[0, 0xff, 0xff, 0x00, 0x00]);
    assert_eq!(
        &idat[7 + 65_535..],
        &[1, 2, 0, 0xfd, 0xff, 0, 255, 0x78, 0x52, 0xc3, 0xb2]
    );
}

#[test]
fn png_empty_scanlines_have_final_stored_block() {
    for size in [[2, 0], [0, 2]] {
        let png = encode_png(&ColorImage::new(size, vec![]));
        let mut chunks = &png[8..];
        chunk(&mut chunks, b"IHDR");
        let (idat, _) = chunk(&mut chunks, b"IDAT");
        assert_eq!(
            idat,
            &[0x78, 0x01, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01]
        );
    }
}

#[test]
fn save_creates_tmp_dir_and_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    app.settings.time.online = false;
    app.settings.time.offline = timestamp();
    let now = timestamp() + chrono::TimeDelta::hours(1);
    let path = app.save_map_image(&image(), now).unwrap();
    assert_eq!(
        path,
        dir.path()
            .join("Tmp/B1_2G_TE1ST_P_TE2ST_20261127_123456.png")
    );
    assert_eq!(fs::read(path).unwrap(), encode_png(&image()));
}

#[test]
fn save_error_goes_to_status() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Tmp"), b"").unwrap();
    let mut app = app(dir.path());
    let err = app.save_map_image(&image(), timestamp()).unwrap_err();
    assert!(err.starts_with("Map save failed:"));
    assert_eq!(app.status, err);
}

#[test]
fn save_rejects_zero_width() {
    assert_empty_screenshot_error([0, 2]);
}

#[test]
fn save_rejects_zero_height() {
    assert_empty_screenshot_error([2, 0]);
}

fn assert_empty_screenshot_error(size: [usize; 2]) {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    let image = ColorImage::new(size, vec![]);
    let err = app.save_map_image(&image, timestamp()).unwrap_err();
    assert_eq!(err, "Map save failed: empty screenshot");
    assert_eq!(app.status, err);
    assert!(!dir.path().join("Tmp").exists());
}

#[test]
fn request_map_save_sets_pending_flag() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app(dir.path());
    assert!(!app.map_save_pending);
    app.request_map_save();
    app.request_map_save();
    assert!(app.map_save_pending);
    assert!(!dir.path().join("Tmp").exists());
}
