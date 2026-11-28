//! The user-supplied map overlay (`InitializeUserSupplied`, `MapDlg.cs:1512-1575`): an image
//! file stretched over a lat/lon box and painted over the tiles at its own opacity.
//!
//! [`decode`] reads and decodes the file and can run on any thread; [`OverlayImage`] owns the
//! uploaded texture. [`OverlayLayer`] keeps the committed texture of the main map, keyed by the
//! file and the box, and decodes a changed key off the UI thread. A file or box that fails keeps
//! the committed texture; switching the overlay off releases it.

use as_geo::latlon::GRect;
use as_settings::GuiSettings;
use eframe::egui::{self, pos2, Color32, ColorImage, Rect, TextureHandle, TextureOptions};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use walkers::{lon_lat, Projector};

/// Files larger than this are refused before decoding [bytes].
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Width and height limit of a decoded image [px] (the wgpu default texture limit).
pub const MAX_SIDE: u32 = 8192;
/// Allocation limit of the decoder [bytes]: one [`MAX_SIDE`]² RGBA image plus headroom.
const MAX_DECODE_ALLOC: u64 = 320 * 1024 * 1024;
/// `Map_UserSuppliedOverlay_FromZoomLevel` / `_ToZoomLevel` (`Settings.settings:796-801`).
pub const ZOOM_RANGE: std::ops::RangeInclusive<f64> = 1.0..=20.0;
/// Web Mercator latitude limit [deg]; the box is clamped to it for projecting only.
const MERCATOR_MAX_LAT: f64 = 85.051_128_78;

const READ_ERROR: &str = "User supplied overlay: cannot read the image file.";
const DECODE_ERROR: &str =
    "User supplied overlay: the file is not a supported image (JPEG, PNG, BMP, TIFF).";
const SIZE_ERROR: &str = "User supplied overlay: the image file is larger than 64 MiB.";
const DIMENSION_ERROR: &str = "User supplied overlay: the image is larger than 8192 x 8192 pixels.";
const BOUNDS_ERROR: &str =
    "User supplied overlay: the bounding box is invalid (Min. Lat/Lon must be below Max. Lat/Lon, within -90..90 and -180..180).";
const WORKER_ERROR: &str = "User supplied overlay: the image could not be loaded.";

/// The overlay box of the settings.
pub fn bounds_of(g: &GuiSettings) -> GRect {
    GRect::new(
        g.overlay_min_lat,
        g.overlay_min_lon,
        g.overlay_max_lat,
        g.overlay_max_lon,
    )
}

/// `Ok` for a proper box: finite, latitudes in -90..=90, longitudes in -180..=180, min below max.
pub fn validate_bounds(b: &GRect) -> Result<(), String> {
    let lat = -90.0..=90.0;
    let lon = -180.0..=180.0;
    let ok = lat.contains(&b.min_lat)
        && lat.contains(&b.max_lat)
        && lon.contains(&b.min_lon)
        && lon.contains(&b.max_lon)
        && b.min_lat < b.max_lat
        && b.min_lon < b.max_lon;
    if ok {
        Ok(())
    } else {
        Err(BOUNDS_ERROR.to_string())
    }
}

/// Read and decode `path` into an RGBA image, refusing files over [`MAX_FILE_BYTES`] and images
/// over [`MAX_SIDE`] per side. The error is a fixed text without the path.
pub fn decode(path: &Path) -> Result<ColorImage, String> {
    let meta = std::fs::metadata(path).map_err(|_| READ_ERROR.to_string())?;
    if !meta.is_file() {
        return Err(READ_ERROR.into());
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(SIZE_ERROR.into());
    }
    let file = std::fs::File::open(path).map_err(|_| READ_ERROR.to_string())?;
    // `take` also bounds a file that grows after the size check.
    let reader = BufReader::new(file.take(MAX_FILE_BYTES));
    let mut reader = image::ImageReader::new(reader)
        .with_guessed_format()
        .map_err(|_| READ_ERROR.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| match e {
        image::ImageError::Limits(_) => DIMENSION_ERROR.to_string(),
        image::ImageError::IoError(_) => READ_ERROR.to_string(),
        _ => DECODE_ERROR.to_string(),
    })?;
    let rgba = decoded.into_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// The decoded overlay as a texture with its lat/lon box.
pub struct OverlayImage {
    texture: TextureHandle,
    bounds: GRect,
}

impl OverlayImage {
    /// Validate `bounds`, decode `path` and upload it (Decision 8). Blocks; the map decodes
    /// through [`OverlayLayer`] instead.
    pub fn load(path: &Path, bounds: GRect, ctx: &egui::Context) -> Result<OverlayImage, String> {
        validate_bounds(&bounds)?;
        let image = decode(path)?;
        Ok(Self::from_image(ctx, image, bounds))
    }

    /// Upload an image [`decode`] returned.
    pub fn from_image(ctx: &egui::Context, image: ColorImage, bounds: GRect) -> Self {
        Self {
            texture: ctx.load_texture("user-supplied-overlay", image, TextureOptions::LINEAR),
            bounds,
        }
    }

    pub fn texture(&self) -> &TextureHandle {
        &self.texture
    }

    pub fn bounds(&self) -> GRect {
        self.bounds
    }

    /// The box on the screen: the projected north-west and south-east corners, as the C#
    /// stretches the bitmap between `FromLatLngToLocal` of the two corners.
    pub fn screen_rect(&self, projector: &Projector<'_>) -> Rect {
        let b = &self.bounds;
        let lat = |l: f64| l.clamp(-MERCATOR_MAX_LAT, MERCATOR_MAX_LAT);
        let nw = projector.project(lon_lat(b.min_lon, lat(b.max_lat)));
        let se = projector.project(lon_lat(b.max_lon, lat(b.min_lat)));
        Rect::from_two_pos(nw.to_pos2(), se.to_pos2())
    }

    /// Paint the texture over its box at `opacity` (clamped to 0..=1), using the projector of
    /// the map layers. Only the visible part is drawn, so a deep zoom keeps the UV precise.
    pub fn draw(&self, painter: &egui::Painter, projector: &Projector<'_>, opacity: f32) {
        let rect = self.screen_rect(projector);
        let visible = rect.intersect(painter.clip_rect());
        if !rect.is_finite() || !visible.is_positive() {
            return;
        }
        let uv = |p: egui::Pos2| {
            pos2(
                (p.x - rect.min.x) / rect.width(),
                (p.y - rect.min.y) / rect.height(),
            )
        };
        let opacity = if opacity.is_nan() {
            0.0
        } else {
            opacity.clamp(0.0, 1.0)
        };
        painter.image(
            self.texture.id(),
            visible,
            Rect::from_min_max(uv(visible.min), uv(visible.max)),
            Color32::WHITE.gamma_multiply(opacity),
        );
    }
}

/// What a texture was loaded for.
#[derive(Clone, Debug, PartialEq)]
struct OverlayKey {
    path: PathBuf,
    bounds: GRect,
}

/// The overlay of the main map: the committed texture and the decode in flight.
#[derive(Default)]
pub struct OverlayLayer {
    committed: Option<OverlayImage>,
    /// The key last asked for, loaded or failed: a key is tried once until it changes.
    requested: Option<OverlayKey>,
    pending: Option<Receiver<Result<ColorImage, String>>>,
}

impl OverlayLayer {
    /// Follow the committed settings: start a decode when the key changed, take up a finished
    /// one, release the texture when the overlay is off or has no file. Returns a status text
    /// for a failure; the committed texture then stays.
    pub fn sync(&mut self, ctx: &egui::Context, g: &GuiSettings) -> Option<String> {
        let path = g
            .overlay_filename
            .as_ref()
            .filter(|p| !p.as_os_str().is_empty());
        let (true, Some(path)) = (g.overlay_enabled, path) else {
            *self = Self::default();
            return None;
        };
        let key = OverlayKey {
            path: path.clone(),
            bounds: bounds_of(g),
        };
        if self.requested.as_ref() != Some(&key) {
            self.requested = Some(key.clone());
            // A newer key supersedes a decode in flight; its result is dropped.
            self.pending = None;
            if let Err(e) = validate_bounds(&key.bounds) {
                return Some(e);
            }
            let (tx, rx) = mpsc::channel();
            let repaint = ctx.clone();
            let spawned = std::thread::Builder::new()
                .name("overlay-decode".into())
                .spawn(move || {
                    let _ = tx.send(decode(&key.path));
                    repaint.request_repaint();
                });
            if spawned.is_err() {
                return Some(WORKER_ERROR.into());
            }
            self.pending = Some(rx);
        }
        let rx = self.pending.as_ref()?;
        let result = match rx.try_recv() {
            Err(TryRecvError::Empty) => return None,
            Ok(result) => result,
            Err(TryRecvError::Disconnected) => Err(WORKER_ERROR.into()),
        };
        self.pending = None;
        match result {
            Ok(image) => {
                let bounds = self.requested.as_ref().map(|k| k.bounds)?;
                // Replacing the handle releases the previous texture.
                self.committed = Some(OverlayImage::from_image(ctx, image, bounds));
                None
            }
            Err(e) => Some(e),
        }
    }

    /// The committed overlay, if any.
    pub fn image(&self) -> Option<&OverlayImage> {
        self.committed.as_ref()
    }

    /// A decode is in flight.
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
}
