//! Map preloader (`MapPreloader`, `MapPreloader.cs:104-262`): fills the tile store
//! (`<data>/maptiles.gmdb`) with the OpenStreetMap tiles of the settings area, zoom 5 down to
//! `map_preloader_max_zoom`, from the airscout.eu mirror only (owner decision Q3), off by default.
//!
//! As C#: the list in loop order (z, x, y), the tiles already stored dropped, a Fisher-Yates
//! shuffle, one tile at a time with 100 ms between them, the C# status texts. Port deviations:
//! - the store check and the write go to one store with the OpenStreetMap `DbId` (S18: C#
//!   checks through ScoutBase's `MapData.Database`, `MapPreloader.cs:159`, and writes through
//!   GMap's asynchronous cache queue, `:92`; the two meet only because both default to the same
//!   `Data.gmdb`);
//! - the lowest zoom is 5 and the highest the setting (C# hard-codes 11 and ignores
//!   `Map_Preloader_MaxZoom`, `MapPreloader.cs:145-146`);
//! - another map provider gets a notice and nothing (C# pulls from the provider's own server,
//!   `MapPreloader.cs:196-203`);
//! - a mirror answer is stored only when it is a 200 with at most `MAX_TILE_BYTES` of PNG or JPEG;
//!   a redirect is followed only on the mirror's host;
//! - a mirror URL on a map provider's domain (`tile.openstreetmap.org` and the others of
//!   `tiles::providers`) is refused with one notice and no request.

use crate::background::sleep_cancellable;
use as_maps::{is_png_or_jpeg, TileKey, TileStore, DBID_OSM, MAX_TILE_BYTES};
use as_settings::{Settings, MAP_PRELOADER_MAX_ZOOM};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// `zmin` of `MapPreloader.cs:145`.
pub const PRELOAD_MIN_ZOOM: u8 = 5;
/// `Thread.Sleep(100)` between two tiles (`MapPreloader.cs:214`).
pub const PRELOAD_DELAY: Duration = Duration::from_millis(100);
/// Per tile: a stalled mirror does not hold the run for the updaters' 300 s.
pub const PRELOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// The only provider the mirror holds (`MapPreloader.cs:196`).
pub const MIRROR_PROVIDER: &str = "OpenStreetMap";

/// Counts of one run. `left = missing - (stored + failed)`: 0 when the run reached the end, as
/// C# `count - i` (`MapPreloader.cs:259`); `failed` tiles are counted on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreloadOutcome {
    /// Tiles of the area (C# `total`).
    pub total: usize,
    /// Of them not in the store (C# `count`).
    pub missing: usize,
    pub stored: usize,
    pub failed: usize,
    pub left: usize,
}

/// Whether the background service starts the preloader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreloadGate {
    /// Off, or no GLOBE / SRTM3 / SRTM1 model enabled.
    Skip,
    /// Wanted, but the provider (named) has no mirror: one notice, nothing loaded.
    NoMirror(String),
    /// The mirror URL points at a map provider's own server (the host named): one notice,
    /// nothing loaded (owner decision Q3).
    ProviderMirror(String),
    Run {
        max_zoom: u8,
    },
}

/// `Map_Preloader_Enabled` (`MapDlg.cs:2288`, `:2393`), then C# returns at once without one of
/// GLOBE, SRTM3, SRTM1 (`MapPreloader.cs:115-123`), then the mirror's provider only, then a
/// mirror URL that is no provider's server.
pub fn preload_gate(s: &Settings) -> PreloadGate {
    let e = &s.elevation_enabled;
    if !s.gui.map_preloader_enabled || !(e.globe || e.srtm3 || e.srtm1) {
        PreloadGate::Skip
    } else if s.gui.map_provider != MIRROR_PROVIDER {
        PreloadGate::NoMirror(s.gui.map_provider.clone())
    } else if let Some(host) = provider_host(&s.background.map_mirror_url) {
        PreloadGate::ProviderMirror(host)
    } else {
        PreloadGate::Run {
            max_zoom: s.gui.map_preloader_max_zoom.min(MAP_PRELOADER_MAX_ZOOM),
        }
    }
}

/// The host of `mirror_url` when it is, or lies under, a map provider's domain
/// ([`crate::tiles::provider_domains`]): a hand-edited `map_mirror_url` must not turn the bulk
/// preloader on a provider's own server (owner decision Q3). A URL without a host is left to
/// fail per tile, as before.
fn provider_host(mirror_url: &str) -> Option<String> {
    let url = url::Url::parse(mirror_url).ok()?;
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    crate::tiles::provider_domains()
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
        .then_some(host)
}

/// Whether a bulk download of the map tiles of `provider` may use its mirror: the provider is
/// the one the mirror holds ([`MIRROR_PROVIDER`]) and `mirror_url` is not a map provider's own
/// server. The error is the notice to show ([`no_mirror_text`] / [`provider_mirror_text`]).
pub fn map_mirror_ok(provider: &str, mirror_url: &str) -> Result<(), String> {
    if provider != MIRROR_PROVIDER {
        Err(no_mirror_text(provider))
    } else if let Some(host) = provider_host(mirror_url) {
        Err(provider_mirror_text(&host))
    } else {
        Ok(())
    }
}

/// The preloader's HTTP client: the honest user agent of `services::client()` (not C#'s
/// "AirScout", `MapPreloader.cs:125`), 30 s per tile, redirects only on the same host.
pub fn preloader_client() -> Result<reqwest::blocking::Client, String> {
    let same_host = reqwest::redirect::Policy::custom(|a| {
        let from = a
            .previous()
            .first()
            .and_then(|u| u.host_str().map(str::to_owned));
        if a.previous().len() < 5 && from.as_deref() == a.url().host_str() {
            a.follow()
        } else {
            a.stop()
        }
    });
    reqwest::blocking::Client::builder()
        .user_agent(as_net::user_agent())
        .timeout(PRELOAD_TIMEOUT)
        .redirect(same_host)
        .build()
        .map_err(|e| format!("cannot create HTTP client: {e}"))
}

/// The C# end text (`MapPreloader.cs:259`, two spaces after "total").
pub fn finished_text(o: &PreloadOutcome) -> String {
    format!(
        "MapPreloader finished, total  {} tile(s), {} left.",
        o.total, o.left
    )
}

/// The notice for a provider without mirror.
pub fn no_mirror_text(provider: &str) -> String {
    format!("MapPreloader: no mirror for provider {provider}, nothing preloaded.")
}

/// The notice for a mirror URL on a map provider's server.
pub fn provider_mirror_text(host: &str) -> String {
    format!("MapPreloader: {host} is a map provider's server, not a mirror; nothing preloaded.")
}

/// Fisher-Yates as `MapPreloader.cs:177-187`: `n` from the length down to 2, swap `n-1` with a
/// random `k` in `0..=n-1`.
fn shuffle<T>(list: &mut [T], rng: &mut fastrand::Rng) {
    let mut n = list.len();
    while n > 1 {
        n -= 1;
        let k = rng.usize(0..=n);
        list.swap(k, n);
    }
}

/// GET one tile: the body if it is a 200 with at most [`MAX_TILE_BYTES`] of PNG or JPEG. The
/// body is read through a cap, so a big answer is never held whole.
fn fetch(client: &reqwest::blocking::Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().ok()?;
    if resp.status() != reqwest::StatusCode::OK
        || resp
            .content_length()
            .is_some_and(|n| n > MAX_TILE_BYTES as u64)
    {
        return None;
    }
    let mut body = Vec::new();
    resp.take(MAX_TILE_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .ok()?;
    (body.len() <= MAX_TILE_BYTES && is_png_or_jpeg(&body)).then_some(body)
}

/// One run of `OnDoWork` (`MapPreloader.cs:142-236`) over `list` (`(z, x, y)` in loop order):
/// drop the stored tiles, shuffle, GET `<base_url>/<z>/<x>/<y>.png` one by one with `delay`
/// between them. A bad answer counts as failed and the run goes on; a store error ends it (as
/// the C# `catch`, `:233-236`) with the counts so far. `None` when `cancel` is set: checked during
/// the store scan, before each GET and before a tile is written.
pub fn preload_run(
    store: &TileStore,
    client: &reqwest::blocking::Client,
    base_url: &str,
    list: Vec<(u8, u32, u32)>,
    delay: Duration,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(String),
) -> Option<PreloadOutcome> {
    let cancelled = || cancel.load(Ordering::Relaxed);
    progress("MapPreloader getting tiles from database.".into());
    let mut o = PreloadOutcome {
        total: list.len(),
        missing: 0,
        stored: 0,
        failed: 0,
        left: 0,
    };
    let mut missing = Vec::new();
    for (zoom, x, y) in list {
        if cancelled() {
            return None;
        }
        let k = TileKey {
            x,
            y,
            zoom,
            db_id: DBID_OSM,
        };
        match store.exists(k) {
            Ok(true) => {}
            Ok(false) => missing.push(k),
            Err(e) => {
                progress(format!("MapPreloader: {e}"));
                return Some(o);
            }
        }
    }
    o.missing = missing.len();
    shuffle(&mut missing, &mut fastrand::Rng::new());
    let base = base_url.trim_end_matches('/');
    for k in missing {
        let path = format!("/{}/{}/{}.png", k.zoom, k.x, k.y);
        progress(format!("Preloading {path}"));
        if cancelled() {
            return None;
        }
        let body = fetch(client, &format!("{base}{path}"));
        if cancelled() {
            return None;
        }
        match body {
            Some(b) => match store.insert(k, Some(chrono::Local::now().naive_local()), &b) {
                // `false`: written meanwhile by someone else, so it is there
                Ok(_) => o.stored += 1,
                Err(e) => {
                    progress(format!("MapPreloader: {e}"));
                    o.left = o.missing - o.stored - o.failed;
                    return Some(o);
                }
            },
            None => {
                o.failed += 1;
                progress(format!("MapPreloader: failed {path}"));
            }
        }
        if sleep_cancellable(delay, cancel) {
            return None;
        }
    }
    Some(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// A PNG signature and a few bytes: the preloader stores what the mirror sends, it does not
    /// decode it.
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nTEST-TILE";

    fn temp_store() -> (tempfile::TempDir, TileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = TileStore::open(&dir.path().join("maptiles.gmdb")).unwrap();
        (dir, store)
    }

    fn key(z: u8, x: u32, y: u32) -> TileKey {
        TileKey {
            x,
            y,
            zoom: z,
            db_id: DBID_OSM,
        }
    }

    /// Run with no delay and no cancel; the progress lines are returned too.
    fn run(
        store: &TileStore,
        url: &str,
        list: Vec<(u8, u32, u32)>,
    ) -> (Option<PreloadOutcome>, Vec<String>) {
        let client = preloader_client().unwrap();
        let cancel = AtomicBool::new(false);
        let mut lines = Vec::new();
        let o = preload_run(
            store,
            &client,
            url,
            list,
            Duration::ZERO,
            &cancel,
            &mut |m| lines.push(m),
        );
        (o, lines)
    }

    #[test]
    fn preload_downloads_missing_tiles_only() {
        let mut server = mockito::Server::new();
        let a = server
            .mock("GET", "/5/16/10.png")
            .with_body(PNG)
            .expect(1)
            .create();
        let b = server
            .mock("GET", "/6/33/21.png")
            .with_body(PNG)
            .expect(1)
            .create();
        let stored = server.mock("GET", "/5/17/10.png").expect(0).create();
        let (_dir, store) = temp_store();
        assert!(store.insert(key(5, 17, 10), None, PNG).unwrap());
        // the mirror URL without a trailing slash (mockito's) works as well as with one
        let (o, lines) = run(
            &store,
            &server.url(),
            vec![(5, 16, 10), (5, 17, 10), (6, 33, 21)],
        );
        a.assert();
        b.assert();
        stored.assert();
        assert_eq!(
            o,
            Some(PreloadOutcome {
                total: 3,
                missing: 2,
                stored: 2,
                failed: 0,
                left: 0,
            })
        );
        assert_eq!(lines[0], "MapPreloader getting tiles from database.");
        for k in [key(5, 16, 10), key(6, 33, 21)] {
            assert_eq!(store.get(k).unwrap().as_deref(), Some(PNG), "{k:?}");
        }
        // every row is an OpenStreetMap tile, with a cache time
        assert_eq!(store.types().unwrap(), vec![(DBID_OSM, 3)]);
        let infos = store.list().unwrap();
        assert!(infos
            .iter()
            .filter(|i| i.key != key(5, 17, 10))
            .all(|i| i.cache_time.is_some()));
    }

    #[test]
    fn preload_url_with_trailing_slash() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", "/osm/7/1/2.png")
            .with_body(PNG)
            .expect(1)
            .create();
        let (_dir, store) = temp_store();
        let (o, _) = run(&store, &format!("{}/osm/", server.url()), vec![(7, 1, 2)]);
        m.assert();
        assert_eq!(o.unwrap().stored, 1);
    }

    #[test]
    fn preload_sends_honest_user_agent() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", "/5/1/1.png")
            .match_header("user-agent", as_net::user_agent().as_str())
            .with_body(PNG)
            .expect(1)
            .create();
        let (_dir, store) = temp_store();
        let (o, _) = run(&store, &server.url(), vec![(5, 1, 1)]);
        m.assert();
        assert_eq!(o.unwrap().stored, 1);
    }

    #[test]
    fn preload_skips_bad_answers() {
        let mut server = mockito::Server::new();
        let _m404 = server.mock("GET", "/5/1/1.png").with_status(404).create();
        let _html = server
            .mock("GET", "/5/1/2.png")
            .with_body("<html><body>Not here</body></html>")
            .create();
        let _m500 = server
            .mock("GET", "/5/1/3.png")
            .with_status(500)
            .with_body(PNG)
            .create();
        let good = server
            .mock("GET", "/5/1/4.png")
            .with_body(PNG)
            .expect(1)
            .create();
        let (_dir, store) = temp_store();
        let (o, _) = run(
            &store,
            &server.url(),
            vec![(5, 1, 1), (5, 1, 2), (5, 1, 3), (5, 1, 4)],
        );
        // the run goes on after each bad answer and reaches the last tile
        good.assert();
        assert_eq!(
            o,
            Some(PreloadOutcome {
                total: 4,
                missing: 4,
                stored: 1,
                failed: 3,
                left: 0,
            })
        );
        assert_eq!(store.count().unwrap(), 1);
        assert_eq!(store.get(key(5, 1, 4)).unwrap().as_deref(), Some(PNG));
    }

    #[test]
    fn preload_skips_unreachable_mirror() {
        // a port nothing listens on: connection refused, counted as failed
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let (_dir, store) = temp_store();
        let (o, _) = run(
            &store,
            &format!("http://127.0.0.1:{port}/"),
            vec![(5, 1, 1)],
        );
        assert_eq!(o.unwrap().failed, 1);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn preload_caps_body() {
        let mut server = mockito::Server::new();
        let mut big = PNG.to_vec();
        big.resize(MAX_TILE_BYTES + 1, 0);
        let _m = server.mock("GET", "/5/1/1.png").with_body(&big).create();
        // the same without Content-Length: the read stops after the cap
        let _chunked = server
            .mock("GET", "/5/1/2.png")
            .with_chunked_body(move |w| {
                w.write_all(PNG)?;
                for _ in 0..(MAX_TILE_BYTES / 4096 + 2) {
                    w.write_all(&[0; 4096])?;
                }
                Ok(())
            })
            .create();
        let mut exact = PNG.to_vec();
        exact.resize(MAX_TILE_BYTES, 0);
        let _ok = server.mock("GET", "/5/1/3.png").with_body(&exact).create();
        let (_dir, store) = temp_store();
        let (o, _) = run(&store, &server.url(), vec![(5, 1, 1), (5, 1, 2), (5, 1, 3)]);
        let o = o.unwrap();
        assert_eq!((o.stored, o.failed), (1, 2), "{o:?}");
        assert!(store.get(key(5, 1, 1)).unwrap().is_none());
        assert!(store.get(key(5, 1, 2)).unwrap().is_none());
        assert_eq!(
            store.get(key(5, 1, 3)).unwrap().map(|b| b.len()),
            Some(MAX_TILE_BYTES)
        );
    }

    /// A redirect to another host is not followed: the preloader talks to the mirror only.
    #[test]
    fn preload_refuses_redirect_to_other_host() {
        let mut other = mockito::Server::new();
        let elsewhere = other.mock("GET", mockito::Matcher::Any).expect(0).create();
        let mut server = mockito::Server::new();
        // `localhost` is another host name than the mirror's 127.0.0.1
        let port = other
            .host_with_port()
            .rsplit(':')
            .next()
            .unwrap()
            .to_string();
        let _m = server
            .mock("GET", "/5/1/1.png")
            .with_status(302)
            .with_header("location", &format!("http://localhost:{port}/5/1/1.png"))
            .create();
        let _same = server
            .mock("GET", "/5/1/2.png")
            .with_status(301)
            .with_header("location", "/moved/5/1/2.png")
            .create();
        let moved = server
            .mock("GET", "/moved/5/1/2.png")
            .with_body(PNG)
            .expect(1)
            .create();
        let (_dir, store) = temp_store();
        let (o, _) = run(&store, &server.url(), vec![(5, 1, 1), (5, 1, 2)]);
        elsewhere.assert();
        moved.assert();
        let o = o.unwrap();
        assert_eq!((o.stored, o.failed), (1, 1), "{o:?}");
    }

    #[test]
    fn preload_stops_on_cancel() {
        let mut server = mockito::Server::new();
        let m = server
            .mock("GET", mockito::Matcher::Regex(r"^/5/\d+/\d+\.png$".into()))
            .with_body(PNG)
            .expect_at_most(1)
            .create();
        let (_dir, store) = temp_store();
        let client = preloader_client().unwrap();
        let cancel = AtomicBool::new(false);
        let mut seen = 0;
        let list = (0..5).map(|y| (5, 3, y)).collect();
        let o = preload_run(
            &store,
            &client,
            &server.url(),
            list,
            Duration::ZERO,
            &cancel,
            &mut |m| {
                if m.starts_with("Preloading ") {
                    seen += 1;
                    // after the first tile
                    if seen == 2 {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            },
        );
        assert_eq!(o, None);
        m.assert();
        assert!(store.count().unwrap() <= 1);
    }

    /// Cancelled during the scan of the store: no request at all.
    #[test]
    fn preload_cancel_during_scan_requests_nothing() {
        let mut server = mockito::Server::new();
        let m = server.mock("GET", mockito::Matcher::Any).expect(0).create();
        let (_dir, store) = temp_store();
        let client = preloader_client().unwrap();
        let cancel = AtomicBool::new(false);
        let o = preload_run(
            &store,
            &client,
            &server.url(),
            vec![(5, 1, 1)],
            Duration::ZERO,
            &cancel,
            &mut |m| {
                if m == "MapPreloader getting tiles from database." {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        assert_eq!(o, None);
        m.assert();
    }

    #[test]
    fn preload_order_is_shuffled_but_complete() {
        let mut server = mockito::Server::new();
        let list: Vec<(u8, u32, u32)> = (0..20).map(|y| (6, 7, y)).collect();
        let mocks: Vec<_> = list
            .iter()
            .map(|&(z, x, y)| {
                server
                    .mock("GET", format!("/{z}/{x}/{y}.png").as_str())
                    .with_body(PNG)
                    .expect(1)
                    .create()
            })
            .collect();
        let (_dir, store) = temp_store();
        let (o, lines) = run(&store, &server.url(), list.clone());
        for m in &mocks {
            m.assert();
        }
        assert_eq!(o.unwrap().stored, 20);
        let order: Vec<String> = lines
            .iter()
            .filter(|l| l.starts_with("Preloading "))
            .cloned()
            .collect();
        let input: Vec<String> = list
            .iter()
            .map(|(z, x, y)| format!("Preloading /{z}/{x}/{y}.png"))
            .collect();
        let (mut a, mut b) = (order.clone(), input.clone());
        a.sort();
        b.sort();
        assert_eq!(a, b, "every tile once");
        // 1 in 20! that a shuffle keeps the order
        assert_ne!(order, input, "not shuffled");
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut rng = fastrand::Rng::with_seed(7);
        for n in [0usize, 1, 2, 50] {
            let mut v: Vec<usize> = (0..n).collect();
            shuffle(&mut v, &mut rng);
            let mut s = v.clone();
            s.sort_unstable();
            assert_eq!(s, (0..n).collect::<Vec<_>>());
            if n == 50 {
                assert_ne!(v, (0..n).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn status_texts_are_the_csharp_ones() {
        let o = PreloadOutcome {
            total: 74_916,
            missing: 10,
            stored: 4,
            failed: 2,
            left: 4,
        };
        // two spaces after "total" as in MapPreloader.cs:259
        assert_eq!(
            finished_text(&o),
            "MapPreloader finished, total  74916 tile(s), 4 left."
        );
        assert_eq!(
            no_mirror_text("OpenTopoMap"),
            "MapPreloader: no mirror for provider OpenTopoMap, nothing preloaded."
        );
    }

    fn wanted() -> Settings {
        let mut s = Settings::default();
        s.gui.map_preloader_enabled = true;
        s
    }

    #[test]
    fn disabled_by_default() {
        assert_eq!(preload_gate(&Settings::default()), PreloadGate::Skip);
        assert_eq!(preload_gate(&wanted()), PreloadGate::Run { max_zoom: 11 });
    }

    #[test]
    fn needs_osm_provider() {
        let mut s = wanted();
        s.gui.map_provider = "OpenTopoMap".into();
        assert_eq!(
            preload_gate(&s),
            PreloadGate::NoMirror("OpenTopoMap".into())
        );
        // switched off: not even the notice
        s.gui.map_preloader_enabled = false;
        assert_eq!(preload_gate(&s), PreloadGate::Skip);
    }

    /// C# returns at once without GLOBE, SRTM3 or SRTM1 (MapPreloader.cs:115-123).
    #[test]
    fn needs_an_elevation_model() {
        let mut s = wanted();
        s.elevation_enabled = as_settings::ElevationEnabled {
            globe: false,
            srtm3: false,
            srtm1: false,
            aster3: true,
            aster1: true,
        };
        assert_eq!(preload_gate(&s), PreloadGate::Skip);
        for m in 0..3 {
            let mut t = s.clone();
            match m {
                0 => t.elevation_enabled.globe = true,
                1 => t.elevation_enabled.srtm3 = true,
                _ => t.elevation_enabled.srtm1 = true,
            }
            assert_eq!(preload_gate(&t), PreloadGate::Run { max_zoom: 11 }, "{m}");
        }
    }

    /// Owner Q3: never a bulk download from a provider's own server, even when a hand-edited
    /// `map_mirror_url` points there. The gate refuses it, so no client is built and no request
    /// goes out (pure check: no server is involved).
    #[test]
    fn refuses_provider_hosts_as_mirror() {
        let domains = crate::tiles::provider_domains();
        for d in ["openstreetmap.org", "opentopomap.org", "thunderforest.com"] {
            assert!(domains.iter().any(|x| x == d), "{d} in {domains:?}");
        }
        for (url, host) in [
            ("http://tile.openstreetmap.org/", "tile.openstreetmap.org"),
            (
                "https://tile.openstreetmap.org/osm/",
                "tile.openstreetmap.org",
            ),
            (
                "http://a.tile.openstreetmap.org/",
                "a.tile.openstreetmap.org",
            ),
            ("https://openstreetmap.org/", "openstreetmap.org"),
            (
                "http://www.openstreetmap.org/tiles/",
                "www.openstreetmap.org",
            ),
            ("https://b.tile.opentopomap.org/", "b.tile.opentopomap.org"),
            ("https://opentopomap.org/", "opentopomap.org"),
            (
                "https://tile.thunderforest.com/landscape/",
                "tile.thunderforest.com",
            ),
            ("http://thunderforest.com/", "thunderforest.com"),
            ("http://TILE.OpenStreetMap.ORG./", "tile.openstreetmap.org"),
            (
                "http://tile%2Eopenstreetmap%2Eorg/",
                "tile.openstreetmap.org",
            ),
            (
                "http://mirror@tile.openstreetmap.org:8080/",
                "tile.openstreetmap.org",
            ),
        ] {
            let mut s = wanted();
            s.background.map_mirror_url = url.into();
            assert_eq!(
                preload_gate(&s),
                PreloadGate::ProviderMirror(host.into()),
                "{url}"
            );
        }
        assert_eq!(
            provider_mirror_text("tile.openstreetmap.org"),
            "MapPreloader: tile.openstreetmap.org is a map provider's server, not a mirror; \
             nothing preloaded."
        );
        // the real mirror and look-alike names that are no provider's domain still run
        for url in [
            as_maps::MIRROR_URL,
            "http://notopenstreetmap.org/osm/",
            "http://openstreetmap.org.mirror.test/osm/",
            "http://127.0.0.1:1234/osm/",
        ] {
            let mut s = wanted();
            s.background.map_mirror_url = url.into();
            assert_eq!(preload_gate(&s), PreloadGate::Run { max_zoom: 11 }, "{url}");
        }
    }

    #[test]
    fn max_zoom_clamped() {
        let mut s = wanted();
        s.gui.map_preloader_max_zoom = 30;
        assert_eq!(preload_gate(&s), PreloadGate::Run { max_zoom: 11 });
        s.gui.map_preloader_max_zoom = 7;
        assert_eq!(preload_gate(&s), PreloadGate::Run { max_zoom: 7 });
    }
}
