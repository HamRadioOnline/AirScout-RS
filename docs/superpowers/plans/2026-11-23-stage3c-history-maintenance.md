# Stage 3c — History/Analysis, Database Maintenance and Tracking Values Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the main GUI (spec stage 3):
- HISTORY (analysis) mode with the player, load/save of positions, history from a user-given URL.
- CrossingHistoryDlg, TrafficDlg, PlaneHistoryDlg and the path CSV export.
- Database maintenance: delete buttons, the generic table dialog, delete single station, `--clean`, per-model elevation coverage maps, the elevation grid on the station map.
- Tracking values (`bw_Track` maths) shown on the map and in plane tooltips. The outputs (rotor, CAT, UDP, files) stay in stage 4.

**Architecture:**
- Task 1 splits `app.rs` (1456 lines) mechanically into `app/` submodules **before** any 3c wiring. New 3c state lives in new `app/*.rs` files as `impl AppState` blocks, so `app/mod.rs` does not grow.
- Long work (history load, file I/O, downloads, crossing history, traffic routes, DB maintenance, coverage scans) runs on std threads through one `JobRunner` (Task 5). Results come back as `ServiceEvent::Job { id, event }`. Each job has a cancel flag; a newer job id makes older events stale.
- Pure, display-free logic goes into small modules with their own tests: position file formats, history index, player, crossing history, ambiguity pass, track maths, coverage cells, elevation grid.
- Core gaps (position DB queries, `PlaneInfoCache::set/clear`, table maintenance helpers, tile existence, path cache deletes) are filled in the core crates. Each gap comes with its own tests.

**Tech Stack:**
- Rust 1.95+, eframe/egui 0.37, egui_plot 0.38, egui_extras 0.37, walkers 0.66, egui_kittest 0.37 (dev), zip 2, reqwest 0.12 blocking, mockito (dev).
- New: `fs4` for the free disk space check (Task 8). This needs user approval (Open question 1).
- Existing crates: as-geo, as-elevation, as-propagation, as-stations, as-aircraft, as-feeds, as-signals, as-settings, as-net, as-db.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md` (stage 3: "history, crossing history, … database maintenance dialogs").

**Inputs:**
- GUI survey: `docs/superpowers/surveys/2026-11-23-gui-survey.md` (§1.2, §1.3, §1.10, §3, §5).
- Ledgers: `docs/superpowers/ledgers/2026-11-23-stage3a-live-map.md`, `docs/superpowers/ledgers/2026-11-23-stage3b-dialogs-background.md`.
- Upstream source: `target/upstream/AirScout/` @ `91b02cb7`. C# references are relative to `target/upstream/AirScout/AirScout/` unless they start with `ScoutBase/` or `AirScout.`.

**Stage split:**
- **3c (this plan):**
  - Analysis tab (MapDlg.cs:6515-6948): ON/OFF, player, scrollbar with markers, Load/Save/Clear positions, History from URL, Show traffic, Export path, Crossing history, Plane history.
  - TrafficDlg, CrossingHistoryDlg, PlaneHistoryDlg, HistoryFromURLDlg.
  - Options Database tab buttons (delete elevation paths, propagation paths, map tiles, single station; maintenance per DB).
  - DatabaseMaintenanceDlg with a read-only View (DatabaseEditDlg), DeleteSingleStationDlg, CleanupDlg (`--clean`), ElevationCopyrightDlg, LicenseDlg.
  - Per-model coverage maps (Options Elevation tab). The MapStationDlg elevation grid at zoom ≥ 17.
  - Track modes and `bw_Track` maths: TrackValues, Doppler strategies, the gauges and rig box, the TRK status field, the tooltip tracking lines.
  - The `TickInput` time decision for HISTORY (finding #86).
- **Deferred into 3c by earlier ledgers, all taken:**
  - DB delete/maintenance buttons and coverage maps (3b plan "Moved out").
  - Elevation grid at zoom ≥ 17 (3b Task 8).
  - Tracking lines in the plane tooltip (`style.rs:2`).
  - `TickInput.now` for HISTORY (3a ledger Task 2, finding #86).
  - The app.rs split (3b ledger Task 9/13/14/15 minors). Moving `FeedService` out of `services.rs` (3b ledger Task 12 minor) joins Task 1.
- **Stage 4:** the Track tab outputs (DDE HRD, UDP Win-Test/AirScout, GS-232A serial, azel.dat files, the CAT Doppler push, MapDlg.cs:7839-7986, :8018-8047, :8157-8249, :8382-8402, :8436-8449). 3c only computes and shows values.
- **Not ported (findings):**
  - The FR24 `clickhandler` lookup in the `.hist` loader (MapDlg.cs:9655-9737). It is a non-public endpoint with a spoofed Firefox User-Agent and a hard-coded cookie.
  - The default history URL `http://history.adsbexchange.com/Aircraftlist.json/`. It is dead/paid.
  - Dead code: `ScoutBaseDatabaseMaintenanceDlg`, `AircraftDatabaseMaintenanceDlg` and `ReflectionDlg` are never instantiated.
  - The Map DB maintenance dialog. The port has no map tile DB, only the walkers cache directory.

## Global Constraints

- **Port policy:**
  - Prediction-affecting behaviour is ported 1:1, even when it is wrong, and logged in `docs/logic-findings.md`. The next finding number is **#106**. Task 16 assigns the numbers in the order of the finding list there.
  - GUI-layer upstream bugs are **not** reproduced. Each still gets a findings entry marked "Not reproduced (GUI)".
  - Tracking outputs belong to stage 4. 3c ports only the maths and the display.
- Pixel-identical UI is a non-goal. The layout follows the original's information architecture.
- **Anti-goal:** the UI must not stutter with hundreds of live aircraft, with a large position history (hundreds of thousands of rows) or while jobs run. No DB scan, file I/O, download or crossing calculation runs on the UI thread. The per-tick history snapshot is O(hexes · log n), not O(n).
- **Secrets:**
  - No secrets in the repo.
  - Credentials (feed keys, the Thunderforest key) come only from user settings.
  - They never appear in logs, `Debug` output, status lines, error texts or the User-Agent.
- **Network:**
  - Every request uses the honest User-Agent `as_net::user_agent()`. No spoofing.
  - Requests to outside services never carry the user's call sign or position beyond what the feed's documented API needs.
  - `getkey.php`, session keys, register, upload and plugin download are **not** ported.
  - Non-public or credential endpoints are off by default and need a disclaimer.
  - Bulk downloads (history archives, map tiles) run only on an explicit user action. They respect the OSM tile usage policy. There is no preloader.
  - Self-hosted CI never runs fork PR code.
- **Settings:**
  - Settings are TOML via `as-settings`.
  - New fields get `#[serde(default)]` defaults taken from `AirScout/Properties/Settings.settings`. Deviations are named in Task 2.
  - A settings file from stage 3b (new fields missing) must load unchanged.
- Every crate has `#![forbid(unsafe_code)]`.
- **SQL:** all SQL uses bound parameters. Table names in the maintenance helpers are only accepted if they appear in `sqlite_master`, and they are quoted with `"…"`.
- **Files:**
  - No file dialogs (no `rfd`, no GTK on Linux).
  - Exports and saved positions go to `<data dir>/Tmp/`. The status line shows the full path.
  - Loadable position files are picked from a list of `*.json`, `*.csv` and `*.hist` files in `<data dir>/Tmp/`.
  - CSV separator is the constant `;` (finding #99 convention).
- **Gate:**
  - Command: `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked`.
  - Linux docker gate: `apt-get install -y libasound2-dev pkg-config`, `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`.
  - CI must stay green on self-hosted Linux, the `x86_64-pc-windows-gnu` cross build, and hosted macOS/Windows.
- **Tests:**
  - Tests need no network (mockito for HTTP) and no display (`egui_kittest` without snapshots).
  - Tests use generic coordinates (for example JO50/JO60 squares, 50.0/8.0). They use no real call signs except the existing test fixtures.
  - Disk space comes from an injectable `trait DiskSpace`, so tests never need 50 GB.
- **File size:** new files stay under about 500 lines. `app/mod.rs` must not grow beyond its Task 1 size by more than about 60 lines. 3c logic goes into `app/history.rs`, `app/tracking.rs`, `app/maintenance.rs`.

## Review Focus

1. **Entering HISTORY with a large position DB (500 000 rows), then switching OFF mid-load.**
   - Expected: the load runs off the UI thread with "Getting position N" progress. OFF cancels it at once, and no stale `HistoryLoaded` is applied later. Playing at +10 s/tick keeps a tick well under a second.
   - Tests: Task 5 `off_while_loading_drops_stale_history`, Task 6 `snapshot_200k_positions_is_fast`.
2. **History from URL with the network down, HTTP 404/500, a corrupt or truncated zip, missing per-step JSON entries, or less than 50 GB free.**
   - Expected: an error in the analysis status, no panic. With low disk space no request is sent. Steps that are present are still imported, and the status counts the missing ones. Cancel stops the transfer within a second.
   - Tests: Task 8 `low_disk_space_refuses_before_request`, `http_404_reports_error_and_imports_nothing`, `corrupt_zip_is_error_not_panic`, `missing_steps_are_counted`, `cancel_stops_download`.
3. **Destructive maintenance (Delete All per table, delete elevation/propagation paths, delete map tiles, delete single station, `--clean`).**
   - Expected: every delete needs an explicit confirmation and is refused while playing or in HISTORY with a job running. It runs on a job thread. Cancel or close deletes nothing.
   - Tests: Task 12 `delete_all_needs_confirmation`, `maintenance_refused_while_playing`; Task 14 `cleanup_close_deletes_nothing`.
4. **Hand-edited or foreign position files (bad lines, unknown columns, empty file, UTF-8 BOM, a JSON array with nested objects, `.hist` without an aircraft DB).**
   - Expected: bad records are skipped and counted, the import reports "N imported, M skipped", and nothing panics.
   - Tests: Task 4 `csv_bad_lines_skipped_and_counted`, `json_nested_objects_parsed`, `empty_file_imports_nothing`, `bom_is_ignored`.
5. **HISTORY with an empty DB, a single instant (oldest == youngest), or a min/max picker set inverted.**
   - Expected: a message and Load enabled for an empty DB. Markers and slider never divide by zero. An inverted range is rejected with a message.
   - Tests: Task 6 `empty_history_shows_message`, `single_instant_markers_no_panic`, `inverted_range_rejected`.

---

### Task 1: Split `app.rs` and `services.rs` (mechanical refactor)

**Files:**
- Move: `crates/airscout/src/app.rs` → `crates/airscout/src/app/mod.rs`
- Create, each moved code only:
  - `crates/airscout/src/app/services_api.rs`: `Services` trait, `RealServices`, `FailedServices`, the `Drop` impl (app.rs:63-142, :1108-1345).
  - `crates/airscout/src/app/dialogs.rs`: the `impl AppState` block for watchlist, options, stations/QRV, obstructions, horizon, set-time and wizard dialogs (app.rs:433-685, :808-827, :852-883).
  - `crates/airscout/src/app/shell.rs`: `AirScoutApp` and `impl eframe::App` (app.rs:1347-1456).
- Move: `FeedService` from `crates/airscout/src/services.rs:579-662` → `crates/airscout/src/feed_service.rs`, re-exported from `services`.
- Test: existing tests only.

**Interfaces:**
- Produces:
  - Public paths stay the same through `pub use` in `app/mod.rs`: `airscout::app::{AppState, Services, RealServices, FailedServices, AirScoutApp, RunMode, ControlTab, BottomTab, PathEntry, load_settings, data_dir, parse_position, dx_watch_item}`.
  - `airscout::services::FeedService` is kept through a re-export.
  - Private `AppState` fields that the submodules need become `pub(super)`. There is no new `pub`.
- No behaviour change. No test is edited except `use` lines, and only if the compiler requires it.

- [ ] **Step 1:** Run `cargo test -p airscout` and record the number of tests (baseline).
- [ ] **Step 2:** Move the code as listed. `app/mod.rs` keeps the types, `AppState` with `new`, play/pause, events, tick, `save_settings`.
- [ ] **Step 3:** Run the full gate. Expected: PASS, with the same number of tests as in Step 1. Report the `wc -l` of each new file. `app/mod.rs` should be ≤ 900 lines.
- [ ] **Step 4: Commit** `refactor(airscout): split app.rs into app/ submodules, FeedService into its own module`.

### Task 2: Settings for 3c

**Files:**
- Modify: `crates/as-settings/src/lib.rs`, `crates/as-settings/src/gui.rs`
- Create: `crates/as-settings/src/analysis.rs`, `crates/as-settings/src/track.rs`
- Test: `crates/as-settings/tests/settings_3c.rs`

**Interfaces:**
- **`Analysis`** (Settings.settings:389-404, :523-526):
  - `history_url: String = ""`. **No default URL** (finding).
  - `history_disclaimer_accepted: bool = false`.
  - `history_directory: String = ""`. Empty means `<data>/Tmp/History`.
  - `history_date: chrono::NaiveDate = 2017-07-01`.
  - `history_step_min: u32 = 5`.
  - `load_file_name: String = ""`.
  - `crossing_with_signal_level: bool = false`.
  - `crossing_ambiguous_gap_s: u32 = 10`.
- **`Track`** (Settings.settings:239-245, :697-715):
  - `activate: bool = false`, `set_az: f64 = 0`, `set_el: f64 = 0`, `offset_s: i64 = 0`, `update_ms: u64 = 1000`.
- **`Doppler`** (:670-703):
  - `dial_freq_hz: i64 = 0`.
  - `strategy: DopplerStrategy = None`, with `pub enum DopplerStrategy { None, A, B, C, D }` serialized as lowercase.
  - C# keeps five bools, and the first true one of A..D wins. Exactly one value is equivalent.
- **New fields on `Settings`:** `analysis: Analysis`, `track: Track`, `doppler: Doppler`.
- **New fields on `GuiSettings`:** `tracking_gauges_show: bool = true` (`Map_TrackingGaugesShow`, :751), `stations_map_overlay_elevation: bool = false` (`StationsMap_OverlayElevation`, :607).

- [ ] **Step 1: Write the failing tests:**
  - `defaults_match_csharp`: the values above.
  - `stage3b_file_loads_unchanged`: serialize `Settings::default()` with the 3c keys removed, load it, assert the 3c defaults and `first_run == false`.
  - `history_url_has_no_default`: `Settings::default().analysis.history_url.is_empty()`.
  - `doppler_strategy_roundtrip`: `"c"` ↔ `DopplerStrategy::C`.
- [ ] **Step 2: Run** `cargo test -p as-settings --test settings_3c`. Expected: FAIL (types missing).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p as-settings`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-settings): stage 3c settings (analysis, track, doppler)`.

### Task 3: Core gaps for history — position DB queries, plane cache

**Files:**
- Modify: `crates/as-aircraft/src/positions.rs`, `crates/as-aircraft/src/cache.rs`
- Test: `crates/as-aircraft/tests/positions_3c.rs`

**Interfaces (C# `AirScout.AircraftPositions/AircraftPositionDatabase.cs`):**
- **`PositionDb`:**
  - `get_all_hex(from, to) -> Result<Vec<String>, AircraftError>` (:588, `GROUP BY Hex`, both bounds inclusive).
  - `get_all_calls(from, to) -> Result<Vec<String>, _>` (:612).
  - `get_by_hex_in(hex, from, to) -> Result<Vec<AircraftPosition>, _>` (:675).
  - `get_by_call_in(call, from, to) -> Result<Vec<AircraftPosition>, _>` (:699).
  - `get_all_with(cancel: &AtomicBool, progress: &mut dyn FnMut(usize)) -> Result<Vec<AircraftPosition>, _>` (:636). Ordered by `LastUpdated ASC`. Progress every 1000 rows; returns `Ok(vec![])` on cancel, like C#.
  - `delete_all() -> Result<usize, _>` (:293).
  - `bulk_insert(&[AircraftPosition]) -> Result<usize, _>` (:459). One transaction; rows written; rollback on error. The C# swallows errors; the port returns them (`bulk_insert_plane_infos` precedent, finding #29).
  - All list queries sort `LastUpdated ASC`. All bind parameters (the C# concatenates SQL; not reproduced, finding).
- **`PlaneInfoCache`:**
  - `set(&self, plane: PlaneInfo)`: the C# indexer `Planes[hex] = info` (MapDlg.cs:5390). It replaces without touching the `old_*` fields.
  - `clear(&self)`: `Planes.Clear()` (:5386).

- [ ] **Step 1: Write the failing tests:**
  - `hex_and_calls_in_range_inclusive`.
  - `by_hex_in_range_sorted`.
  - `get_all_with_reports_progress_and_cancels`: 2500 rows give progress 1000 and 2000; with cancel set first, the result is empty.
  - `delete_all_counts`.
  - `bulk_insert_rolls_back_on_error`.
  - `cache_set_replaces_without_old_values`.
  - `cache_clear_empties`.
- [ ] **Step 2: Run** `cargo test -p as-aircraft --test positions_3c`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p as-aircraft`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-aircraft): position history queries and cache set/clear`.

### Task 4: Position file formats (save/load)

**Files:**
- Create: `crates/as-aircraft/src/position_files.rs`
- Test: `crates/as-aircraft/tests/position_files.rs`

**Interfaces:**
- `pub struct ImportStats { pub imported: Vec<AircraftPosition>, pub skipped: usize }`.
- **`write_json(w: &mut dyn Write, rows: &[AircraftPosition]) -> io::Result<()>`:**
  - A JSON array of objects `{"Hex","Call","Lat","Lon","Alt","Track","Speed","LastUpdated"}`. `LastUpdated` is ISO-8601 UTC.
  - C# writes `PlaneInfo` objects with `Time` that its own loader cannot read, and leaves a trailing comma when the date filter drops trailing rows (MapDlg.cs:9505-9532). That is not reproduced (GUI, finding).
- **`write_csv(w, rows)`:**
  - Header exactly `time[utc];hex;call;lat[deg];lon[deg];alt[ft];track[deg];speed[kts]`.
  - Time format `%Y-%m-%d %H:%M:%SZ`. Numbers `{:.8}` (MapDlg.cs:9534-9562).
- **`read_json(text: &str) -> Result<ImportStats, String>`** (:9606-9653):
  - The first non-whitespace char must be `[`, else `Err`.
  - Use `serde_json` over the array, not the C# `{`…`}` scan. Accept `LastUpdated` or `Time`, so C# saves load too.
  - Entries without a time or hex are counted as skipped.
- **`read_csv(text: &str) -> Result<ImportStats, String>`** (:9739-9796):
  - Header split on `;`, lower-cased, `[...]` stripped. Columns are found by name: `time, hex, call, lat, lon, alt, track, speed`. A missing `time`/`hex`/`lat`/`lon` column is `Err`.
  - Lines without `;` are skipped.
  - Hex and call are upper-cased.
  - Time is parsed as UTC from `%Y-%m-%d %H:%M:%SZ`, `%Y-%m-%d %H:%M:%S` and RFC 3339. C# parses with the local culture and converts local → UTC; not reproduced (GUI, finding).
  - A leading BOM is ignored.
- **`read_hist(text: &str, find_by_call: &dyn Fn(&str) -> Option<String>) -> Result<ImportStats, String>`** (:9655-9737, `AirScout.Core/HistSON.cs:20-56`):
  - Input `Dictionary<string, HistJSON>`. Mapping: `Hex = id`, `Call = callsign` or `"[unknown]"`, `LastUpdated = timestamp` (unix s), lat/lon/track/altitude/speed.
  - When `find_by_call(call)` returns a hex, use it.
  - The FR24 web lookup is **not ported** (finding).
- Every reader returns stats. A per-record error never fails the whole file.

- [ ] **Step 1: Write the failing tests:**
  - `json_roundtrip`.
  - `json_accepts_csharp_time_field`.
  - `json_nested_objects_parsed`: an entry with `"IntPoint": {"Lat":1,"Lon":2}` imports.
  - `json_garbage_is_error_not_panic`.
  - `csv_roundtrip_and_header_exact`.
  - `csv_bad_lines_skipped_and_counted`.
  - `csv_columns_in_any_order`.
  - `bom_is_ignored`.
  - `empty_file_imports_nothing`.
  - `hist_maps_fields_and_uses_db_hex`.
- [ ] **Step 2: Run** `cargo test -p as-aircraft --test position_files`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-aircraft): position file formats (json, csv, hist)`.

### Task 5: JobRunner and the history data getter

**Files:**
- Create: `crates/airscout/src/jobs.rs`, `crates/airscout/src/analysis.rs` (job bodies), `crates/airscout/src/history.rs` (`HistoryData`, `HistoryIndex`)
- Modify: `crates/airscout/src/services.rs` (`ServiceEvent::Job`), `crates/airscout/src/app/services_api.rs`
- Test: `crates/airscout/tests/analysis.rs`

**Interfaces:**
- **`jobs.rs`:**
  - `pub struct JobRunner`.
  - `JobRunner::spawn(name, id, cancel: Arc<AtomicBool>, tx, f: impl FnOnce(&JobCtx) + Send + 'static)`.
  - `JobCtx { id, cancel, tx }` with `progress(text)`, `cancelled() -> bool`, `send(JobEvent)`.
  - A thread-spawn failure sends `JobEvent::Failed`.
  - `ServiceEvent::Job { id: u64, event: JobEvent }`.
  - `pub enum JobEvent { Progress(String), Done(String), Failed(String), HistoryLoaded(Box<HistoryData>) }`. Later tasks add variants (listed in their Interfaces).
- **`AnalysisJob` and the services trait:**
  - `pub enum AnalysisJob { LoadHistory, SavePositions { path: PathBuf, from, to, csv: bool }, LoadFiles(Vec<PathBuf>), ClearPositions }`.
  - Later tasks add `Download`, `ExportPath`, `Crossings`, `PlaneLists`, `ExportPlane`, `Traffic`.
  - `Services::run_analysis(&mut self, id: u64, job: AnalysisJob)` (default no-op).
  - `Services::cancel_analysis(&mut self)`: sets the flag and drops the handle.
  - In `RealServices`, the jobs open `PositionDb`/`AircraftDb` in `paths.data_dir` on the job thread.
- **`LoadHistory`** ports `bw_Analysis_DataGetter` (MapDlg.cs:9340-9418):
  - `oldest`, then `youngest + planes_position_ttl_min` minutes, then `count`, then `get_all_with` (progress "Getting position N").
  - Enrichment per hex, the first time the hex is seen: `aircraft_find_by_hex`, then `aircraft_type_find_by_icao(type_code)`. Missing values become `"[unknown]"` and category `None`.
  - Each position becomes a `PlaneInfo` with time/lat/lon/track/alt/speed from the row.
- **`pub struct HistoryData { pub oldest, pub youngest, pub count: u64, pub positions: Vec<PlaneInfo> }`:**
  - `positions` is sorted by time.
  - An empty DB gives `oldest = youngest = None`.
- **`pub struct HistoryIndex`**, built from `&[PlaneInfo]`, holds per-hex sorted indices:
  - `fn latest_per_hex(&self, positions: &[PlaneInfo], min: DateTime<Utc>, at: DateTime<Utc>) -> Vec<PlaneInfo>`: for each hex, the last position with `min <= time <= at`. This is the same result as the C# O(n) refill (MapDlg.cs:5384-5391), computed with a binary search per hex.
  - `fn markers(&self, positions: &[PlaneInfo]) -> Vec<DateTime<Utc>>`: the distinct report seconds.
- **Other jobs:**
  - `SavePositions` uses `get_all(from, to)` and Task 4 `write_json`/`write_csv`.
  - `LoadFiles` uses the readers by extension and `bulk_insert` every 1000 rows, then reports `Done("N imported, M skipped")`. Unknown extensions are skipped with a note (C# writes nothing silently).
  - `ClearPositions` uses `delete_all`.

- [ ] **Step 1: Write the failing tests** (temp data dir, real DB files):
  - `load_history_enriches_and_sorts`.
  - `load_history_empty_db`.
  - `latest_per_hex_matches_linear_scan`: random 2000 positions, compared to the O(n) reference.
  - `save_then_load_roundtrip_json_and_csv`.
  - `load_files_reports_counts`.
  - `clear_positions_empties_db`.
  - `cancel_load_history_sends_no_loaded`.
  - `off_while_loading_drops_stale_history`: an app-level test with a fake `Services`; a `Job` event with an old id after OFF changes nothing.
- [ ] **Step 2: Run** `cargo test -p airscout --test analysis`. Expected: FAIL.
- [ ] **Step 3: Implement.** Add the `Job` arm to `AppState::on_event`, delegating to `app/history.rs` (`fn on_job_event`).
- [ ] **Step 4: Run** `cargo test -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): job runner and history data getter`.

### Task 6: HISTORY mode — state, player, tick

**Files:**
- Create: `crates/airscout/src/app/history.rs`, `crates/airscout/src/player.rs`
- Modify: `crates/airscout/src/pipeline.rs`, `crates/airscout/src/app/mod.rs` (fields, `tick_due`, the `on_tick` LIFE/HISTORY branch)
- Test: `crates/airscout/tests/history_mode.rs`, `crates/airscout/tests/pipeline.rs`

**Interfaces:**
- **`TickInput` time decision (closes finding #86):**
  - `TickInput` gains `pub nearest_at: DateTime<Utc>`, the time passed to `get_nearest_planes`. `now` stays `CurrentTime` for `get_all` and the tooltips.
  - LIFE: `nearest_at = now`. This is equal to C# `DateTime.UtcNow` there.
  - HISTORY: `nearest_at = Utc::now()` (wall clock), **ported 1:1** (MapDlg.cs:5140).
  - Effect: the first path's crossings use the historic positions. Later paths in MULTI see the planes moved to the wall clock by `get_nearest_planes` (finding #24 interplay), as in C#.
  - Finding #86 becomes "Resolved in 3c: ported 1:1".
- **`player.rs` (MapDlg.cs:6865-6906):**
  - `pub enum PlayerButton { Rewind, Back, Pause, Forward, FastForward }`.
  - `pub fn step(interval_s: i64, b: PlayerButton) -> i64`: Rewind −10, Back −1, Pause 0, Forward +1, FastForward +10, clamped to `-3600..=3600`.
  - **Not reproduced (GUI):** the C# clamps Back/Rewind to ≥ 1, so they still play forward (finding).
  - `pub fn advance(t, interval_s, min, max) -> DateTime<Utc>`: `t + interval`, clamped to `[min, max]` (:5372-5376).
- **AppState (`app/history.rs`):**
  - `pub life_mode: LifeMode`.
  - `pub history: Option<HistoryState>`, with `HistoryState { data: Arc<HistoryData>, index: HistoryIndex, min, max, interval_s: i64, cache: PlaneInfoCache, markers: Vec<DateTime<Utc>>, status: String, job_id: u64 }`.
- **`enter_history()`** (ON, :6547-6561):
  - Refused while playing (the Play button is disabled in HISTORY, and ON is reachable only while paused).
  - Sets `life_mode = History`.
  - Calls `pause()`, `stop_background()` and `services.run_analysis(id, LoadHistory)`.
  - C# keeps the position maintainer running in HISTORY. The port stops it, so it cannot delete rows under the analysis (finding).
- **`leave_history()`** (OFF, :6563-6609):
  - Cancels the analysis job and drops `history`.
  - Calls `start_background()` and sets `life_mode = Life`, mode paused.
- **On `HistoryLoaded`** (:9442-9497):
  - An empty DB sets the status "No positions in the database" and keeps Load enabled.
  - Otherwise `min = oldest`, `max = youngest`, `time.offline = oldest`, `time.online = false`.
  - Paths are requested for the current mode through the same helper Play uses. Extract `fn request_paths_for_mode(&mut self) -> Result<(), String>` from `play()`; no feed starts.
- **`set_history_range(min, max) -> Result<(), String>`:**
  - Clamped to `[oldest, youngest]`. `min >= max` is `Err`.
  - The C# Max picker clamps the Min picker (copy-paste, :6934-6942) and every change resets the scrollbar. Both are not reproduced (finding).
- **`player(b)`:** sets `interval_s = player::step(..)`.
- **`scroll_to(secs: i64)`** (:6908-6922):
  - Sets `time.offline = min + secs`, clamped, then refills the snapshot and runs one tick.
  - The C# scroll does not refill the cache (stale planes); not reproduced (finding).
- **`tick_due`:** also true in HISTORY while `interval_s != 0`, with the same `map_update_s` period.
- **`on_tick`, HISTORY branch** (:5370-5395):
  - `time.offline = advance(..)`.
  - `cache.clear()`, then `cache.set(p)` for each `index.latest_per_hex(min, offline)`.
  - `planes = cache.get_all(offline, ttl)`, then `tick(TickInput { now: offline, nearest_at: Utc::now(), .. })`.
  - Feeds the same result handling as LIFE, without the alarm sound.
- `markers` are filled from the loaded positions. C# never fills `AllLastUpdated` (:553, :6847), so its markers stay empty; not reproduced (finding).
- `pub fn marker_bins(markers: &[DateTime<Utc>], min, max, width_px: usize) -> Vec<bool>`: it has no division by zero when `min == max` (the C# divides by `Max - Min`, :6839-6855).

- [ ] **Step 1: Write the failing tests:**
  - `pipeline.rs`: `history_nearest_at_moves_planes_for_second_path`. Two paths. A plane crosses both at `now`. With `nearest_at = now + 1 day`, path 1 still finds it; path 2 sees it moved and gives 0. With `nearest_at = now`, both find it.
  - `history_mode.rs`:
    - `player_steps_and_back_goes_backwards`: Back from 0 → −1. Rewind twice → −21. Forward clamps at 3600.
    - `advance_clamps_to_range`.
    - `enter_history_stops_services_and_loads`: a fake records `stop_background` and `run_analysis(LoadHistory)`.
    - `leave_history_restarts_background`.
    - `empty_history_shows_message`.
    - `history_tick_uses_latest_per_hex_up_to_offline_time`.
    - `scroll_refills_snapshot`.
    - `inverted_range_rejected`.
    - `single_instant_markers_no_panic`.
    - `snapshot_200k_positions_is_fast`: 200 000 positions over 2000 hexes. `latest_per_hex` takes < 50 ms in a debug build (same style as the 3a timing test).
- [ ] **Step 2: Run** `cargo test -p airscout --test history_mode --test pipeline`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): HISTORY mode with player and snapshot tick`.

### Task 7: Analysis tab UI

**Files:**
- Create: `crates/airscout/src/ui/analysis.rs`
- Modify: `crates/airscout/src/app/mod.rs` (`BottomTab::Analysis`), `crates/airscout/src/ui/mod.rs`, `crates/airscout/src/ui/control.rs` (Play disabled in HISTORY)
- Test: `crates/airscout/tests/ui_analysis.rs`

**Interfaces (layout per survey §1.1 `tp_Analysis`):**
- **Mode:** ON/OFF radios. ON is enabled only while paused in LIFE.
- **Database group:**
  - "Load Positions": a list of `*.json|*.csv|*.hist` in `<data>/Tmp/`, multi-select, then `LoadFiles`. Remember `analysis.load_file_name`.
  - "Save Positions": a JSON/CSV choice. The file is `<data>/Tmp/Plane Positions {min:%Y_%m_%d_%H_%M_%S} to {max:…}.{ext}` (:6611-6636).
  - "Clear Positions": confirmation, then `ClearPositions` and a history reload. The C# leaves `AllPositions` and the UI untouched (:6673-6676); not reproduced (finding).
  - "Load History from URL" (Task 8).
  - The status line.
- **Functions:** Show All Traffic (Task 11), Export Path (Task 9), Crossing History (Task 10), Export Plane History (Task 9).
- **Time:** current offline time and step width [s].
- **Controls:** the five player buttons.
- **Player:**
  - Min/max date-time text fields (`%Y-%m-%d %H:%M:%S`, validated through `set_history_range`).
  - An egui `Slider` over `0..=(max-min).num_seconds()` calling `scroll_to`.
  - A marker strip painted under it from `marker_bins`.
- All buttons except OFF are disabled until `HistoryLoaded`, and while a job runs.
- `pub fn analysis_enabled(state: &AppState) -> AnalysisButtons` is pure and tested. `AnalysisButtons` is a struct of bools.

- [ ] **Step 1: Write the failing tests:**
  - `buttons_disabled_until_loaded`.
  - `on_disabled_while_playing`.
  - Kittest `player_buttons_change_interval`.
  - Kittest `save_writes_to_tmp_and_status_shows_path` (fake services record the path).
  - `play_button_disabled_in_history`.
- [ ] **Step 2: Run** `cargo test -p airscout --test ui_analysis`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): analysis tab`.

### Task 8: History from URL (generic downloader)

**Files:**
- Create: `crates/airscout/src/history_download.rs`, `crates/airscout/src/ui/dialogs/history_url.rs`
- Modify: `Cargo.toml` (workspace dep `fs4`, after user approval), `crates/airscout/Cargo.toml`, `crates/airscout/src/analysis.rs` (`AnalysisJob::Download`)
- Test: `crates/airscout/tests/history_download.rs`

**Interfaces (user decision B):**
- Same file format and import as C# (MapDlg.cs:8653-8920, HistoryFromURLDlg.cs), with a user URL and **no default**. It is off until a URL is entered and the disclaimer is accepted.
- **`pub fn archive_url(base: &str, date: NaiveDate) -> String`:** `base` + `/` (if missing) + `%Y-%m-%d.zip` (:8880-8890).
- **`pub fn step_entry_names(date, step_min: u32) -> Vec<String>`:**
  - `{date:%Y-%m-%d}-{HH:02}{MM:02}Z.json` for `i in (0..1440).step_by(step_min.max(1))` (:8740-8750).
  - Example: `2017-07-01-0005Z.json`.
- **`pub fn parse_aclist(json: &str, types: &dyn Fn(&str) -> Option<AircraftType>) -> Vec<PlaneInfo>`** reads `root.acList[]` (:8752-8800):
  - `Call`, `Lat`, `Long` → lon, `Trak` → track, `GAlt` → alt ft, `Spd` → speed kts, `Reg`, `Icao` → hex, `Type`, `PosTime` (unix ms) → time.
  - Missing values become `MIN_UTC`/NaN, as the helpers at :8672-8701 do.
  - The type is enriched with `AircraftTypeFindByICAO`.
- **`pub fn keep(p: &PlaneInfo, s: &Settings) -> bool`:** `check::plane(p)`, `planes_min_alt_m <= alt_m <= planes_max_alt_m`, inside `settings.area` (:8800-8815).
- **`pub trait DiskSpace { fn available(&self, dir: &Path) -> io::Result<u64>; }`:** `Fs4Disk` for the app, a fake for tests.
- **`DownloadRequest { url_base, date, step_min, dir: PathBuf }`. The job:**
  1. Creates `dir`.
  2. Refuses with "Not enough disk space to run this operation." if `available < 50 * 1024^3` (:8875-8878). No request is sent then.
  3. If `{dir}/{date}.zip` is missing, a streaming GET goes to `{date}.zip.part` with `as_net::user_agent()`. Progress `"Downloaded X.XX GB of Y.YY GB  (P.PP%)."` (:8655-8670). Cancel is checked per chunk. Rename on success.
  4. Reads the step entries straight from the zip. There is no extraction to disk. The C# extraction is commented out (:8730-8734).
  5. Writes `{dir}/{date}.csv` with the C# header `time;call;reg;hex;lat;lon;track;alt;speed;squawk;radar;type` and C# row format (`u` time, `F8` lat/lon, int track/alt/speed, squawk and radar empty).
  6. Imports through Task 4 `read_csv` + `bulk_insert`, then reloads the history.
  - The C# never imports the CSV and its worker ends before the download (:8864-8917). Both are not reproduced (finding).
  - Missing entries are counted: `"N steps imported, M missing"`.
- **HistoryFromURLDlg:**
  - Fields URL, directory (text, empty = `<data>/Tmp/History`), date, step [min].
  - A disclaimer like the plane feeds (Task 10 of 3b, `ui/dialogs/disclaimer.rs`), stored in `analysis.history_disclaimer_accepted`. Text: the archive server is third-party; its terms apply; the download can be ~8 GB and needs ~50 GB free (label HistoryFromURLDlg.Designer :94).
  - Start is disabled while the URL is empty or the disclaimer is not accepted.
  - The URL appears in status and error texts only with its query string stripped (`background::strip_query`).

- [ ] **Step 1: Write the failing tests** (mockito, temp dir, fake disk):
  - `archive_url_adds_slash`.
  - `step_entry_names_every_5_min`: 288 names, the first is `…-0000Z.json`.
  - `parse_aclist_fields`.
  - `keep_filters_alt_and_area`.
  - `low_disk_space_refuses_before_request`: mockito expects 0 hits.
  - `http_404_reports_error_and_imports_nothing`.
  - `corrupt_zip_is_error_not_panic`.
  - `missing_steps_are_counted`.
  - `download_imports_into_position_db`.
  - `existing_zip_is_not_downloaded_again`.
  - `cancel_stops_download`: slow mock; the job ends within 1 s.
  - `start_disabled_without_url_or_disclaimer` (pure dialog state).
- [ ] **Step 2: Run** `cargo test -p airscout --test history_download`. Expected: FAIL.
- [ ] **Step 3: Implement.** Pin `fs4` at the latest release with `features = ["sync"]`. Check that the windows-gnu cross build still links (CI).
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): history download from a user URL`.

### Task 9: Path CSV export and PlaneHistoryDlg

**Files:**
- Create: `crates/airscout/src/exports.rs`, `crates/airscout/src/ui/dialogs/plane_history.rs`
- Modify: `crates/airscout/src/analysis.rs` (jobs `ExportPath`, `PlaneLists`, `ExportPlane`; `JobEvent::PlaneLists { hexes, calls }`)
- Test: `crates/airscout/tests/exports.rs`

**Interfaces:**
- **Export Path** (MapDlg.cs:6694-6825):
  - Job `ExportPath { entry: PathEntry, model: ElevationModel, max_alt_m: f64, my_call, dx_call }`. It takes the SINGLE path the app already computed (`single_path()`); without one, the status says "no path".
  - The job runs `store.find_or_create_path(model, lat1, lon1, lat2, lon2, model.default_step_width_m(), true, cancel)`.
  - One row per elevation point `i`: `dist = i * step / 1000`, `p = path.info_point(dist)`.
  - Columns:
    - `dist`, `p.lat`, `p.lon`, `elv[i]`
    - `p.h1`, `p.h2`, `max(h1,h2)`, `max_alt_m`, `p.f1`
    - `eps1 = epsilon_from_heights(params.h1, dist, elv, radius)°`
    - `eps2 = epsilon_from_heights(params.h2, D - dist, elv, radius)°`
    - `eps1_min°`, `eps2_min°`, the model name
  - Header `Distance[km];Lat[deg];Lon[deg];Elevation[m];Min_h1[m];Min_h2[m];Min_h[m];Max_h[m];F1[m];eps1[deg];eps2[deg];eps1_min[deg];eps2_min[deg];ElevationModel`.
  - The C# header has 14 columns but rows have 13 (finding, not reproduced GUI).
  - File `<data>/Tmp/Path Information {my} to {dx}.csv`, with `/` replaced by `_`.
- **PlaneHistoryDlg** (PlaneHistoryDlg.cs:66-165):
  - Job `PlaneLists { from, to }` gives `get_all_hex`/`get_all_calls`.
    - Hex filtered by `convert::to_hex` (6 upper-case hex chars).
    - Calls filtered by `call_shape` (length ≥ 4, no `-`) plus the airline check `check::call` (PlaneInfoConverter.cs:120, :268).
  - Two combos, Hex "or" Call. Typing in one clears the other.
  - Export (job `ExportPlane { key: HexOrCall, from, to }`, with `pub enum HexOrCall { Hex(String), Call(String) }`):
    - Header `time;hex;call;lat[deg];lon[deg];alt[m];track[deg];speed[kmh];dist[km];av_speed[km/h]`.
    - Rows: `time %Y-%m-%d %H:%M:%S`, alt `ft_m`, speed `kts_kmh`.
    - For i > 0: `dist = latlon::distance(prev, cur)`, `av_speed = dist / hours(cur - prev)`. Row 0 has 0/0.
    - Ported 1:1, so a zero time difference writes `Infinity`/`NaN` (finding).
  - File `<data>/Tmp/Plane History_{key}_{from:%Y%m%d_%H%M%S}_to_{to:…}.csv`.
  - The C# exports on the UI thread with concatenated SQL and an ignored cancel; not reproduced (finding).

- [ ] **Step 1: Write the failing tests:**
  - `path_csv_row_count_and_14_columns` (synthetic elevation store as in the 3b horizon tests).
  - `path_csv_eps_values`: one row checked against `epsilon_from_heights` by hand.
  - `plane_lists_filtered`.
  - `plane_history_dist_and_av_speed`.
  - `plane_history_same_second_writes_infinity`.
  - `plane_history_by_call_mixes_hexes` (C# behaviour).
- [ ] **Step 2: Run** `cargo test -p airscout --test exports`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): path CSV export and plane history dialog`.

### Task 10: CrossingHistoryDlg

**Files:**
- Create: `crates/airscout/src/crossing_history.rs` (pure algorithm and CSV), `crates/airscout/src/ui/dialogs/crossing_history.rs`
- Modify: `crates/airscout/src/analysis.rs` (job `Crossings`; `JobEvent::Crossings(Box<CrossingResult>)`)
- Test: `crates/airscout/tests/crossing_history.rs`

**Interfaces (CrossingHistoryDlg.cs):**
- **Range** (:38-57): `from` is cut to 00:00 and clamped up to `oldest`. `from > to` sets `from = to`. Step 60 s (MapDlg.cs:6835), minimum 1.
- **Path** (:59-127): `pub fn crossing_path(paths: &Paths, s: &Settings) -> Result<PropagationPath, String>`.
  - Port the C# path set-up 1:1:
    - `LocationFindOrCreate` with a 6-char locator (`LocFromLatLon(.., 3)`).
    - QRV defaults when 0.
    - The best-case shift when `path_best_case_elevation` is set and the locator is not precise.
    - `LocalObstructionFind(my)` toward bearing12.
    - `find_or_create_path` with `R·K`, F1 clearance, the default step width of the effective model.
  - Reuse `services` helpers (`ground_elevation`, `best_case::ui_position`, obstruction lookup) where the C# matches `UpdatePaths`. Name every difference in the module doc and in the finding.
- **Types:** `pub struct CrossingParams { pub from, pub to, pub step_s: i64, pub ttl_min: i64, pub max_radius_km: f64, pub max_distance_km: f64, pub max_alt_m: f64, pub max_delta_m: f64, pub with_signal_level: bool }` (from `Settings`); `pub struct CrossingResult { pub crossings: Vec<PlaneInfo>, pub count_hi: Vec<(DateTime<Utc>, u32)>, pub count_lo: Vec<(DateTime<Utc>, u32)> }`. The signal lookup is passed as `signal: &dyn Fn(DateTime<Utc>) -> Option<f64>`.
- **Algorithm:** `pub fn crossing_history(positions: &mut Vec<PlaneInfo>, path: &PropagationPath, p: &CrossingParams, signal: &dyn Fn(DateTime<Utc>) -> Option<f64>, cancel: &AtomicBool, progress: &mut dyn FnMut(usize)) -> Vec<PlaneInfo>`. It ports the worker :137-217:
  1. Preselect `from <= time <= to` and `distance(mid, pos) <= D/2`.
  2. `time = first.time`.
  3. Loop while `time <= to`:
     - Build a fresh cache.
     - From `startindex`, insert every item with `time >= t - ttl`, remember the first such index, and stop after the first item with `time > t` (that item is already inserted).
     - `get_all(t, ttl)`, then `get_nearest_planes(t, …, planes_filter_max_circumcircle_km, max_distance, max_alt, max_delta)`.
     - Keep `int_qrb <= max_distance`. With `crossing_with_signal_level`, keep only planes with a stored `SignalDb::find(plane.time)`, written into `signal_strength`, else `f64::MIN`.
     - `t += step`.
  - **Aliasing 1:1, inside the dialog only.** The C# cache holds references to the preselected objects, and `InsertOrUpdateIfNewer` writes newer reports into the first object of a hex. Reproduce this with a cache of indices into `positions`: an update mutates `positions[first_index_of_hex]`.
  - The caller passes a **copy** of the history positions. The C# `ref AllPositions` corrupts the main history too; that is not reproduced (finding).
  - Crossings keep the report time and position (finding #24 interplay), and duplicates per step stay (1:1).
  - **Not reproduced (GUI):** the `startindex == -1` crash (:170-172) ends the loop with the results so far. The per-plane progress flood becomes progress per step.
- **Chart** (:233-299): buckets keyed by `plane.time` (1 s). Potential 100 goes to `count_hi` (red), everything else to `count_lo` (blue). An egui_plot bar chart.
- **CSV** (:302-402):
  - `pub fn mark_ambiguous(c: &mut [PlaneInfo], gap_s: f64)` is the ambiguity pass, ported 1:1 with its quirks (:311-336): index 0 is never reset or added, the hex at `i` joins the closed group, and the last item is always ambiguous.
  - Header with separators: the C# joins the names without a separator (:349-369); not reproduced (finding).
  - 21 fields (:377-397); signal `f64::MIN` is written as `-255`.
  - File `<data>/Tmp/Path Crossing History_{my}_{dx}_{from:%Y%m%d_%H%M%S}_to_{to:…}.csv`.
- **Dialog:**
  - Calculate (job) and Back (cancel). Export is enabled after a complete run.
  - The settings checkbox `crossing_with_signal_level` and the gap field `crossing_ambiguous_gap_s`.
  - Opening requires loaded history with `max > min` (MapDlg.cs:6827-6837).

- [ ] **Step 1: Write the failing tests:**
  - `crossing_history_finds_plane_on_path`.
  - `aliasing_updates_first_object_of_hex`.
  - `caller_positions_untouched`: the app keeps its history; the job runs on a clone.
  - `signal_level_filter`.
  - `startindex_gap_ends_loop_without_panic`.
  - `mark_ambiguous_quirks` (a hand-built list pins each quirk).
  - `csv_header_has_separators_and_21_fields`.
  - `chart_buckets_hi_lo`.
- [ ] **Step 2: Run** `cargo test -p airscout --test crossing_history`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): crossing history dialog`.

### Task 11: TrafficDlg

**Files:**
- Create: `crates/airscout/src/traffic.rs`, `crates/airscout/src/ui/dialogs/traffic.rs`
- Modify: `crates/airscout/src/analysis.rs` (job `Traffic { from, to }`; `JobEvent::Routes(Vec<Vec<GPoint>>)`)
- Test: `crates/airscout/tests/traffic.rs`

**Interfaces (TrafficDlg.cs):**
- Start/stop default to `oldest`/`youngest`. An empty DB uses `Utc::now()` for both (:61-68).
- **`pub fn split_routes(points: &[AircraftPosition]) -> Vec<Vec<GPoint>>`:**
  - A new segment starts when `distance(last, p) >= 100` km (:125).
  - The point that triggers the split **starts** the new segment. The C# drops it; not reproduced (finding).
  - Segments with fewer than 2 points are dropped.
- **Job:** one query `get_all(from, to)` grouped by hex in memory, instead of the C# N+1 queries (:101-143, finding). Cancel is checked per hex.
- **Window:**
  - A walkers map with the area rectangle (magenta, width 3, no fill, :32-57).
  - Routes drawn black with alpha `opacity% * 255 / 100`. Opacity is 10..=100, default 10.
  - Start/stop fields `%Y-%m-%d %H:%M` and a "Show" button.
  - Routes are thinned with `ui::map_geom::thin` before drawing.

- [ ] **Step 1: Write the failing tests:**
  - `split_keeps_trigger_point`.
  - `single_point_routes_dropped`.
  - `traffic_job_groups_by_hex`.
  - `empty_db_defaults_to_now`.
- [ ] **Step 2: Run** `cargo test -p airscout --test traffic`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): traffic dialog`.

### Task 12: Database maintenance (Options Database tab, maintenance dialog, single station delete, copyright, license)

**Files:**
- Modify:
  - `crates/as-db/src/lib.rs`
  - `crates/as-stations/src/db.rs`, `crates/as-aircraft/src/db.rs`, `crates/as-aircraft/src/positions.rs`, `crates/as-signals/src/lib.rs`
  - `crates/as-elevation/src/path.rs`, `crates/as-elevation/src/model.rs`, `crates/as-propagation/src/engine.rs`
  - `crates/airscout/src/ui/options/database.rs`, `crates/airscout/src/ui/options/elevation.rs`, `crates/airscout/src/ui/options/info.rs`
- Create: `crates/airscout/src/maintenance.rs`, `crates/airscout/src/app/maintenance.rs`, `crates/airscout/src/ui/dialogs/db_maintenance.rs`, `crates/airscout/src/ui/dialogs/delete_station.rs`
- Test: `crates/as-db/tests/maintenance.rs`, `crates/airscout/tests/maintenance.rs`

**Interfaces:**
- **`as_db`** (ScoutBase/ScoutBase.Database/ScoutBaseDatabase.cs:124-163):
  - `table_list(&Connection) -> Result<Vec<String>>`: from `sqlite_master` where `type='table'`, without `sqlite_%`.
  - `row_count(&Connection, table) -> Result<u64>`.
  - `clear_table(&Connection, table) -> Result<usize>`: one transaction.
  - `select_non_blob(&Connection, table, limit: usize) -> Result<(Vec<String>, Vec<Vec<String>>)>`: columns from `PRAGMA table_info` whose type is not BLOB.
  - Every helper checks `table` against `table_list` and returns `Err` otherwise.
- **Connection access:**
  - `with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R` on `StationDb`, `AircraftDb`, `PositionDb`, `SignalDb`.
  - `PropagationEngine::with_conn(model, f)`. `ElevationStore::with_conn` exists.
- **Descriptions:** `pub fn table_description(table: &str) -> &'static str`, with the C# texts verbatim:
  - ElevationDatabase.cs:133-136, StationDatabase.cs:53-55, PropagationDatabase.cs:95-96, AircraftDatabase.cs:58-62.
  - Unknown tables give `"[not found]"`.
- **Core deletes:**
  - `ElevationStore::elevation_path_delete_all(model) -> Result<usize>` (ElevationDatabase.cs:1320-1331).
  - `PropagationEngine::path_delete_all(model) -> Result<usize>` (PropagationDatabase.cs:470-481).
  - `StationDb::locations_like(pattern: &str) -> Result<Vec<Location>>` (bound `LIKE`, StationDatabase.cs:645-655).
- **Copyright:** `ElevationModel::copyright(self) -> &'static str`, the `Elevation_*_Copyright` defaults verbatim (Settings.settings:422, :434, :455, :610, :635).
- **`maintenance.rs`:**
  - `pub enum DbId { Stations, Aircraft, Positions, Signals, Elevation(ElevationModel), Propagation(ElevationModel) }`.
  - `pub enum MaintenanceJob { Sizes, Tables(DbId), ClearTable(DbId, String), View(DbId, String), DeleteElevationPaths, DeletePropagationPaths, DeleteMapTiles, DeleteStation { call, loc }, Locations(String) }`.
  - `JobEvent` gains `Sizes(Vec<(DbId, String, f64)>)`, `Tables(DbId, Vec<(String, String, u64)>)`, `View { columns, rows }`, `Locations(Vec<(String, String)>)`.
  - `Services::run_maintenance(id, job)` (default no-op) reuses `JobRunner`.
- **Options Database tab** (OptionsDlg.cs:296-520):
  - For each DB: path (shortened) and size in MB (F0), plus the total.
  - "Delete All Elevation Paths" and "Delete All Propagation Paths" for **all five** models. The C# does GLOBE/SRTM3/SRTM1 only (:385-403); not reproduced (finding).
  - "Delete All Map Tiles" removes the walkers cache directory `tiles::tiles_cache_dir`, **with** confirmation. The C# has none (:405-408, finding).
  - "Delete Single Station" opens the dialog.
  - "Database Maintenance" per DB.
  - The Options dialog is only open while paused (3b). Deletes are additionally refused while a job runs.
- **DatabaseMaintenanceDlg** (:22-130):
  - Rows: table, description, row count, "Delete All" (YesNo confirmation, then `ClearTable`, then refresh the count), "View".
  - View shows the CAUTION confirmation, then a read-only `egui_extras` table of the first **10 000** rows (C# loads all; finding), with Find Next (F3) as DatabaseEditDlg.cs:35-116.
  - No write-back and no `LastUpdated` stamping. The C# stamps only the in-memory table (:118-129, finding).
- **DeleteSingleStationDlg** (:22-66):
  - The call field is upper-cased and trimmed. With ≥ 2 chars, a `Locations("%text%")` job runs.
  - The locator combo holds the locators of the chosen call.
  - Delete, after confirmation, runs `location_delete(call, loc)` and `qrv_delete(call, loc, Band::All)`. The C# Delete button has no handler (finding).
  - The help text says only own entries go for good; global ones return with the next update (`.resx:121-122`).
- **ElevationCopyrightDlg:** a "Copyright" button per model on the Elevation tab shows `copyright()`.
- **LicenseDlg:** a "Show license" button on the Info tab opens a window with the workspace `LICENSE` (`include_str!`).

- [ ] **Step 1: Write the failing tests:**
  - `as-db`:
    - `table_list_row_count_clear`.
    - `unknown_table_is_rejected` (`"x; DROP TABLE y"` gives Err).
    - `select_skips_blob_columns_and_limits`.
  - `airscout`:
    - `delete_all_needs_confirmation`: a pure dialog state; the job is only sent after confirm.
    - `maintenance_refused_while_playing`.
    - `delete_paths_all_five_models`.
    - `delete_station_removes_location_and_qrv`.
    - `locations_like_needs_two_chars`.
    - `sizes_listed_for_all_dbs`.
    - `table_descriptions_verbatim`.
- [ ] **Step 2: Run** `cargo test -p as-db --test maintenance` and `cargo test -p airscout --test maintenance`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit** `feat: database maintenance dialogs and delete buttons`.

### Task 13: Elevation coverage maps and the station-map elevation grid

**Files:**
- Modify: `crates/as-elevation/src/store.rs` (`tile_exists`), `crates/as-elevation/src/lib.rs` (`elevation_color`)
- Create: `crates/airscout/src/coverage.rs`, `crates/airscout/src/ui/options/coverage.rs`
- Modify: `crates/airscout/src/ui/dialogs/map_station.rs`, `crates/airscout/src/ui/options/elevation.rs`, `crates/airscout/src/maintenance.rs` (jobs `Coverage(ElevationModel)`, `ElevationGrid { rect: GRect, model: ElevationModel }`; `JobEvent::Coverage { found: Vec<String>, missing: Vec<String> }`, `JobEvent::ElevationGrid(Vec<(GRect, i16)>)`)
- Test: `crates/as-elevation/tests/coverage.rs`, `crates/airscout/tests/coverage.rs`

**Interfaces:**
- **Core:**
  - `ElevationStore::tile_exists(model, index6: &str) -> Result<bool, ElevationError>` (ElevationDatabase.cs:949-969).
  - `pub fn elevation_color(value: f64) -> [u8; 3]` ports `GetElevationColor` (:520-529): `/100`, clamp 0..100, then `DEMColorPalette` with the stops at :2059-2066. NaN maps to 0.
- **Coverage (OptionsDlg.cs:1356-1461):**
  - Job `Coverage(model)` runs `Catalogue::fetch(client, base_url, model, area, <data>/tmp)`. This is the small `files.zip`, a conditional download from the elevation server the updaters use.
  - Then, for each catalogue key, `tile_exists(&key[..6])`. The job sends `JobEvent::Coverage { found: Vec<String>, missing: Vec<String> }`.
  - It starts **only from a "Show coverage" button** per model (explicit action). C# starts on tab Enter.
  - `pub fn tile_cell(index6) -> GRect`: bottom-left of the 6-char locator, 1/24° lat × 2/24° lon.
  - Drawing:
    - Found tiles: green α50 stroke and fill. Missing tiles: red α50.
    - The area rectangle: magenta, width 3.
    - Status `"{n} tile(s) found, {m} more tile(s) available and missing."`.
  - Closing the view cancels the job. The C# also zooms the General tab map by mistake (:1449 etc.); not reproduced (finding).
- **Elevation grid (MapStationDlg.cs:269-424):**
  - Shown when `gui.stations_map_overlay_elevation` and zoom ≥ 17. A checkbox "Overlay elevation grid (zoom ≥ 17)".
  - `pub fn grid_cells(view: GRect) -> Vec<GRect>`: the view is snapped to 6-char locator bounds. Steps lat `5.5556e-4 / 2`, lon `5.5556e-4`. Only cells intersecting the view are kept, capped at 2000 cells (the 3a grid cap rule).
  - Job `ElevationGrid { rect, model: effective_model() }` gives `(cell, elv)` from `store.elevation(model, centre, false)`. This is the selected model only, ported 1:1 (:320); it differs from `GetElevation` (finding).
  - Colour: `elevation_color((elv - min) / (max - min) * 10000)` with α100, normalised over the view. With `max == min` the C# gives NaN; the port uses 0 (finding).
  - Label `"{elv} m"`, centred, black.
  - Re-run on zoom/pan with a newer job id, replacing the C# busy-wait with DoEvents (:278-282).

- [ ] **Step 1: Write the failing tests:**
  - `tile_exists_true_false`.
  - `elevation_color_stops` (0, 100 and one mid stop, against the C# palette).
  - `tile_cell_geometry` (JO50AA).
  - `coverage_job_splits_found_missing` (mockito `files.zip` built in the test, temp store with one tile).
  - `grid_cells_step_and_cap`.
  - `grid_colour_flat_view_no_nan`.
  - `grid_hidden_below_zoom_17`.
- [ ] **Step 2: Run** `cargo test -p as-elevation --test coverage` and `cargo test -p airscout --test coverage`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): elevation coverage maps and station map elevation grid`.

### Task 14: CleanupDlg (`--clean`)

**Files:**
- Create: `crates/airscout/src/cleanup.rs`, `crates/airscout/src/ui/dialogs/cleanup.rs`
- Modify: `crates/airscout/src/main.rs`, `crates/airscout/src/app/shell.rs`
- Test: `crates/airscout/tests/cleanup.rs`

**Interfaces (CleanupDlg.cs:42-104, MapDlg.cs:632-646):**
- `airscout --clean` (also `/CLEAN`, `-CLEAN`) shows the dialog **before** any database is opened.
- `pub struct CleanupChoice { settings, aircraft_db, scoutbase_db, tmp, tiles }`, all `true` by default (Designer :59-160).
- `pub fn targets(c: &CleanupChoice, settings_path: &Path, data_dir: &Path) -> Vec<PathBuf>`:
  - Settings file.
  - `aircrafts.db3`, `aircraftpositions.db3`, `signals.db3`.
  - `stations.db3`, each `ElevationModel::db_file_name()`, each `propagation_<…>`, plus their `-wal`/`-shm` files.
  - `Tmp`/`tmp` dirs, `tiles_cache_dir`.
  - The C# Log, legacy Elevation and Plugin dirs do not exist in the port.
  - Paths are computed **before** anything is deleted. The C# resets the settings first and then reads the DB dirs from the reset values (:49-63); not reproduced (finding).
- `pub fn run(targets) -> Result<(), Vec<String>>`: deletes files and dirs and collects errors. "Retry" re-runs; "Continue" starts the app.
- Closing the window, or Cancel, exits without deleting (C# `Abort` → exit -1).

- [ ] **Step 1: Write the failing tests:**
  - `targets_cover_all_db_files`.
  - `targets_computed_before_delete`.
  - `run_deletes_and_reports_errors` (temp dir with a read-only file where possible; otherwise a missing path is not an error).
  - `cleanup_close_deletes_nothing` (pure dialog state).
  - `clean_flag_parsed` (`/CLEAN`, `-CLEAN`, `--clean`).
- [ ] **Step 2: Run** `cargo test -p airscout --test cleanup`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): cleanup dialog (--clean)`.

### Task 15: Tracking values (bw_Track maths and display)

**Files:**
- Create: `crates/airscout/src/tracking.rs`, `crates/airscout/src/app/tracking.rs`, `crates/airscout/src/ui/options/track.rs`
- Modify: `crates/airscout/src/style.rs` (tooltip lines), `crates/airscout/src/ui/map.rs` (gauges/rig box, right-click DX), `crates/airscout/src/ui/status.rs` (TRK), `crates/airscout/src/ui/options/mod.rs`
- Test: `crates/airscout/tests/tracking.rs`, `crates/airscout/tests/style.rs`

**Interfaces (MapDlg.cs:8054-8155, :8251-8281, TrackValues.cs):**
- **`TrackValues`:**
  - Fields: `timestamp`, `hex`, `my_azimuth`, `my_elevation`, `my_distance`, `my_slant_range`, `my_doppler`, `dx_*` the same, `rx_hz: i64`, `tx_hz: i64`.
  - Defaults: NaN for az/el/dist/slant, 0 for the Dopplers and frequencies.
- **`pub fn track_values(at, plane: Option<&PlaneInfo>, mode: TrackMode, s: &Settings, my_h: f64, dx_h: f64, old: Option<&TrackValues>) -> TrackValues`:**
  - **TRACK** (:8073-8131), with `R' = 6371 * band K`:
    - `az = bearing(my, plane)`.
    - `dist = distance(my, plane)`. This is not K-scaled.
    - `slant = slant_range_from_heights(my_h, dist, alt_m, R')`.
    - `el = epsilon_from_heights(my_h, dist, alt_m, R')°`.
    - DX the same with `dx_h`.
    - Doppler only when `old` has the same hex and `timediff > 0`: `speed = (old.slant - new.slant) * 1000 / timediff` [m/s], `doppler = doppler_shift(band.to_hz(), speed)`.
  - **SINGLE** (:8134-8139): `my_azimuth = track.set_az`, `my_elevation = track.set_el`. Nothing else.
  - **NONE**: all NaN.
- `pub fn valid(v: &TrackValues) -> bool`: az and el are not NaN and `0 <= az < 360` (:8142-8145).
- **`pub fn rx_tx(strategy: DopplerStrategy, dial: i64, my: f64, dx: f64) -> (i64, i64)`:** each Doppler is cast to `i64` separately (:8251-8281).
  - A: `(dial + my + dx, dial)`
  - B: `(dial, dial - my - dx)`
  - C: `(dial + my + dx, dial - my - dx)`
  - D: `(dial + my, dial - my)`
  - None: `(0, 0)`
- **State (`app/tracking.rs`):**
  - `pub track_mode: TrackMode`, `pub track: Option<TrackValues>`, `track_heights: Option<(f64, f64)>`.
  - The heights are `ground_elevation + my_height_m` / `dx_height_m` (:8082-8083), set once per Play through a new `Services::ground_elevation(lat, lon) -> f64` (default 0).
  - Every 1 s while playing in LIFE with `track.activate`, values are computed from `cache.get(selected, now + offset_s, ttl)`. C# sleeps a fixed 1000 ms in the inner loop; `Track_Update` applies only to the outer loop (:8313-8315). The port uses 1 s and does not read `update_ms` (finding).
  - Tracking is off in HISTORY (C# stops `bw_Track`).
- **Transitions** (:5478-5510, :5533-5546, :5027-5030, :5086-5089):
  - A click on a plane while playing with `track.activate` sets `TRACK`; the current plane is the selected hex.
  - Deselecting sets `NONE`.
  - A right-click on the DX marker sets `SINGLE` with `set_az = QTF`, `set_el = 0`.
  - A tick without a selected plane sets `NONE` (1:1, so SINGLE lasts one tick).
  - Pause sets `NONE` (:4099).
- **Display:**
  - Gauges: only when `gui.tracking_gauges_show && playing && TrackMode::Track && track.is_some()` (:5602-5662). An Az/El readout box at the map's top-left.
  - Rig box: when `strategy != None`, lines `MyDop`, `DXDop` (F0), `Dial`, `RX`, `TX`, in monospace.
  - `tooltip_lines` gains `track: Option<&TrackValues>`. For detailed tooltips, when `TrackMode::Track` and values exist (:4913-4940):
    - The separator, then `MyAzimuth: {:05.2}°`, `MyElevation: {:05.2}°`, `MyDoppler: {:.0}Hz`, each gated by its `InfoWin` flag.
    - The same for DX.
    - Shown on every detailed tooltip, not only the tracked plane (1:1).
  - Update the `style.rs:2` module doc.
  - TRK status field (`SayTrack`, :8361-8378): NONE DarkGray/Control, STOPPED Control/DarkGray, SINGLE/TRACKING White/DarkGreen, ERROR Yellow/Red.
- **Options "Track" tab, minimal:** activate, offset [s], Doppler strategy, dial frequency [Hz], gauges shown. A note says the outputs follow in stage 4.

- [ ] **Step 1: Write the failing tests:**
  - `track_values_az_el_dist_slant` (generic coordinates, values cross-checked against `as_geo` calls).
  - `doppler_needs_previous_same_hex`.
  - `doppler_sign_positive_when_approaching`.
  - `single_mode_uses_set_az_el`.
  - `none_is_invalid`.
  - `rx_tx_strategies` (A–D, None).
  - `click_while_playing_sets_track_and_deselect_none`.
  - `right_click_dx_sets_single_with_qtf`.
  - `no_selection_tick_resets_to_none`.
  - `tracking_off_in_history`.
  - `style.rs`: `tooltip_tracking_lines_gated_by_infowin`, `simple_tooltip_has_no_tracking_lines`.
- [ ] **Step 2: Run** `cargo test -p airscout --test tracking --test style`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): tracking values, gauges and tooltip lines`.

### Task 16: Docs, findings, gates

**Files:**
- Modify: `README.md` (status rows: 3c done; the desktop app section covers HISTORY, the analysis dialogs, maintenance, `--clean`, tracking display; `fs4`), `docs/logic-findings.md`

**Steps:**
- [ ] **Step 1:** Add findings from **#106**, in this order. Each cites C# file:line and says ported 1:1 / Not reproduced (GUI) / Not ported / Port deviation.
  1. History from URL: the default URL `http://history.adsbexchange.com/Aircraftlist.json/` (Settings.settings:389) is dead/paid and dropped. No default; off until the user enters a URL and accepts the disclaimer; the 50 GB check is kept.
  2. Player Back/Rewind clamp (MapDlg.cs:6865-6881). Not reproduced (GUI).
  3. #86 resolved: `GetNearestPlanes(UtcNow)` in HISTORY ported 1:1 (:5140), with its effect on later MULTI paths. Update the #86 row itself.
  4. History GUI group: `AllLastUpdated` never filled (:553, :6847); the Max picker clamps Min (:6934-6942); the scroll does not refill the cache (:6908-6922); TTL added twice after Load (:9856-9857); Clear leaves `AllPositions` (:6673-6676); the maintainer keeps running in HISTORY (:2477-2504). Not reproduced, one row each.
  5. Position files: JSON save not readable by its loader, trailing comma (:9505-9532, :9606-9653); CSV local-time parse (:9739-9796); SQL concatenation in the position DB. Not reproduced.
  6. `.hist` FR24 lookup with spoofed UA and cookie, and the Hex not updated (:9655-9737). Not ported.
  7. Downloader: worker ends before the download, extraction commented out, CSV never imported (:8730-8734, :8864-8917). Not reproduced.
  8. CrossingHistory: aliasing (1:1 inside the dialog, main history copy not corrupted), report time and position (#24), duplicates, the `startindex -1` crash, per-plane progress (CrossingHistoryDlg.cs:137-217, AircraftDatabase.cs:2520-2530).
  9. CrossingHistory CSV header without separators (:349-369). The ambiguity quirks are ported 1:1 (:311-336).
  10. Path CSV: 14 header columns, 13 values (:6786-6799).
  11. PlaneHistory: `av_speed` Infinity/NaN (1:1); UI-thread export, ignored cancel, SQL concatenation (not reproduced).
  12. Traffic: split point dropped, N+1 queries (TrafficDlg.cs:101-143).
  13. Maintenance: path deletes skip ASTER, map tiles deleted without confirmation, the single-station Delete without a handler, the Map DB maintenance button without a handler, no worker pause (OptionsDlg.cs:385-474, DeleteSingleStationDlg.Designer :88-98).
  14. Maintenance View: read-only grid, the `LastUpdated` stamp only in memory, the port caps at 10 000 rows (DatabaseMaintenanceDlg.cs:71-130, DatabaseEditDlg.cs:118-129).
  15. Dead dialogs not ported: ScoutBase/AircraftDatabaseMaintenanceDlg (the copy-paste deletes station tables), ReflectionDlg.
  16. CleanupDlg: reset before reading the DB dirs (CleanupDlg.cs:49-63). Not reproduced.
  17. Coverage maps: start on tab Enter, the General map zoomed by mistake (OptionsDlg.cs:1449 etc.). Not reproduced; explicit button.
  18. Station-map elevation grid: selected model only (1:1); NaN colour on a flat view (not reproduced); cell cap.
  19. Tracking: fixed 1 s loop instead of `Track_Update` (:8313-8315); `DopplerShift` doc says positive = receding while the caller passes positive = approaching (the result is physically right; doc only, also in `as_geo::propagation::doppler_shift`); values on every detailed tooltip; SINGLE reset by `DrawPlanes`; `MyDistance` not K-scaled. All ported 1:1.
- [ ] **Step 2:** Run the full gate locally on macOS, then the Linux docker gate (`apt-get install -y libasound2-dev pkg-config`, `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`). Expected: green.
- [ ] **Step 3: Commit** `docs: stage 3c README and logic findings`.

## Open questions (user)

1. Task 8 adds the `fs4` crate for the free disk space check, which is a new dependency. Alternatives are `sysinfo` (larger) or dropping the check, but the user wants it kept. Is `fs4` OK?
