//! Map tile providers (seeded from `MapProvider.cs`) and the HTTP options of the tile loader.

use as_maps::{TileKey, TileStore};
use as_settings::Secret;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkers::sources::{Attribution, TileUrls};
use walkers::{
    HeaderValue, HttpOptions, HttpSource, SourceData, Stats, Style, StyledSources, TileData,
    TileId, TiledSource,
};

/// A slippy-map tile server.
///
/// `url` is a template with `{z}`, `{x}`, `{y}`, optionally `{s}` (subdomain a, b or c) and
/// `{apikey}`. The key lives apart in `api_key` and is substituted only by [`tile_url`], so
/// `Debug` output, status texts and the template never carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileProvider {
    pub name: &'static str,
    pub url: String,
    pub attribution: &'static str,
    /// Page the attribution links to.
    pub attribution_url: &'static str,
    pub max_zoom: u8,
    pub api_key: Secret,
}

/// Name of the Thunderforest provider of the original.
pub const THUNDERFOREST_NAME: &str = "OCM Landscape";

fn osm(name: &'static str, url: &str, attribution: &'static str, max_zoom: u8) -> TileProvider {
    TileProvider {
        name,
        url: url.into(),
        attribution,
        attribution_url: "https://www.openstreetmap.org/copyright",
        max_zoom,
        api_key: Secret::default(),
    }
}

/// OSM and OpenTopoMap, plus Thunderforest "OCM Landscape" if `key` is set (it needs a personal
/// API key, which only comes from the user settings).
pub fn providers(key: &Secret) -> Vec<TileProvider> {
    let mut list = vec![
        osm(
            "OpenStreetMap",
            "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
            "© OpenStreetMap contributors",
            19,
        ),
        osm(
            "OpenTopoMap",
            "https://{s}.tile.opentopomap.org/{z}/{x}/{y}.png",
            "Map © OpenTopoMap, data © OpenStreetMap contributors",
            17,
        ),
    ];
    if !key.is_empty() {
        list.push(TileProvider {
            name: THUNDERFOREST_NAME,
            url: "https://tile.thunderforest.com/landscape/{z}/{x}/{y}.png?apikey={apikey}".into(),
            attribution: "Maps © Thunderforest, Data © OpenStreetMap contributors",
            attribution_url: "https://www.thunderforest.com/",
            max_zoom: 22,
            api_key: key.clone(),
        });
    }
    list
}

/// The registrable domains of every provider's tile server (`openstreetmap.org`,
/// `opentopomap.org`, `thunderforest.com`), taken from [`providers`] so the list follows it.
/// The map preloader refuses a mirror on any of them (owner decision Q3).
pub fn provider_domains() -> Vec<String> {
    let mut list: Vec<String> = providers(&Secret::from("any"))
        .iter()
        .filter_map(|p| {
            let host = p.url.split("://").nth(1)?.split(['/', ':', '?']).next()?;
            let labels: Vec<&str> = host.split('.').filter(|l| !l.starts_with('{')).collect();
            (labels.len() >= 2).then(|| labels[labels.len() - 2..].join(".").to_lowercase())
        })
        .collect();
    list.dedup();
    list
}

/// The provider called `name`; an unknown name (or Thunderforest without a key) gives
/// OpenStreetMap.
pub fn provider_by_name(name: &str, key: &Secret) -> TileProvider {
    let mut list = providers(key);
    let i = list.iter().position(|p| p.name == name).unwrap_or(0);
    list.swap_remove(i)
}

/// Tile cache directory inside the data directory.
pub fn tiles_cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("tiles")
}

/// HTTP options of the tile loader: honest user agent, disk cache in `cache_dir`, default
/// parallelism (the OSM tile usage policy forbids a browser user agent and bulk downloads).
pub fn http_options(cache_dir: &Path) -> HttpOptions {
    HttpOptions {
        cache: Some(cache_dir.to_path_buf()),
        user_agent: HeaderValue::from_str(&as_net::user_agent()).ok(),
        ..HttpOptions::default()
    }
}

/// Fills a provider's URL template for one tile. The only place that inserts the API key.
pub fn tile_url(p: &TileProvider, x: u32, y: u32, zoom: u8) -> String {
    let sub = ["a", "b", "c"][(x as usize + y as usize) % 3];
    p.url
        .replace("{apikey}", p.api_key.expose())
        .replace("{s}", sub)
        .replace("{z}", &zoom.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string())
}

/// Adapter that lets walkers load tiles of a [`TileProvider`].
pub struct ProviderSource(pub TileProvider);

impl TileUrls for ProviderSource {
    fn tile_url(&self, id: TileId) -> String {
        tile_url(&self.0, id.x, id.y, id.zoom)
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: self.0.attribution,
            url: self.0.attribution_url,
            logo_light: None,
            logo_dark: None,
        }
    }

    fn max_zoom(&self) -> u8 {
        self.0.max_zoom
    }
}

/// The `as-maps` tile store inside the data directory (prefilled by `maptile-gen` or the
/// preloader; the map view only reads it).
pub fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("maptiles.gmdb")
}

/// Open the tile store of `data_dir` (created when missing). An error gives `None`, so the map
/// loads every tile over HTTP, and the text for the status bar.
pub fn open_store(data_dir: &Path) -> (Option<Arc<TileStore>>, Option<String>) {
    match TileStore::open(&store_path(data_dir)) {
        Ok(store) => (Some(Arc::new(store)), None),
        Err(e) => (None, Some(format!("map tile store: {e}"))),
    }
}

/// The map layer of provider `p`: tiles from `store` first, else over HTTP with the disk cache
/// in `cache_dir`.
// `Style` is a unit struct only while the walkers `vector` feature is off; `default()` builds
// in both cases.
#[allow(clippy::default_constructed_unit_structs)]
pub fn tile_layer(
    p: &TileProvider,
    store: Option<Arc<TileStore>>,
    cache_dir: &Path,
    ctx: &eframe::egui::Context,
) -> StyledSources {
    StyledSources::new(Style::default(), ctx.clone()).with_tiled_source(
        "",
        StoreFirst::new(
            store,
            as_maps::db_id(p.name),
            HttpSource::new(
                ProviderSource(p.clone()),
                http_options(cache_dir),
                ctx.clone(),
            ),
        ),
    )
}

/// A tile source that serves a tile from the `as-maps` store when it is there and asks
/// `fallback` (the HTTP source) otherwise.
///
/// Every store lookup (hit or miss) is memoised, so a frame does not query SQLite again for a
/// tile already looked up. A memoised miss stays a miss: a tile the preloader stores after the
/// map looked it up shows only after the memo is cleared (full), the provider is switched or the
/// app restarts. Accepted, since the HTTP source serves that tile meanwhile. A store error
/// (busy past the timeout, damaged file) counts as a miss for that tile and is reported once.
pub struct StoreFirst<F: TiledSource> {
    store: Option<Arc<TileStore>>,
    db_id: Option<i32>,
    fallback: F,
    /// The store's answer per tile: `Some` is the image, `None` asks `fallback`.
    memo: HashMap<TileId, Option<Arc<SourceData>>>,
    error_logged: bool,
}

/// Most tiles the memo holds; it is cleared when full.
const MEMO_CAP: usize = 1024;

impl<F: TiledSource> StoreFirst<F> {
    pub fn new(store: Option<Arc<TileStore>>, db_id: Option<i32>, fallback: F) -> Self {
        Self {
            store,
            db_id,
            fallback,
            memo: HashMap::new(),
            error_logged: false,
        }
    }

    /// The tile from the store; `None` without a store, for a provider without a GMap `DbId`,
    /// for a tile not stored, and on a store error (reported once, without tile bytes).
    fn lookup(&mut self, id: TileId) -> Option<Arc<SourceData>> {
        let (store, db_id) = (self.store.as_ref()?, self.db_id?);
        let key = TileKey {
            x: id.x,
            y: id.y,
            zoom: id.zoom,
            db_id,
        };
        match store.get(key) {
            Ok(bytes) => bytes.map(|b| Arc::new(SourceData::Encoded(b.into()))),
            Err(e) => {
                if !std::mem::replace(&mut self.error_logged, true) {
                    eprintln!("map tile store: {e}; tiles come from the tile server");
                }
                None
            }
        }
    }
}

impl<F: TiledSource> TiledSource for StoreFirst<F> {
    fn data(&mut self, tile_id: TileId) -> TileData {
        let hit = match self.memo.get(&tile_id) {
            Some(memoised) => memoised.clone(),
            None => {
                let found = self.lookup(tile_id);
                if self.memo.len() >= MEMO_CAP {
                    self.memo.clear();
                }
                self.memo.insert(tile_id, found.clone());
                found
            }
        };
        match hit {
            Some(data) => TileData::Ready(data),
            None => self.fallback.data(tile_id),
        }
    }

    fn attribution(&self) -> Attribution {
        self.fallback.attribution()
    }

    fn tile_size(&self) -> u32 {
        self.fallback.tile_size()
    }

    fn max_zoom(&self) -> u8 {
        self.fallback.max_zoom()
    }

    fn stats(&self) -> Stats {
        self.fallback.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use as_maps::{DBID_OPENTOPOMAP, DBID_OSM};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nstore tile";
    const ID: TileId = TileId {
        x: 34,
        y: 21,
        zoom: 6,
    };

    /// Fallback that records what it was asked and never fetches anything.
    #[derive(Default)]
    struct Fake {
        calls: Vec<TileId>,
    }

    impl TiledSource for Fake {
        fn data(&mut self, tile_id: TileId) -> TileData {
            self.calls.push(tile_id);
            TileData::Missing
        }

        fn attribution(&self) -> Attribution {
            Attribution {
                text: "fake",
                url: "https://example.invalid/",
                logo_light: None,
                logo_dark: None,
            }
        }
    }

    fn key(id: TileId, db_id: i32) -> TileKey {
        TileKey {
            x: id.x,
            y: id.y,
            zoom: id.zoom,
            db_id,
        }
    }

    /// A store in a temp dir holding `PNG` at `ID` for `db_id`.
    fn store_with(db_id: i32) -> (tempfile::TempDir, Arc<TileStore>) {
        let dir = tempfile::tempdir().unwrap();
        let store = TileStore::open(&store_path(dir.path())).unwrap();
        assert!(store.insert(key(ID, db_id), None, PNG).unwrap());
        (dir, Arc::new(store))
    }

    fn ready_bytes(d: &TileData) -> Option<Vec<u8>> {
        match d {
            TileData::Ready(s) => match s.as_ref() {
                SourceData::Encoded(b) => Some(b.to_vec()),
                _ => None,
            },
            _ => None,
        }
    }

    #[test]
    fn store_path_is_in_data_dir() {
        assert_eq!(
            store_path(Path::new("/data")),
            Path::new("/data").join("maptiles.gmdb")
        );
    }

    #[test]
    fn open_store_creates_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let (store, err) = open_store(dir.path());
        assert!(store.is_some() && err.is_none());
        assert!(store_path(dir.path()).is_file());
    }

    #[test]
    fn open_store_error_gives_none_and_text() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            store_path(dir.path()),
            b"not a database, but long enough to read",
        )
        .unwrap();
        let (store, err) = open_store(dir.path());
        assert!(store.is_none());
        let err = err.unwrap();
        assert!(err.starts_with("map tile store: "), "{err}");
        assert!(err.contains("maptiles.gmdb"), "{err}");
    }

    #[test]
    fn store_hit_is_ready_and_skips_fallback() {
        let (_dir, store) = store_with(DBID_OSM);
        let mut s = StoreFirst::new(Some(store), Some(DBID_OSM), Fake::default());
        assert_eq!(ready_bytes(&s.data(ID)).as_deref(), Some(PNG));
        assert!(s.fallback.calls.is_empty());
    }

    #[test]
    fn store_miss_asks_fallback() {
        let (_dir, store) = store_with(DBID_OSM);
        let mut s = StoreFirst::new(Some(store), Some(DBID_OSM), Fake::default());
        let other = TileId { x: 35, ..ID };
        assert!(matches!(s.data(other), TileData::Missing));
        assert_eq!(s.fallback.calls, vec![other]);
    }

    #[test]
    fn no_store_asks_fallback() {
        let mut s = StoreFirst::new(None, Some(DBID_OSM), Fake::default());
        assert!(matches!(s.data(ID), TileData::Missing));
        assert_eq!(s.fallback.calls, vec![ID]);
    }

    #[test]
    fn unknown_provider_asks_fallback() {
        let (_dir, store) = store_with(DBID_OSM);
        let mut s = StoreFirst::new(Some(store), as_maps::db_id("Unknown"), Fake::default());
        assert!(matches!(s.data(ID), TileData::Missing));
        assert_eq!(s.fallback.calls, vec![ID]);
    }

    #[test]
    fn store_error_falls_back() {
        let (dir, store) = store_with(DBID_OSM);
        // A second connection drops the tables: the next lookup is a "no such table" error.
        let c = rusqlite::Connection::open(store_path(dir.path())).unwrap();
        c.execute_batch("DROP TABLE TilesData; DROP TABLE Tiles;")
            .unwrap();
        let mut s = StoreFirst::new(Some(store), Some(DBID_OSM), Fake::default());
        assert!(matches!(s.data(ID), TileData::Missing));
        assert!(matches!(s.data(ID), TileData::Missing));
        assert_eq!(s.fallback.calls, vec![ID, ID]);
        assert!(s.error_logged);
    }

    #[test]
    fn lookup_is_memoised() {
        let (dir, store) = store_with(DBID_OSM);
        let mut s = StoreFirst::new(Some(store), Some(DBID_OSM), Fake::default());
        assert_eq!(ready_bytes(&s.data(ID)).as_deref(), Some(PNG));
        // Gone from the file: the second call must not ask SQLite again.
        let c = rusqlite::Connection::open(store_path(dir.path())).unwrap();
        c.execute_batch("DELETE FROM TilesData; DELETE FROM Tiles;")
            .unwrap();
        assert_eq!(ready_bytes(&s.data(ID)).as_deref(), Some(PNG));
        assert!(s.fallback.calls.is_empty());
    }

    #[test]
    fn memo_is_cleared_when_full() {
        let (dir, store) = store_with(DBID_OSM);
        let mut s = StoreFirst::new(Some(store), Some(DBID_OSM), Fake::default());
        assert!(ready_bytes(&s.data(ID)).is_some());
        let c = rusqlite::Connection::open(store_path(dir.path())).unwrap();
        c.execute_batch("DELETE FROM TilesData; DELETE FROM Tiles;")
            .unwrap();
        for x in 0..MEMO_CAP as u32 {
            let _ = s.data(TileId { x, y: 0, zoom: 12 });
        }
        assert!(s.memo.len() <= MEMO_CAP);
        // The hit was evicted with the rest, so the deleted tile is now looked up again.
        assert!(matches!(s.data(ID), TileData::Missing));
    }

    #[test]
    fn type_must_match() {
        let (_dir, store) = store_with(DBID_OPENTOPOMAP);
        let mut s = StoreFirst::new(Some(store), Some(DBID_OSM), Fake::default());
        assert!(matches!(s.data(ID), TileData::Missing));
        assert_eq!(s.fallback.calls, vec![ID]);
    }

    #[test]
    fn metadata_comes_from_fallback() {
        let s = StoreFirst::new(None, None, Fake::default());
        assert_eq!(s.attribution().text, "fake");
        assert_eq!(s.tile_size(), 256);
        assert_eq!(s.max_zoom(), 19);
        assert_eq!(s.stats().in_progress, 0);
    }
}
