# Stage 3a — Live Map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A runnable `airscout` desktop app (egui/eframe) that shows the map with both stations, the propagation path and its mutual-visibility segment, live aircraft from the selected feed with their predicted crossing, a plane list, the path profile chart and a status bar — the original's SINGLE path mode in LIFE mode.

**Architecture:** New crate `crates/airscout` with a GUI-free library (`src/lib.rs`: settings view, tick pipeline, overlay geometry, plane styling, chart series, background services) and a thin eframe binary (`src/main.rs`, `src/ui/*`) that only draws what the library computes. Background work runs on std threads and reports through `std::sync::mpsc` channels; the UI polls the channels each frame and calls `ctx.request_repaint_after(Map_Update)`. Map tiles come from `walkers` with its own disk cache.

**Tech Stack:** eframe/egui 0.37, egui_plot 0.38, egui_extras 0.37 (table), walkers 0.66 (`default-features = false`), rustls 0.23 (`ring` provider), egui_kittest 0.37 (dev), existing `as-*` crates.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md` (stage 3). GUI source survey with file:line refs: `docs/superpowers/surveys/2026-11-23-gui-survey.md` (Task 1 commits it).

**Upstream:** `target/upstream/AirScout` @ `91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`. All `MapDlg.cs:N` refs are into `target/upstream/AirScout/AirScout/MapDlg.cs`.

## Stage 3 split

Stage 3 (≈20k lines of WinForms) is three plans, each ending in a usable app:

- **3a (this plan):** app shell, map, SINGLE path, live planes, plane list, path profile, status bar, alarm.
- **3b:** MULTI mode + watchlist, OptionsDlg with band grid, MapStationDlg, LocalObstructionsDlg, HorizonDlg, FirstRunWizard, feed settings + disclaimer dialogs, background updaters (elevation, stations, aircraft) with status LEDs, PathCalculator, map preloader, SpecLab spectrum, SetTimeDlg.
- **3c:** history/analysis mode (player, load/save positions, history from URL, CrossingHistoryDlg, TrafficDlg, PlaneHistoryDlg, path export), database maintenance and cleanup dialogs, tracking values (`bw_Track` maths; outputs belong to stage 4).

## Global Constraints

- Port policy: port 1:1; prediction-affecting behaviour is reproduced even when wrong and logged in `docs/logic-findings.md` (next number **#82**).
- GUI-layer upstream bugs that do not affect prediction (wrong delete target, ignored settings, worker mix-ups, `DoEvents` from threads) are **not** reproduced; each still gets a findings entry marked "Not reproduced (GUI)".
- Pixel-identical UI is a non-goal; layout follows the original's information architecture (spec, Non-goals).
- Anti-goal: the UI must not stutter with hundreds of live aircraft. Path computation never runs on the UI thread (the original's `UpdatePaths` does; MapDlg.cs:4313).
- No secrets in the repo. Credentials only from user settings, never logged.
- Honest User-Agent `as_net::user_agent()` on every request, map tiles included (OSM tile policy). No browser or header spoofing.
- Requests to outside services never carry the user's call sign or position beyond what the feed's documented API needs; `getkey.php`, session keys, `AirScout_Register_URL`, location/QRV uploads and the plugin download are **not** ported.
- Settings are TOML via `as-settings`; new fields get `#[serde(default)]` defaults taken from `AirScout/Properties/Settings.settings`.
- Every crate: `#![forbid(unsafe_code)]`.
- Gate: `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked`. CI must stay green on self-hosted Linux, the `x86_64-pc-windows-gnu` cross build, and hosted macOS/Windows.
- Tests need no network and no display: GUI tests use `egui_kittest` without snapshots.

## Review Focus

1. **Feed down or returning errors for minutes** — the map keeps the last planes until their TTL runs out, the status bar shows the error text, the UI stays responsive. Test: Task 6 `feed_error_reaches_status_and_keeps_cache`.
2. **Elevation data missing for the path area (first start, no network)** — the path is drawn red (invalid), the status bar says which squares are missing, the app does not hang or panic. Test: Task 6 `path_without_elevation_is_invalid_not_error`.
3. **My or DX locator invalid / both stations identical / path longer than `Path_MaxLength`** — no path request is sent, the field is marked, no panic in geometry (zero-length path, antimeridian). Tests: Task 3 `zero_length_path_has_no_segments`, `antimeridian_path_is_split`, Task 7 `invalid_locator_blocks_play`.
4. **Hundreds of planes per tick** — the tick pipeline for 1000 planes and one path stays well inside one `Map_Update` second. Test: Task 2 `pipeline_1000_planes_is_fast` (plain test, 500 ms budget in a debug build).
5. **Settings file from stage 1/2 (fields missing) or a corrupt file** — loads with defaults / shows the parse error and starts with defaults without overwriting the broken file. Tests: Task 1 `stage2_file_loads_with_gui_defaults`, Task 7 `corrupt_settings_are_not_overwritten`.

---

### Task 1: GUI settings and crate skeleton

**Files:**
- Modify: `crates/as-settings/src/lib.rs`
- Create: `crates/as-settings/src/gui.rs`
- Create: `crates/airscout/Cargo.toml`, `crates/airscout/src/lib.rs`, `crates/airscout/src/main.rs`
- Modify: `Cargo.toml` (workspace deps)
- Create: `docs/superpowers/surveys/2026-11-23-gui-survey.md` (copy of the survey)
- Test: `crates/as-settings/tests/gui_settings.rs`

**Interfaces:**
- Produces: `as_settings::{GuiSettings, InfoWin, Area}`; `Settings.gui: GuiSettings`, `Settings.area: Area`.
  - `Area { min_lat: 35.0, max_lat: 60.0, min_lon: -15.0, max_lon: 30.0 }` (`MinLat`…`MaxLon`).
  - `GuiSettings` fields and defaults: `map_provider: "OpenStreetMap"`, `map_update_s: u64 = 1`, `map_auto_center: true`, `map_opacity: f32 = 1.0`, `map_label_calls: true`, `map_show_beam_width: false`, `locator_small_letters_for_subsquares: false`, `planes_filter_min_alt_m: i32 = 0`, `planes_filter_max_circumcircle_km: f64 = 0.0`, `planes_filter_min_category: PlaneCategory = None`, `planes_icon_size: [u32; 4] = [16, 24, 36, 48]` (L, M, H, S), `alarm_activate: false`, `alarm_distance_km: 100.0`, `alarm_bring_to_front: true`, `alarm_play_sound: true`, `info_win: InfoWin`, `my_calls: Vec<String>` (MRU, max 10), `window_size: Option<[f32; 2]>`, `window_pos: Option<[f32; 2]>`, `side_panel_width: Option<f32>`, `bottom_panel_height: Option<f32>`, `show_info_box/zoom_box/filter_box/alarm_box: bool = true`, `elevation_base_url: as_elevation::catalogue::DEFAULT_BASE_URL`, `elevation_download: true`.
  - `InfoWin` bools with defaults: `metric true, imperial false, position true, time true, epsilon false, squint true, track true, type_ true, alt true, dist true, speed false, angle false, my_azimuth/my_elevation/my_doppler/dx_azimuth/dx_elevation/dx_doppler false`.
  - `as-settings` gains a dependency on `as-aircraft` for `PlaneCategory` (check for a cycle first: `as-aircraft` must not depend on `as-settings`; if it does, store the category as `i32` and ledger a ruling).

- [ ] **Step 1: Write the failing tests** in `crates/as-settings/tests/gui_settings.rs`: `gui_defaults_match_csharp` (assert every default above), `stage2_file_loads_with_gui_defaults` (write a TOML holding only stage-2 fields, `load_or_default` → `gui == GuiSettings::default()`, `area == Area::default()`), `gui_roundtrip` (change five fields, save, load, equal).
- [ ] **Step 2: Run** `cargo test -p as-settings --test gui_settings` — Expected: FAIL (no `gui` field).
- [ ] **Step 3: Implement** `gui.rs` with `#[serde(default)]` structs and add the two fields to `Settings`.
- [ ] **Step 4: Run** the same command — Expected: PASS, then `cargo test -p as-settings`.
- [ ] **Step 5: Crate skeleton.** `crates/airscout/Cargo.toml`: package `airscout`, `[[bin]] name = "airscout"`, deps `eframe = "0.37"` (default features; no `persistence`, settings live in as-settings), `egui_plot = "0.38"`, `egui_extras = "0.37"`, `walkers = { version = "0.66", default-features = false }`, `rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }`, workspace `as-*`, `chrono`; dev-deps `egui_kittest = "0.37"`, `mockito`, `tempfile`. `main.rs` installs `rustls::crypto::ring::default_provider().install_default()` and opens an empty `eframe::run_native` window titled `AirScout-RS`. Copy the survey to `docs/superpowers/surveys/`.
- [ ] **Step 6: Run the gate** and `cargo build -p airscout --target x86_64-pc-windows-gnu` if mingw is available locally (else rely on CI). Expected: green; `cargo tree -p airscout -i aws-lc-sys` prints nothing.
- [ ] **Step 7: Commit** `feat(airscout): GUI settings and app skeleton`.

### Task 2: Tick pipeline

**Files:**
- Create: `crates/airscout/src/pipeline.rs`
- Test: `crates/airscout/tests/pipeline.rs`

**Interfaces:**
- Consumes: `as_aircraft::crossing::get_nearest_planes`, `PlaneInfoCache::get_all`, `as_settings::Settings`.
- Produces:
  - `pub struct TickInput<'a> { pub now: DateTime<Utc>, pub paths: &'a [PropagationPath], pub planes: Vec<PlaneInfo>, pub settings: &'a Settings }`
  - `pub struct TickResult { pub active: BTreeMap<String, PlaneInfo>, pub nearest: Option<PlaneInfo>, pub highest_potential: i32, pub shown: Vec<PlaneInfo>, pub alarm: Option<PlaneInfo>, pub chart_hi: Vec<ChartPlane>, pub chart_lo: Vec<ChartPlane> }`
  - `pub struct ChartPlane { pub dist_km: f64, pub alt_m: f64, pub call: String }`
  - `pub fn tick(input: TickInput) -> TickResult`

Behaviour to port, with refs:
- `UpdatePlanes` MapDlg.cs:5093-5229: per path `get_nearest_planes(input.now, …, planes_filter_max_circumcircle_km, band max_distance, planes_max_alt_m, planes_max_delta_m)`; merge into `active` (higher potential wins only when `int_point` is set; equal potential → smaller `int_qrb`); `nearest` reset **per path** so only the last path's nearest survives (reproduce, new finding); `highest_potential` only counts planes passing the filter (min alt, min category).
- The original passes `DateTime.UtcNow`, not `CurrentTime` (MapDlg.cs:5140). In LIFE mode both are equal; the history case is stage 3c — `TickInput.now` carries `CurrentTime` and the 3c plan decides. Ledger this.
- `DrawPlanes` MapDlg.cs:4956-5091: `shown` = active planes with `alt_m >= planes_filter_min_alt_m` and `category >= planes_filter_min_category`; chart points for the first path only when `int_qrb <= max_distance`: `alt_diff > 0` → `chart_hi`, else `chart_lo`; `dist_km` = distance My → `int_point`.
- Alarm MapDlg.cs:3457: first shown plane with potential 100 or 75 and `int_qrb < alarm_distance_km`, only if `alarm_activate`.

- [ ] **Step 1: Write the failing tests**: `merge_prefers_higher_potential_with_int_point`, `merge_equal_potential_prefers_nearer`, `nearest_is_from_last_path_only` (two paths, nearest on path 1 lost — comment cites the finding number), `filter_hides_low_and_light_planes`, `chart_points_split_by_alt_diff`, `alarm_needs_activate_and_distance`, `pipeline_1000_planes_is_fast` (1000 synthetic planes around a JO50→JO61 path built with `PropagationPath` from a fixed `PathParams` without elevation: use the as-propagation test helper or construct the struct directly; assert `< 500 ms` in debug).
- [ ] **Step 2: Run** `cargo test -p airscout --test pipeline` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `pipeline.rs`.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Findings + commit.** Add #82 (nearest per path) to `docs/logic-findings.md`. Commit `feat(airscout): tick pipeline (UpdatePlanes/DrawPlanes)`.

### Task 3: Map overlay geometry

**Files:**
- Create: `crates/airscout/src/overlays.rs`
- Test: `crates/airscout/tests/overlays.rs`

**Interfaces:**
- Produces (all positions `as_geo::GPoint`, no egui types):
  - `pub fn distance_rings(lat: f64, lon: f64) -> Vec<Vec<GPoint>>` — rings every 100 km to 1000 km (`CreateDistances` MapDlg.cs:1317), 360 points each.
  - `pub struct LocatorCell { pub loc: String, pub rect: GRect }`; `pub fn locator_grid(view: GRect, zoom: f64, small_letters: bool) -> Vec<LocatorCell>` — cell size by zoom per `InitializeLocators` MapDlg.cs:1577-1682 (1–5: 10×20°; 6–9: 1×2°; 10–13: 1×2° and 1/24°; 14–17: 1/24° and 1/240°; 18–20: 1/240° and 1/5760°), cap 2000 cells (ruling: protects the frame rate; the original has no cap).
  - `pub struct PathOverlay { pub full: Vec<Vec<GPoint>>, pub valid: bool, pub visible: Vec<Vec<GPoint>>, pub hot_markers: [Option<GPoint>; 3], pub mid: GPoint }`; `pub fn path_overlay(p: &PropagationPath, max_alt_m: f64) -> PathOverlay` per `DrawPath` MapDlg.cs:4122-4219 (visible = points with `0 < max(h1,h2) < max_alt`; hot markers first/middle/last of those). Segments are split at the antimeridian.
  - `pub fn beam_width_deg(gain_dbi: f64) -> f64` = `sqrt(41253 / 10^((gain + 2.15)/10))` (`SupportFunctions.cs:45`); `pub fn beam_wedge(lat, lon, bearing, width_deg, length_km) -> Vec<GPoint>` (MapDlg.cs:4222-4305).

- [ ] **Step 1: Write the failing tests**: `rings_are_100km_apart`, `grid_cell_size_by_zoom` (zoom 4, 7, 12, 15, 19), `grid_is_capped`, `small_letters_flag`, `visible_segment_matches_heights` (path built from fixed `PathParams` over flat zero elevation: assert the magenta segment equals the hand-computed range), `zero_length_path_has_no_segments`, `antimeridian_path_is_split`, `beam_width_of_10dbi` (= 16.82…°, compute in the test from the formula).
- [ ] **Step 2: Run** `cargo test -p airscout --test overlays` — Expected: FAIL.
- [ ] **Step 3: Implement** `overlays.rs`.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): map overlay geometry`.

### Task 4: Plane styling and tooltip text

**Files:**
- Create: `crates/airscout/src/style.rs`
- Test: `crates/airscout/tests/style.rs`

**Interfaces:**
- Produces:
  - `pub enum PlaneColor { Magenta, Red, DarkOrange, Gray }`; `pub fn plane_color(potential: i32) -> PlaneColor` (100/75/50/else; `CreatePlaneSimple` MapDlg.cs:4751-4820).
  - `pub fn plane_icon_px(cat: PlaneCategory, sizes: [u32; 4]) -> u32` (SUPERHEAVY→S, HEAVY→H, MEDIUM→M, LIGHT and others→L).
  - `pub fn tooltip_lines(p: &PlaneInfo, info: &InfoWin, my: GPoint) -> Vec<String>` — the lines and number formats of `CreatePlaneDetailed` MapDlg.cs:4822-4954 for the fields gated by `InfoWin` (tracking lines are 3c: skip and ledger).
  - `pub fn rgb(c: PlaneColor) -> [u8; 3]` (Magenta 255,0,255; Red 255,0,0; DarkOrange 255,140,0; Gray 128,128,128).

- [ ] **Step 1: Write the failing tests**: `color_by_potential`, `icon_size_by_category`, `tooltip_metric_default` (one plane, default `InfoWin`: assert exact strings copied from the C# format strings), `tooltip_imperial`, `tooltip_without_crossing_has_no_dist` (potential 0).
- [ ] **Step 2: Run** `cargo test -p airscout --test style` — Expected: FAIL.
- [ ] **Step 3: Implement** `style.rs`.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): plane colors, sizes and tooltips`.

### Task 5: Path profile series

**Files:**
- Create: `crates/airscout/src/charts.rs`
- Test: `crates/airscout/tests/charts.rs`

**Interfaces:**
- Consumes: `PropagationPath::info_points`, `ElevationPath` (as-elevation) for the terrain line.
- Produces: `pub struct ProfileSeries { pub elevation: Vec<[f64; 2]>, pub min_h1: Vec<[f64; 2]>, pub min_h2: Vec<[f64; 2]>, pub max_h: Vec<[f64; 2]>, pub min_h_area: Vec<([f64; 2], f64)>, pub los: Vec<[f64; 2]>, pub elevation_y_max: f64 }` (x = km from My, y = m); `pub fn profile(p: &PropagationPath, terrain: &[f64], max_alt_m: f64) -> ProfileSeries` per `UpdateCharts` MapDlg.cs:4602-4658 (`Min_H` area from `max_alt` down to `min(max_alt, max(h1,h2))`; LOS via the core's epsilon/height helpers; y max = max elevation + 10 %).

- [ ] **Step 1: Write the failing tests**: `series_lengths_match_info_points`, `min_h_area_bounds`, `los_line_ends_at_antenna_heights`, `y_max_is_ten_percent_above_peak`.
- [ ] **Step 2: Run** `cargo test -p airscout --test charts` — Expected: FAIL.
- [ ] **Step 3: Implement** `charts.rs`. If the epsilon/height helpers are private in `as-propagation`, make them `pub` (no behaviour change).
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): path profile series`.

### Task 6: Background services

**Files:**
- Create: `crates/as-elevation/src/prepare.rs` (moved from `as-cli`), modify `crates/as-elevation/src/lib.rs`, `crates/as-cli/src/main.rs`
- Create: `crates/airscout/src/services.rs`
- Test: `crates/as-elevation/tests/prepare.rs`, `crates/airscout/tests/services.rs`

**Interfaces:**
- Produces in as-elevation: `pub struct AreaStatus { pub complete: bool, pub missing: Vec<String>, pub sea_added: usize }`; `pub fn prepare_area(store: &ElevationStore, client: &reqwest::blocking::Client, base_url: &str, model: ElevationModel, rect: GRect, sea_file: &Path, download: bool, progress: &mut dyn FnMut(String)) -> Result<AreaStatus, ElevationError>` — exactly the CLI's `prepare` (as-cli/src/main.rs:220-275) without `eprintln!`; the CLI keeps its output by printing from `progress` and `missing`.
- Produces in airscout:
  - `pub struct Paths { pub store: Arc<ElevationStore>, pub engine: Arc<PropagationEngine>, pub stations: Arc<StationDb>, pub data_dir: PathBuf }` and `pub fn open_data(dir: &Path) -> Result<Paths, AppError>`.
  - `pub enum ServiceEvent { Status(String), Error(String), PathReady { path: PropagationPath, terrain: Vec<f64> }, PathFailed(String), Feed(FeedEvent) }`.
  - `pub struct PathService` with `pub fn spawn(paths: Paths, tx: Sender<ServiceEvent>) -> Self`, `pub fn request(&self, req: PathRequest)` (newest request wins; older pending ones are dropped), `pub fn stop(self)`.
  - `pub struct PathRequest { pub my: GPoint, pub my_call: String, pub dx: GPoint, pub dx_call: String, pub settings: Settings }` — the SINGLE branch of `UpdatePaths` MapDlg.cs:4313-4596: antenna heights from `qrv_find_or_create_default` (band), ground elevation of the selected model (fallback order of `GetElevation` MapDlg.cs:3649 ASTER1→…→GLOBE), `prepare_area` over the path bbox when `gui.elevation_download`, then `find_or_create_path` with `PathParams` built like `as-cli` `build_path`, `radius_km`, band `f1_clearance`, local obstruction from the store (`f64::MIN` if none).
  - `pub struct FeedService` with `pub fn start(paths: &Paths, settings: &Settings, cache: Arc<PlaneInfoCache>, tx: Sender<ServiceEvent>) -> Result<Self, AppError>` (wraps `FeedWorker::spawn` with `FeedArgs` from `area` and **the area centre** as `my_lat/my_lon` — same privacy rule as the CLI; ledger) and `pub fn stop(self)`.
  - `pub enum AppError` (thiserror).

- [ ] **Step 1: Write the failing tests** for `prepare_area` (mockito: one square 200 with a fixture zip from the stage-1 tests, one 404 → sea, assert `complete`, `sea_added == 1`, file written; `download = false` → `missing` lists the square). Run `cargo test -p as-elevation --test prepare` — Expected: FAIL.
- [ ] **Step 2: Move** `prepare` into `prepare.rs`, switch the CLI to it. Run `cargo test -p as-elevation --test prepare` and `cargo test -p as-cli` — Expected: PASS, CLI tests unchanged.
- [ ] **Step 3: Write the failing service tests**: `path_request_produces_path` (temp data dir, elevation download disabled, model None → path computed, `PathReady`), `newest_request_wins` (send three, receive one `PathReady` for the third), `path_without_elevation_is_invalid_not_error` (model GLOBE, no tiles, download off → `PathReady` with `valid == false` and a `Status` naming the missing squares), `feed_error_reaches_status_and_keeps_cache` (mockito feed URL returning 500 via `feed_settings`; cache pre-filled; assert `Feed(Error)` arrives and the cache still holds the plane).
- [ ] **Step 4: Run** `cargo test -p airscout --test services` — Expected: FAIL.
- [ ] **Step 5: Implement** `services.rs`.
- [ ] **Step 6: Run** — Expected: PASS. Then the gate.
- [ ] **Step 7: Commit** `feat(airscout): path and feed services; prepare_area in as-elevation`.

### Task 7: App shell, control panel, status bar

**Files:**
- Create: `crates/airscout/src/app.rs` (state + update logic, GUI-free where possible), `crates/airscout/src/ui/mod.rs`, `crates/airscout/src/ui/control.rs`, `crates/airscout/src/ui/status.rs`
- Modify: `crates/airscout/src/main.rs`
- Test: `crates/airscout/tests/app.rs`, `crates/airscout/tests/ui_control.rs`

**Interfaces:**
- Consumes: Tasks 1, 2, 6.
- Produces:
  - `pub enum RunMode { Paused, Playing }`; `pub struct AppState { pub settings: Settings, pub mode: RunMode, pub path: Option<PropagationPath>, pub terrain: Vec<f64>, pub tick: TickResult, pub status: String, pub selected: Option<String>, pub settings_error: Option<String>, … }`.
  - `impl AppState { pub fn play(&mut self) -> Result<(), String>; pub fn pause(&mut self); pub fn on_event(&mut self, e: ServiceEvent); pub fn on_tick(&mut self, now: DateTime<Utc>, cache: &PlaneInfoCache) }`.
  - `pub struct AirScoutApp` implementing `eframe::App`.
- Layout (survey §1.1): top bar (UTC, band combo), right side panel with "Single" (My call/loc, DX call/loc, QTF/QRB) and "Opt" (zoom ±, auto-center, filter min alt/max circumcircle/min category, alarm on), central map area, bottom panel (Task 9), status bar (status text, calculations, feed state).
- Play (MapDlg.cs:3998-4061): validates locators, sends a `PathRequest`, starts the feed, disables band/call/loc/filter inputs. Pause (MapDlg.cs:4063-4103): stops the feed, re-enables inputs. Ticks every `map_update_s` while playing (`ti_Progress_Tick` MapDlg.cs:5353-5400, LIFE branch only).
- Locator fields accept a Maidenhead locator or `lat,lon`; turn pale green when precise (`UpdateStatus` MapDlg.cs:3400).
- Settings: load at start; on parse error keep defaults, show the error, and do **not** save on exit (Review Focus 5). Save window geometry and settings on exit.
- Alarm: when `tick.alarm` is set and `alarm_bring_to_front`, send `ViewportCommand::Focus`; sound is not ported in 3a (ledger: needs an audio crate; 3b decides).

- [ ] **Step 1: Write the failing state tests** in `tests/app.rs`: `invalid_locator_blocks_play`, `play_sends_path_request_and_pause_stops_feed` (with a fake sink: make `AppState` take a `trait Services` so tests inject a recorder), `path_event_updates_state`, `tick_only_while_playing`, `corrupt_settings_are_not_overwritten`.
- [ ] **Step 2: Run** `cargo test -p airscout --test app` — Expected: FAIL.
- [ ] **Step 3: Implement** `app.rs`.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Write the failing kittest** in `tests/ui_control.rs`: `play_button_disables_inputs` (click "Play" with valid settings → the My locator text edit is disabled), `qtf_qrb_shown` (JO50AA→JO61UF: the QTF/QRB labels show the values from `as_geo::latlon`).
- [ ] **Step 6: Implement** `ui/control.rs`, `ui/status.rs`, `ui/mod.rs`, wire `main.rs`. Run `cargo test -p airscout` — Expected: PASS.
- [ ] **Step 7: Commit** `feat(airscout): app shell, control panel, status bar`.

### Task 8: Map view

**Files:**
- Create: `crates/airscout/src/ui/map.rs`, `crates/airscout/src/tiles.rs`
- Test: `crates/airscout/tests/tiles.rs`

**Interfaces:**
- Consumes: Tasks 3, 4, 7.
- Produces:
  - `pub struct TileProvider { pub name: &'static str, pub url: &'static str, pub attribution: &'static str, pub max_zoom: u8 }`; `pub fn providers() -> &'static [TileProvider]` seeded from `MapProvider.cs` (OpenStreetMap `https://tile.openstreetmap.org/{z}/{x}/{y}.png`, OpenTopoMap); Thunderforest needs a user API key → omitted in 3a (ledger).
  - `pub fn http_options(cache_dir: &Path) -> walkers::HttpOptions` with `user_agent = as_net::user_agent()`, `cache = Some(<data>/tiles)`, default parallelism.
  - `ui::map::MapView` drawing: tiles, distance rings around My (gray), locator grid (labels when the cell is ≥ 40 px wide), path (black valid / red invalid, width 3), visible segment (magenta, width 3), markers My red / DX yellow / mid blue, beam wedges when `map_show_beam_width` (gray α 50), planes as rotated triangles in `plane_color`/`plane_icon_px` with call labels, tooltip on hover (`tooltip_lines`), click toggles selection, selected plane shows a thin black line to its `int_point` and a permanent tooltip. Auto-center keeps the path midpoint centred; "fit" zooms to the path bbox (`SetZoomToFitRect`).
- Map opacity: tiles drawn with `map_opacity`.

- [ ] **Step 1: Write the failing tests**: `osm_provider_url_and_attribution`, `http_options_use_honest_user_agent` (assert the header value equals `as_net::user_agent()` and does not contain "Mozilla").
- [ ] **Step 2: Run** `cargo test -p airscout --test tiles` — Expected: FAIL.
- [ ] **Step 3: Implement** `tiles.rs` and `ui/map.rs` (walkers `Map` with plugins for the overlays).
- [ ] **Step 4: Run** `cargo test -p airscout` — Expected: PASS. Run the app once locally (`cargo run -p airscout`) against OSM and check: tiles load, path drawn, planes move with OpenSky (manual, note the result in the ledger).
- [ ] **Step 5: Commit** `feat(airscout): map view with tiles, path and planes`.

### Task 9: Bottom panel — path profile, plane list, nearest plane

**Files:**
- Create: `crates/airscout/src/ui/profile.rs`, `crates/airscout/src/ui/planes.rs`
- Test: `crates/airscout/tests/ui_planes.rs`

**Interfaces:**
- Consumes: Tasks 2, 4, 5, 7.
- Produces: bottom tabs "Path Profile" and "Planes".
  - Profile: egui_plot with the `ProfileSeries` lines (elevation area brown, min H1 red, min H2 gold, max H dark blue dotted, min-H area magenta, LOS black) and plane points (`chart_hi` magenta squares, `chart_lo` gray squares) with the call as hover text; My/DX calls at the chart ends (`pv_Path_Paint` MapDlg.cs:4705).
  - Planes (spec stage 3 item; the original has none — ledger): `egui_extras::TableBuilder` over `tick.shown` sorted by potential desc then `int_qrb` asc; columns Call, Type, Cat, Alt, Dist (`int_qrb`), Min (`minutes_to_crossing`), Pot, Squint. Row colored by `plane_color`. Clicking a row selects the plane (same state as the map). Header shows the plane count (`tp_Map.Text`).
  - Nearest box above the table: call, type, category, alt, angle, dist of `tick.nearest` (`gb_Spectrum_NearestInfo`).

- [ ] **Step 1: Write the failing kittest**: `plane_list_sorted_by_potential` (state with three planes → row order), `clicking_row_selects_plane`, `empty_list_shows_hint`.
- [ ] **Step 2: Run** `cargo test -p airscout --test ui_planes` — Expected: FAIL.
- [ ] **Step 3: Implement** both panels.
- [ ] **Step 4: Run** `cargo test -p airscout` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): path profile, plane list, nearest plane`.

### Task 10: CI, docs, findings

**Files:**
- Modify: `.github/scripts/ci-prepare.sh` (if eframe needs system packages on the self-hosted runner), `.github/workflows/ci.yml`
- Modify: `README.md`, `docs/logic-findings.md`
- Create: `docs/superpowers/ledgers/2026-11-23-stage3a-live-map.md` (at the end, from the SDD ledger)

- [ ] **Step 1:** Push to a branch, open no PR; check the CI run. If the Linux job fails to build eframe, add the missing packages to the `missing` list of `ci-prepare.sh` (expected none: winit loads X11/Wayland at run time). Expected: all four jobs green.
- [ ] **Step 2:** Findings: GUI bugs from the survey §6 that 3a touches, each "Not reproduced (GUI)" or reproduced with reason; plus #82 from Task 2. Numbers contiguous.
- [ ] **Step 3:** README: stage 3a row, `cargo run -p airscout`, screenshot-free usage notes, tile-cache location, OSM attribution note.
- [ ] **Step 4:** Gate green on macOS and in the Linux docker gate.
- [ ] **Step 5: Commit** `docs: stage 3a README, findings, ledger`.
