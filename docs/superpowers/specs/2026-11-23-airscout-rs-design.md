# AirScout-RS — Design

Date: 2026-11-23
Status: approved 2026-11-23

## Intent

Rewrite AirScout (https://github.com/dl2alf/AirScout, C#/.NET WinForms, Windows-centric) in Rust
so it runs natively on macOS, Linux and Windows. The user wants the **whole suite**, not a subset.
After a faithful port, review and fix the prediction logic, which reportedly contains errors.

AirScout predicts and monitors aircraft-scatter radio paths between two amateur radio stations:
it holds a global elevation model, ingests live aircraft positions, computes the propagation path
and the zone of mutual visibility, and shows which aircraft are usable for a QSO now and soon.

## Goals

- Feature parity with the original suite, delivered in stages (see Stages).
- One native binary per tool per OS. No .NET, no Mono, no Wine.
- Prediction core is a pure, tested Rust library usable without the GUI.
- Behaviour of the original core captured in golden tests **before** any logic fix, so every fix
  is a visible, reviewable diff in results.

## Non-goals

- Pixel-identical WinForms UI. Layout follows the original's information architecture, not its look.
- Binary compatibility with the original SQLite databases or settings files (import is nice-to-have,
  see "Left to the builder").
- Porting vendored third-party libraries (GMap.NET, OxyPlot, BouncyCastle, DotNetZip, AeroWizard,
  NDDE, CustomScrollBar, AquaGauge, CubicSpline, OrbitTools, SerializableGenerics). Use crates.

## Anti-goals

- Silently "fixing" logic during the port. Port first, fix later, each fix separately documented.
- A UI that stutters with hundreds of live aircraft.
- Platform-specific features (DDE, OmniRig COM) leaking into the shared code; they live behind
  `cfg(windows)` and the app works fully without them.

## Constraints

- Source of truth: upstream repo at the latest commit at time of port (cloned shallow).
  Own code ≈ 55–60k lines C#: ScoutBase (~23k), AirScout GUI (~20k, MapDlg.cs alone 10k),
  aircraft DB/positions (~6k), 12 plane-feed plugins (~1k each), LibADSB (~2.5k), CAT (~3k),
  plus PlaneServer, ViewClient, generators, RainScout.
- Elevation data: original downloads pre-built tiles from
  `http://www.airscout.eu/downloads/ScoutBase/1/ElevationData/{GLOBE,SRTM3,SRTM1,ASTER3,ASTER1}`
  (directory listing returns 403; individual files unverified). Aircraft DB JSONs at
  `.../downloads/database/*.json` return **404**. The Rust port must not depend solely on
  airscout.eu: support raw public sources (GLOBE, SRTM/Copernicus DEM, OpenSky/ADS-B DBs) as fallback.
- License: upstream ships a LICENSE in `AirScout/`; the port keeps attribution to DL2ALF and the
  same license terms (builder to confirm license text before first public push).

## Approaches considered (GUI)

- **A) egui/eframe — chosen by the user.** Pure Rust, single binary, immediate-mode redraw suits
  hundreds of moving aircraft. Map via `walkers` (OSM slippy tiles), charts via `egui_plot`.
  Trade-off: map widget less mature than Leaflet; path/zone overlays drawn by us.
- B) Tauri + Leaflet/MapLibre. Best map, but two languages and per-OS webviews (WebKitGTK issues).
- C) Slint / iced. Pure Rust, but no usable map component; most work. Rejected.

## Architecture

Cargo workspace, crates prefixed `as-`:

| Crate | Ports | Notes |
|---|---|---|
| `as-geo` | ScoutBase.Core (LatLon, MaidenheadLocator, geodesy, units) | no I/O, `#![forbid(unsafe_code)]` |
| `as-elevation` | ScoutBase.Elevation | tile model, catalogue, downloader/updater, local obstructions, horizon, elevation path, SQLite cache |
| `as-propagation` | ScoutBase.Propagation | PropagationPath, Horizon, IntersectionPoint, settings, path cache |
| `as-stations` | ScoutBase.Stations | call/locator DB, QRV per band |
| `as-aircraft` | AirScout.Aircrafts, AirScout.AircraftPositions, AirScout.Core | aircraft/type/airline/airport DB, position history, PlaneInfo, categories |
| `as-adsb` | LibADSB | Mode-S/ADS-B decoding |
| `as-feeds` | AirScout.PlaneFeeds + all 11 plugins (+Template) | `PlaneFeed` trait; one module per feed; async (tokio + reqwest) |
| `as-maps` | ScoutBase.Maps | offline OSM tile cache in SQLite, feeds `walkers` tile source; used by `maptile-gen` |
| `as-signals` | AirScout.Signals | signal level DB |
| `as-cat` | AirScout.CAT, ScoutBase.CAT, HamLibSharp | rigctld TCP (all OS), optional hamlib FFI, OmniRig only `cfg(windows)` |
| `as-wintest` | WinTest.cs | UDP Win-Test protocol |
| `as-dde` | NDDE usage | `cfg(windows)` only |
| `as-web` | AirScout/Webserver.cs + wwwroot | embedded HTTP server (axum), serves the existing web UI assets |
| `as-db` | ScoutBase.Database, SQLiteDatabase | rusqlite wrapper, migrations |
| `as-settings` | Settings.cs, BandSettings | serde + TOML in OS config dir (`directories` crate) |
| `airscout` (bin) | AirScout GUI | eframe app: map, path, plane list, profile/horizon charts, all dialogs, first-run wizard |
| `airscout-planeserver` (bin) | AirScoutPlaneServer | headless daemon or egui window with tray (owner Q1) |
| `airscout-viewclient` (bin) | AirScoutViewClient | eframe |
| `maptile-gen`, `elevation-tile-gen` (bins) | MapTileGenerator, ElevationTileGenerator | CLI (clap) |
| `rainscout` (bin) + `rainscout-core`, `rainscout-radars` | RainScout* , Radars | eframe |
| `airscout-cli` subcommands `horizon-gen`, `elevation-coverage`, `wintest-sim` (as-cli) | HorizonGenerator, ElevationCoverageMapper, WinTestSimulator | CLI (clap); owner decision Q6, 2026-11-24 |
| `catcheck` (bin) | CATCheck | CLI over as-cat (rigctld, hamlib, OmniRig) |

Concurrency: background workers (feeds, path calculator, elevation download, CAT, Win-Test) run on
tokio; GUI reads shared state via `Arc<RwLock<…>>`/channels and requests repaint. CPU-heavy path
and horizon calculations use rayon.

Cross-platform: CI on GitHub Actions builds and tests macOS (arm64+x64), Linux x64, Windows x64.
Release artefacts: `.dmg`/`.app`, `.AppImage` + tarball, `.msi`/zip (cargo-dist or cargo-bundle).

## Stages

Each stage ends with green CI on all three OS and a usable artefact.

1. **Core port + golden tests.** `as-geo`, `as-elevation`, `as-propagation`, `as-stations`, `as-db`,
   `as-settings`. Build a small C# harness (run under .NET on macOS/Linux, netstandard-compatible
   ScoutBase subset) that dumps original results (distances, bearings, locator conversions, path
   points, horizons, intersection points, mutual-visibility zones) for a fixed set of station pairs
   and bands to JSON. Rust tests compare against these with tight tolerances. If the C# code cannot
   be run headless, transcribe the formulas and hand-compute fixtures instead (builder decides, notes why).
2. **Aircraft + feeds.** `as-aircraft`, `as-adsb`, all 11 feeds, `as-signals`. Recorded HTTP
   fixtures for each feed; live smoke test behind an ignored test.
3. **Main GUI.** Map with stations, path, visibility zone, live aircraft with predicted crossing,
   plane list, elevation profile, horizon view, options, band settings, watchlist, history,
   crossing history, first-run wizard, database maintenance dialogs, local obstructions.
4. **Integrations.** `as-wintest`, `as-cat` (rigctld first), `as-web`, `as-dde`/OmniRig on Windows.
5. **Suite tools.** PlaneServer, ViewClient, generators, WinTestSimulator, CATCheck, RainScout.
6. **Logic review & fixes.** Systematic review of the propagation/elevation/intersection maths
   against first principles (WGS84 vs. sphere, refraction / k-factor, Fresnel clearance, antimeridian
   and polar cases, step-width aliasing, time handling, extrapolation of aircraft positions).
   Each finding: write a failing test, fix, record the golden-test delta in `docs/logic-fixes.md`.
   User sees the list before fixes ship by default.

## Decided by the user

- Whole suite in scope.
- GUI: egui/eframe (approach A).
- Reported logic errors: no concrete reports known; stage 6 reviews systematically.
- Releases unsigned for now; signing/notarization added later when distributing.

## Calls made by Claude (correctable)

- Port 1:1 first, fix later (stage 6), so fixes are measurable.
- Stage order as above.
- Settings as TOML, not the original XML `user.config`.
- Fresh SQLite schemas via rusqlite; no compatibility with original DB files.
- rigctld over TCP as the primary CAT path on all OS; native hamlib FFI and OmniRig optional.
- Repository layout: this repo (`AirScout-RS`) is the workspace root.

- Golden tests also cover the aircraft-crossing prediction (`AirScout.Core/PlaneInfo.cs`,
  `AirScout.Aircrafts/PlaneInfoCache.cs`, crossing logic in `AirScout/MapDlg.cs`), not only ScoutBase.
- Elevation data: first verify airscout.eu tile files. Golden tests feed the **same** tile files to
  C# and Rust, so the source choice does not affect parity. If airscout.eu is gone, Rust reads raw
  GLOBE + SRTM3/SRTM1 (public mirrors) and Copernicus GLO-30 as an extra option.
- Aircraft/airline/airport DB: first verify airscout.eu; fallback OpenSky aircraft database CSV,
  OurAirports, OpenFlights airlines. Source is pluggable.
- Feeds: all 11 ported. Required to work: those with public, key-free access (OpenSky, ADSB.one,
  airplanes.live/BinCraft-style, local dump1090/readsb/RTL1090/VRS). Feeds needing credentials take
  user-supplied keys; obfuscated credentials from the original are **not** copied. Feeds whose
  terms forbid scraping are ported but disabled by default with a notice.
- Wire compatibility: keep JSON shapes of PlaneServer/AirScoutServer and the `wwwroot` web UI API
  unchanged, so old and new clients interoperate. Win-Test UDP is byte-compatible by nature.
- Maps work offline from the `as-maps` store (`<data>/maptiles.gmdb`, prefilled by `maptile-gen` or
  by the opt-in preloader from the airscout.eu mirror) plus the `walkers` HTTP disk cache of
  on-demand browsing; browsing never writes the store. Online tiles honour the OSM tile usage
  policy (User-Agent, HTTP cache headers, no bulk download from the tile servers).

## Left to the builder

- Exact crate versions (pin at implementation time).
- Whether an import tool for original settings/stations DBs is worth it (stage 3+, low priority).
- Choice of release tooling (cargo-dist vs. cargo-bundle + cargo-wix).
- Tolerances per golden test.
