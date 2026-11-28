# Stage 3b — MULTI Mode, Dialogs and Background Workers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the stage-3a live map into a configurable app:
- MULTI mode with a watchlist.
- The Options dialog and the station, local obstruction and horizon dialogs.
- Feed settings and the feed disclaimer.
- Background database updaters with status LEDs, and the background path calculator.
- The Spectrum Lab spectrum, offline time, the alarm sound, and the first-run wizard.

**Architecture:**
- Pure state and logic stay testable without a display: `pipeline.rs`, `app.rs` behind `trait Services`, and new service modules in `crates/airscout/src/`.
- Long-running work runs on std threads. The threads talk to the UI through the existing `ServiceEvent` channel and share one cancel flag. On exit the app sets the flag and detaches the threads, as in 3a.
- Dialogs are egui windows that edit a **copy** of the settings. "OK" applies the copy; "Cancel" drops it.
- Small core gaps (local obstruction storage, `LocationGetAll`) are filled in the core crates, each with its own tests.

**Tech Stack:**
- Rust 1.95+, eframe/egui 0.37, egui_plot 0.38, egui_extras 0.37, walkers 0.66, egui_kittest 0.37 (dev).
- rodio for the alarm sound, behind the default cargo feature `sound`.
- Existing crates: as-geo, as-elevation, as-propagation, as-stations, as-aircraft, as-feeds, as-signals, as-settings, as-net.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md` (stage 3).

**Inputs:**
- GUI survey: `docs/superpowers/surveys/2026-11-23-gui-survey.md`.
- Stage-3a ledger: `docs/superpowers/ledgers/2026-11-23-stage3a-live-map.md`.
- Upstream source: `target/upstream/AirScout/` @ `91b02cb7`. C# references in this plan are relative to `target/upstream/AirScout/AirScout/` unless they start with `ScoutBase/`.

**Stage split:**
- **3b (this plan):**
  - MULTI mode, the watchlist and WatchlistDlg.
  - OptionsDlg: General, Database (update mode only), Stations, Map, Elevation, Path with the band grid, Planes, Alarm, SpecLab, Watchlist, Info.
  - MapStationDlg, LocalObstructionsDlg, HorizonDlg.
  - Feed settings and disclaimer dialogs.
  - Background updaters (stations, aircraft, elevation, position maintainer) with status LEDs.
  - PathCalculator.
  - SpecLab spectrum and the nearest-plane box.
  - SetTimeDlg, the alarm sound, FirstRunWizard.
  - Thunderforest with a user API key, QRV antenna gain, local obstructions in paths, the `Elevation_*_Enabled` flags.
- **Moved out of 3b:**
  - **Map preloader** moves to stage 5, together with `as-maps` and `maptile-gen`. The C# preloader pulls zoom 5–11 over the whole area, which is a bulk download. The OSM tile usage policy forbids that against `tile.openstreetmap.org`, and the C# preloader used AirScout's own tile server instead.
  - Database delete and maintenance buttons and per-model tile coverage maps move to 3c.
  - The Track, CAT and Network tabs move to stage 4.
- **Not ported (ledger and findings):**
  - QRZ lookup: it scrapes qrz.com and sends the call outward.
  - Location/QRV upload.
  - MapProviderDlg acceptance: it is keyed to the Windows registry.
  - `gm_Nearest`, the second map in the Spectrum tab.
  - News feed and planes.json writer.

## Global Constraints

- **Port policy:**
  - Prediction-affecting behaviour is ported 1:1, even when it is wrong, and logged in `docs/logic-findings.md`. The next finding number is **#88**.
  - GUI-layer upstream bugs are **not** reproduced. Each still gets a findings entry marked "Not reproduced (GUI)".
- Pixel-identical UI is a non-goal. The layout follows the original's information architecture.
- **Anti-goal:** the UI must not stutter with hundreds of live aircraft or while background workers run. No path, horizon or database bulk work runs on the UI thread.
- **Secrets:**
  - No secrets in the repo.
  - Credentials (feed keys, the Thunderforest key) come only from user settings.
  - They never appear in logs, `Debug` output, status lines, error texts or the User-Agent.
- **Network:**
  - Every request uses the honest User-Agent `as_net::user_agent()`. No spoofing.
  - Requests to outside services never carry the user's call sign or position beyond what the feed's documented API needs.
  - `getkey.php`, session keys, register, upload and plugin download are **not** ported.
- **Settings:**
  - Settings are TOML via `as-settings`.
  - New fields get `#[serde(default)]` defaults taken from `AirScout/Properties/Settings.settings`.
  - A settings file from stage 3a (new fields missing) must load unchanged.
- Every crate has `#![forbid(unsafe_code)]`.
- **Gate:**
  - Command: `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked`.
  - CI must stay green on self-hosted Linux, the `x86_64-pc-windows-gnu` cross build, and hosted macOS/Windows.
- **Tests:**
  - Tests need no network (mockito for HTTP) and no display (`egui_kittest` without snapshots).
  - Tests need no audio device: the sound player is behind a trait and tests use a fake.
- No file dialogs (no `rfd`), to avoid GTK on Linux. Exports go to `<data dir>/Tmp/` and the status line shows the full path.

## Review Focus

1. **airscout.eu unreachable or HTTP 500 during background updates.**
   - Expected: the LED turns red with the error text in its tooltip, the map stays usable, and exit is immediate even mid-download.
   - Test: Task 11 `updater_error_sets_error_led` and `cancel_stops_updaters_quickly`.
2. **MULTI with an empty, all-unchecked or all-out-of-range watchlist, or a watchlist of 1000 entries.**
   - Expected: Play is refused with a message and no panic. With 50 checked paths and 1000 planes, a tick stays inside one second.
   - Tests: Task 5 `multi_play_without_targets_is_refused`, Task 4 `pipeline_50_paths_1000_planes_is_fast`.
3. **Options "Cancel" after edits in several tabs, including the QRV table and local obstructions.**
   - Expected: nothing is changed in the settings or the station/elevation databases.
   - Tests: Task 7 `cancel_discards_edits`, Task 8 `qrv_written_only_on_ok`.
4. **Credentials typed into feed settings or the Thunderforest key.**
   - Expected: masked in the editor, redacted in `Debug`, absent from status and error lines.
   - Tests: Task 1 `secret_debug_is_redacted`, Task 10 `secret_fields_are_masked`, Task 7 `tile_url_with_key_not_in_status`.
5. **First start versus an existing stage-3a settings file.**
   - Expected: the wizard appears only on a real first run. Aborting it exits without writing a settings file. A stage-3a file never triggers it.
   - Tests: Task 1 `stage3a_file_does_not_trigger_wizard`, Task 15 `abort_writes_no_settings`.

---

### Task 1: Settings for 3b

**Files:**
- Modify: `crates/as-settings/src/lib.rs`, `crates/as-settings/src/gui.rs`
- Create: `crates/as-settings/src/watchlist.rs`, `crates/as-settings/src/secret.rs`
- Test: `crates/as-settings/tests/settings_3b.rs`

**Interfaces:**
- Produces: `as_settings::{Secret, ElevationEnabled, WatchlistItem, Watchlist, Background, SpecLab, TimeSettings}`.
- **`Secret(String)`:**
  - Serde is transparent.
  - `Debug` prints `Secret(***)` when the value is non-empty and `Secret("")` when it is empty.
  - Methods: `expose(&self) -> &str`, `is_empty()`.
- **`ElevationEnabled`:**
  - Fields and defaults: `globe: true`, `srtm3: false`, `srtm1: false`, `aster3: false`, `aster1: false`.
  - `fn effective_model(&self) -> ElevationModel` ports `SetElevationModel` (MapDlg.cs:3752). Order ASTER1 > ASTER3 > SRTM1 > SRTM3 > GLOBE, else None.
  - `fn is_enabled(&self, m: ElevationModel) -> bool`.
- **`WatchlistItem`:**
  - Fields: `call: String`, `loc: String`, `checked: bool`, `selected: bool`, `out_of_range: bool`, `remove: bool`.
  - `Watchlist(Vec<WatchlistItem>)`.
  - Methods: `index_of(call)`, `index_of_loc(call, loc)`, `sort()` (Call, then Loc, as `CompareTo`).
  - `insert_capped(item, max) -> bool` does nothing when the call/loc already exists or the list has `max` entries.
- **`Background`:**
  - `update_on_startup: true`, `update_periodically: false`, `update_period_min: u64 = 60`, `calculations_enable: true`.
  - `stations_url: as_stations::DEFAULT_UPDATE_URL`, `aircraft_url: as_aircraft::updater::DEFAULT_UPDATE_URL`.
- **`SpecLab`:**
  - `enabled: false`, `url: "http://localhost:10080/"`, `file_name: "_fft_expt.json"`, `f1: 400`, `f2: 1600`, `update_s: u64 = 1`.
- **`TimeSettings`:**
  - `online: true`, `offline: DateTime<Utc> = 2015-02-25T00:00:00Z`.
- **New fields on `Settings`:**
  - `elevation_enabled: ElevationEnabled`, `watchlist: Watchlist`, `watchlist_max_count: usize = 1000`, `location_restrict_to_area: bool = true` (`Location_RestrictToAreaOfInterest`), `background: Background`, `speclab: SpecLab`, `time: TimeSettings`.
  - `feeds_disclaimer_accepted: Vec<String>`.
  - `first_run: bool`, with a **field-level** `#[serde(default)]`. A file without the field reads `false`, while `Settings::default()` sets `true`.
- **New fields on `GuiSettings`:**
  - `thunderforest_api_key: Secret` (empty).
  - `horizon_plot_polar: bool = true`.
- **`Settings::my_calls_push(&mut self, call)`:** MRU, front insert, dedup, max 10. This closes the 3a deferred minor.
- `Settings.elevation_model` stays for file compatibility but is no longer read by the app. Task 3 switches the app to `elevation_enabled.effective_model()`.

- [ ] **Step 1: Write the failing tests:**
  - `defaults_match_csharp`: the values above.
  - `effective_model_order`: all off → None; GLOBE+SRTM3 → SRTM3; all on → ASTER1.
  - `stage3a_file_does_not_trigger_wizard`: write `Settings::default()` minus the new keys with toml, load it, assert `first_run == false` and the watchlist is empty.
  - `fresh_default_is_first_run`.
  - `secret_debug_is_redacted`: `format!("{:?}", settings)` with key `"abc123"` contains no `abc123`.
  - `secret_roundtrips_as_plain_string`.
  - `watchlist_insert_capped_and_sort`.
  - `my_calls_mru_max_10`.
- [ ] **Step 2: Run** `cargo test -p as-settings --test settings_3b` — Expected: FAIL (types missing).
- [ ] **Step 3: Implement** the types and fields.
- [ ] **Step 4: Run** `cargo test -p as-settings` — Expected: PASS, including the old tests.
- [ ] **Step 5: Commit** `feat(as-settings): stage 3b settings (watchlist, background, speclab, time, secrets)`.

### Task 2: Core gaps — local obstruction storage, LocationGetAll

**Files:**
- Modify: `crates/as-elevation/src/store.rs`, `crates/as-elevation/src/obstruction.rs`
- Modify: `crates/as-stations/src/db.rs`
- Test: `crates/as-elevation/tests/obstruction_db.rs`, `crates/as-stations/tests/location_all.rs`

**Interfaces:**
- **Produces on `ElevationStore`:**
  - `local_obstruction_find(model, lat, lon) -> Result<Option<LocalObstruction>, ElevationError>`.
  - `local_obstruction_find_or_create_default(model, lat, lon, save: bool) -> Result<LocalObstruction, _>`.
  - `local_obstruction_insert_or_update_if_newer(model, &LocalObstruction, at: DateTime<Utc>) -> Result<bool, _>`.
  - `local_obstruction_delete(model, lat, lon) -> Result<bool, _>`.
- **Port source:** `ScoutBase/ScoutBase.Elevation/ElevationDatabase.cs:1772-2000` and `LocalObstructionDesignator.cs`.
  - Port the table name, key columns, blob layout and LastUpdated semantics 1:1.
  - The default is `LocalObstruction::new(lat, lon, None, None)`, which keeps finding #4.
- **Produces on `StationDb`:**
  - `location_get_all(rect: Option<GRect>) -> Result<Vec<Location>, StationError>` (`LocationGetAll`, plus the bbox overload).
  - `location_delete(call, loc) -> Result<bool, _>`.
  - `qrv_delete(call, loc, band) -> Result<bool, _>`.
- All SQL uses bound parameters.

- [ ] **Step 1: Write the failing tests:**
  - `obstruction_roundtrip_per_model`: what is stored in GLOBE is not found in SRTM3.
  - `obstruction_default_is_zero_and_unsaved_when_save_false`.
  - `obstruction_update_only_if_newer`.
  - `location_get_all_in_rect`: three locations, the rect contains two.
  - `location_and_qrv_delete`.
- [ ] **Step 2: Run** `cargo test -p as-elevation --test obstruction_db` and `cargo test -p as-stations --test location_all` — Expected: FAIL.
- [ ] **Step 3: Implement.** Add the table to the existing migrations so old DB files gain it.
- [ ] **Step 4: Run** both — Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): local obstruction storage, LocationGetAll, deletes`.

### Task 3: Path inputs — model flags, local obstruction, QRV gain

**Files:**
- Modify: `crates/airscout/src/services.rs`, `crates/airscout/src/app.rs`, `crates/airscout/src/ui/map.rs`
- Test: `crates/airscout/tests/services.rs`, `crates/airscout/tests/app.rs`

**Interfaces:**
- **`PathRequest` changes:**
  - Becomes `{ id, my: GPoint, my_call: String, targets: Vec<PathTarget>, settings: Settings }`.
  - `pub struct PathTarget { pub call: String, pub pos: GPoint }`.
  - SINGLE sends one target.
- **`ServiceEvent::PathReady` changes:**
  - Becomes `{ id, index: usize, call: String, path, terrain, my_gain_dbi: f64, dx_gain_dbi: f64 }`. `index` is the position in `targets`.
  - `PathFailed` gains `index`.
  - New `ServiceEvent::PathsDone { id }` after the last target.
- **Model and elevation lookup:**
  - The model is `settings.elevation_enabled.effective_model()`. With `None`, the result is `PathFailed("no elevation model enabled")`.
  - `ground_elevation` skips models whose flag is off. This is the C# `GetElevation` check of `Elevation_*_Enabled` (MapDlg.cs:3649), and resolves finding #87.
- **Local obstruction:**
  - Port `UpdatePaths` SINGLE (MapDlg.cs:4313-4596): `LocalObstructionFind(my)` per model.
  - `PathParams.local_obstruction = obstruction.obstruction(h1_ground + antenna, bearing12)` when one is stored, else `f64::MIN`.
  - Read the C# for the exact height argument and keep it 1:1.
- **Gains:** `qrv_find_or_create_default(call, loc, band).antenna_gain`; 0 means `default_antenna_gain(band)`, like the height.
- **AppState:**
  - `path`/`terrain` are replaced by `pub paths: Vec<PathEntry>`.
  - `pub struct PathEntry { pub call: String, pub path: PropagationPath, pub terrain: Vec<f64>, pub my_gain_dbi: f64, pub dx_gain_dbi: f64 }`. It is ordered by target index.
  - `pub fn single_path(&self) -> Option<&PathEntry>`.
- **Map:**
  - The beam wedge uses `beam_width_deg(entry.my_gain_dbi)` at My and `dx_gain_dbi` at DX.
  - Every entry's path is drawn.
  - The profile and Fit use `single_path()` (this also closes the 3a ruling on the default gain).

- [ ] **Step 1: Write the failing tests:**
  - `services.rs`:
    - `disabled_model_is_skipped_in_ground_elevation`: GLOBE off, SRTM3 on with data.
    - `stored_obstruction_enters_path_params`.
    - `qrv_gain_reported_in_path_ready`.
    - `multi_targets_report_index_and_done`.
  - `app.rs`: `effective_model_from_flags`.
- [ ] **Step 2: Run** `cargo test -p airscout` — Expected: FAIL.
- [ ] **Step 3: Implement.** Update all 3a call sites and tests to the new shapes.
- [ ] **Step 4: Run** `cargo test -p airscout` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): enabled elevation models, local obstruction and QRV gain in paths`.

### Task 4: Pipeline MULTI

**Files:**
- Modify: `crates/airscout/src/pipeline.rs`
- Test: `crates/airscout/tests/pipeline.rs`

**Interfaces:**
- `TickInput` gains `pub mode: as_aircraft::enums::PathMode`.
- `TickResult` gains `pub per_path_potential: Vec<i32>`, aligned with `paths`. It holds the highest potential among the planes of that path that pass the filter. This is what C# writes into the watchlist item tooltip (MapDlg.cs:5093-5229, step 8).
- Chart points are only filled when `mode == PathMode::Single`. This replaces `paths.len() == 1` and closes the 3a deferred minor.
- `nearest` and `highest_potential` stay last-path (finding #82).

- [ ] **Step 1: Write the failing tests:**
  - `multi_reports_potential_per_path`: two paths, one plane crossing only the second → `[0, 100]`.
  - `multi_has_no_chart_points`.
  - `single_mode_with_one_path_has_chart_points`.
  - `pipeline_50_paths_1000_planes_is_fast`: debug build, budget 1000 ms, same style as the 3a timing test.
- [ ] **Step 2: Run** `cargo test -p airscout --test pipeline` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): MULTI mode in the tick pipeline`.

### Task 5: MULTI in the app state

**Files:**
- Modify: `crates/airscout/src/app.rs`
- Test: `crates/airscout/tests/app.rs`

**Interfaces:**
- `ControlTab` gains `Multi`. `AppState::path_mode() -> PathMode` returns Multi when `control_tab == Multi` on Play. The tab is fixed while playing (C# :9922).
- **`play()` in SINGLE:**
  - Sends one target.
  - Inserts DX into the watchlist with `insert_capped(.., watchlist_max_count)`, marking `out_of_range` when the distance is greater than `path_max_length_km` (C# UpdatePaths).
  - Calls `my_calls_push(my_call)`.
- **`play()` in MULTI:**
  - Targets are the checked, not out-of-range items, with positions from `maidenhead::lat_lon_from_loc` of `item.loc`. Port `LocationFindOrCreate` via `StationDb` only if the C# reads a stored position; check MapDlg.cs:4313-4596.
  - With no targets, it returns `Err("no stations checked in the watchlist")` and sends nothing.
- `pub fn set_watch_checked(&mut self, idx: usize, checked: bool)`: while playing, it marks `watchlist_dirty`. The next `on_tick` re-sends the path request (`WatchListUpdated`) with a new `path_gen`.
- `on_tick` passes `mode` and the paths. After the tick it stores `per_path_potential` per watchlist call in `pub watch_potential: BTreeMap<String, i32>`.

- [ ] **Step 1: Write the failing tests:**
  - `multi_play_without_targets_is_refused`.
  - `multi_play_sends_checked_in_range_targets`.
  - `single_play_inserts_dx_into_watchlist_with_oor_flag`.
  - `checking_item_while_playing_resends_paths`.
  - `tab_change_blocked_while_playing`.
- [ ] **Step 2: Run** `cargo test -p airscout --test app` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): MULTI mode and watchlist in the app state`.

### Task 6: Multi tab and WatchlistDlg

**Files:**
- Create: `crates/airscout/src/ui/multi.rs`, `crates/airscout/src/ui/dialogs/mod.rs`, `crates/airscout/src/ui/dialogs/watchlist.rs`
- Modify: `crates/airscout/src/ui/control.rs`, `crates/airscout/src/ui/mod.rs`
- Test: `crates/airscout/tests/ui_multi.rs`

**Interfaces:**
- **Multi tab (MapDlg.cs:6059-6332, :5261-5287):**
  - Columns Call and Loc, each row with a checkbox.
  - Sorted with checked rows first, then by Call.
  - Row background from `watch_potential`: >75 magenta, >50 red, >0 orange.
  - Out-of-range rows have gray text and their checkbox is disabled.
  - Clicking a row sets DX (call + loc → position) when paused.
  - Clicking the header checks all rows that are not out of range.
  - The hover tooltip shows QRB/QTF from My.
  - A "Manage" button opens the dialog.
- `pub fn watch_row_color(potential: i32) -> Option<Color32>` is pure and tested.
- **WatchlistDlg (survey §3):**
  - Left list: "all callsigns" from `StationDb::location_get_all(area if location_restrict_to_area)`, with a prefix filter text box.
  - Right list: the current watchlist.
  - Buttons: Add, Remove, Remove all.
  - OK writes the right list back as `Watchlist`, sorted. Cancel drops it.
  - Station loading runs once per dialog open on a thread and the list shows "loading…" until it arrives.

- [ ] **Step 1: Write the failing tests:**
  - `watch_row_color_thresholds`: 0 → None, 1 → orange, 51 → red, 76 → magenta.
  - Kittest `oor_row_checkbox_disabled`.
  - Kittest `header_click_checks_all_in_range`.
  - Kittest `watchlist_dialog_add_and_ok` (fake station list injected).
- [ ] **Step 2: Run** `cargo test -p airscout --test ui_multi` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p airscout` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): Multi tab and watchlist dialog`.

### Task 7: Options dialog shell and the simple tabs

**Files:**
- Create: `crates/airscout/src/ui/options/mod.rs` and one file per tab group: `general.rs`, `database.rs`, `map.rs`, `elevation.rs`, `path.rs`, `planes.rs`, `alarm.rs`, `speclab.rs`, `watchlist.rs`, `info.rs`
- Modify: `crates/airscout/src/tiles.rs`, `crates/airscout/src/app.rs`, `crates/airscout/src/ui/control.rs`
- Test: `crates/airscout/tests/ui_options.rs`, `crates/airscout/tests/tiles.rs`

**Interfaces:**
- `pub struct OptionsDlg { draft: Settings, tab: OptionsTab, errors: Vec<String> }`.
- `pub fn show(&mut self, ctx) -> Option<OptionsOutcome>`, where `OptionsOutcome = Ok(Settings) | Cancel`.
- Opening is only possible while paused (the C# button is disabled while playing).
- **On Ok, `AppState::apply_options(settings)`:**
  - Saves the settings file.
  - Restarts the background services (Task 11).
  - Rebuilds the tile provider.
  - Sets the tick interval.
- **Tabs and fields:**
  - **General:** area Min/Max Lat/Lon. Error when min ≥ max or the values are out of range.
  - **Database:** update mode radio Never / On startup / Periodically, the period in minutes, and `calculations_enable`.
  - **Map:**
    - Provider combo, opacity, icon sizes L/M/H/S, beam width, label calls, `map_update_s`.
    - A Thunderforest key field, masked, with a "show" toggle.
  - **Elevation:** the five enable checkboxes plus the base URL and `elevation_download`. Warn when all five are off.
  - **Path:**
    - The band grid (`band_settings`), columns per survey §3: K-Factor, F1-Clearance, Ground Clearance, Max Distance, Max Squint, Max Elevation.
    - Also `path_best_case_elevation`, `path_max_length_km`, `path_near_field_suppression_m`.
  - **Planes:** min/max alt, TTL, max delta, filter min category, InfoWin checkboxes. Feed selection comes in Task 10.
  - **Alarm:** activate, distance, bring to front, play sound.
  - **SpecLab:** the `SpecLab` fields.
  - **Watchlist:** max count and "Clear watchlist".
  - **Info:** version, license (the workspace license text), attribution to DL2ALF.
- **`tiles.rs`:**
  - `providers(key: &Secret) -> Vec<TileProvider>`. The list adds "OCM Landscape" (Thunderforest, `https://tile.thunderforest.com/landscape/{z}/{x}/{y}.png?apikey={key}`, attribution per `MapProvider.cs`) only when the key is non-empty.
  - The key is substituted only inside the tile URL builder.

- [ ] **Step 1: Write the failing tests:**
  - `cancel_discards_edits`: kittest; change the area and alarm distance, press Cancel, settings unchanged.
  - `ok_applies_and_saves`: fake services record the restart.
  - `invalid_area_blocks_ok`.
  - `thunderforest_only_with_key`.
  - `tile_url_with_key_not_in_status`: provider error and status strings never contain the key.
  - `options_disabled_while_playing`.
- [ ] **Step 2: Run** `cargo test -p airscout --test ui_options --test tiles` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p airscout` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): options dialog`.

### Task 8: Stations tab and MapStationDlg

**Files:**
- Create: `crates/airscout/src/ui/options/stations.rs`, `crates/airscout/src/ui/dialogs/map_station.rs`
- Test: `crates/airscout/tests/ui_stations.rs`

**Interfaces:**
- **Stations tab (OptionsDlg.cs:525-1190):**
  - My and DX: call, lat, lon, locator, height.
  - The locator and lat/lon stay in sync, as in the C# Leave handlers.
  - The call is checked with `as_stations::callsign::check`.
- **QRV grid for the selected station:**
  - One row per band: antenna height, gain, power.
  - Loaded via `qrv_get_all`.
  - Written with `qrv_insert_or_update_if_newer` **only on Options OK**. The C# writes on Leave; that is GUI-layer and not reproduced (finding).
- Buttons open MapStationDlg, LocalObstructionsDlg and HorizonDlg (Task 9).
- **MapStationDlg:**
  - A walkers map with a draggable station marker and the locator polygon of the current 6-char locator.
  - Lat/lon/locator fields follow the marker.
  - OK returns the new position. Cancel leaves it unchanged.
  - The elevation grid overlay at zoom ≥ 17 is deferred to 3c (ledger).
- No QRZ lookup, no upload (Global Constraints).

- [ ] **Step 1: Write the failing tests:**
  - `locator_and_latlon_sync`.
  - `invalid_call_marked`.
  - `qrv_written_only_on_ok`: edit the gain, Cancel → DB unchanged; OK → DB updated.
  - `map_station_drag_updates_fields`: drive the pure `MapStationState::drag_to(GPoint)`.
- [ ] **Step 2: Run** `cargo test -p airscout --test ui_stations` — Expected: FAIL.
- [ ] **Step 3: Implement.** Station DB work for the grid runs on the Options open/OK path (small, bounded queries). It is not per frame.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): stations options and station map dialog`.

### Task 9: LocalObstructionsDlg and HorizonDlg

**Files:**
- Create: `crates/airscout/src/ui/dialogs/obstructions.rs`, `crates/airscout/src/ui/dialogs/horizon.rs`, `crates/airscout/src/horizon_service.rs`
- Test: `crates/airscout/tests/horizon.rs`

**Interfaces:**
- **LocalObstructionsDlg:**
  - A grid of 360 rows: bearing, distance [m], height [m].
  - "Clear all".
  - OK stores the obstruction with `local_obstruction_insert_or_update_if_newer` for the **effective** model, at the station position. Cancel stores nothing.
- **`HorizonService::start(paths, req: HorizonRequest, tx) -> HorizonHandle`:**
  - Runs `engine.find_or_create_horizon` on a thread with a cancel flag.
  - Progress goes out as `ServiceEvent::HorizonPoint { id, az, point }`, then `HorizonDone { id, horizon }` or `HorizonFailed { id, error }`.
- **`HorizonRequest { id, call, pos, height_m, settings }`:**
  - Calculation distance is 500 km.
  - Parameters as HorizonDlg (survey §3): GHz, R·K, F1, step width of the effective model, and the stored local obstruction.
- **HorizonDlg:**
  - egui_plot of Eps_min [°] and distance [km] over bearing.
  - A polar/cartesian toggle (`horizon_plot_polar`).
  - Drawn live from the progress points.
  - "Export CSV" writes `<data>/Tmp/horizon_<call>.csv` with columns `Bearing;Eps_Min;Distance;Elevation`. The separator follows C# `GetCSVSeparator`; check it and port it.
  - Closing the dialog cancels the computation.

- [ ] **Step 1: Write the failing tests:**
  - `horizon_service_reports_360_points_and_done`: tiny synthetic elevation store, as in the 3a services tests.
  - `horizon_cancel_on_drop`.
  - `horizon_csv_columns`.
  - `obstruction_ok_stores_cancel_does_not`.
- [ ] **Step 2: Run** `cargo test -p airscout --test horizon` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): local obstructions and horizon dialogs`.

### Task 10: Feed selection, feed settings and disclaimer

**Files:**
- Modify: `crates/as-feeds/src/plugin.rs` and the feeds that take credentials
- Create: `crates/airscout/src/ui/dialogs/feed_settings.rs`, `crates/airscout/src/ui/dialogs/disclaimer.rs`
- Modify: `crates/airscout/src/ui/options/planes.rs`
- Test: `crates/as-feeds/tests/secrets.rs`, `crates/airscout/tests/ui_feeds.rs`

**Interfaces:**
- `PlaneFeed::secret_keys(&self) -> &'static [&'static str]` defaults to `&[]`. Every feed whose `settings()` holds a credential (client id/secret, API key, password, user) lists those keys.
- **Planes tab:**
  - Feed combo from `as_feeds::registry::all_feeds()`, showing `name()` and `info()`.
  - Choosing a feed whose `disclaimer()` is `Some`, and which is not in `feeds_disclaimer_accepted`, opens the disclaimer dialog. Accept adds the name; Decline reverts the selection.
- **FeedSettingsDlg:**
  - A generic editor over the feed's `toml::Table` (string, integer, float, bool).
  - Keys in `secret_keys()` use a masked text field.
  - "Default" restores `feed_by_name(..).settings()`.
  - OK runs `load_settings` on a fresh feed. On `Err` the message is shown (it is credential-free by the as-feeds contract) and the table is not stored.
  - Import/Export are not ported (no file dialogs; ledger).

- [ ] **Step 1: Write the failing tests:**
  - `as-feeds`: `credential_feeds_declare_secret_keys` (for each feed: every key whose name contains `key`, `secret`, `password`, `token` or `user` is in `secret_keys()`).
  - `airscout`:
    - `secret_fields_are_masked`.
    - `disclaimer_decline_reverts_selection`.
    - `disclaimer_accept_is_remembered`.
    - `invalid_feed_settings_not_stored`.
- [ ] **Step 2: Run** `cargo test -p as-feeds --test secrets` and `cargo test -p airscout --test ui_feeds` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat: feed selection, settings editor and disclaimer`.

### Task 11: Background updaters and status LEDs

**Files:**
- Create: `crates/airscout/src/background.rs`
- Modify: `crates/airscout/src/app.rs`, `crates/airscout/src/ui/status.rs`, `crates/airscout/src/services.rs` (`ServiceEvent`)
- Test: `crates/airscout/tests/background.rs`

**Interfaces:**
- **LED types:**
  - `pub enum Led { Aircraft, Stations, Elevation(ElevationModel) }`.
  - `pub enum LedState { Empty, Updating, Complete, UpToDate, Error, Unknown }`.
  - `pub fn led_color(s: LedState) -> Color32` ports `DatabaseStatus.cs`: ERROR red, UPTODATE green, COMPLETE blue, UPDATING gold, EMPTY black, otherwise plum.
- `ServiceEvent::Led { led: Led, state: LedState, text: String }`. `text` is shown as the LED tooltip and is credential-free.
- **`BackgroundService::start(paths: Paths, settings: &Settings, tx, cancel: Arc<AtomicBool>) -> BackgroundService`** starts one thread per job:
  - The stations updater: `update_stations` with the area when `location_restrict_to_area`.
  - The aircraft updater: `update_aircraft_db`.
  - The position maintainer: `as_aircraft::positions::maintain`.
  - One elevation updater per **enabled** model: `prepare_area(store, client, base, model, area_rect, sea_file, true, ..)`.
- Each job runs once when `update_on_startup` is set, then every `update_period_min` while `update_periodically` is set. Sleeps check the cancel flag at least every 200 ms.
- `stop(self)` sets cancel and detaches.
- `ServiceEvent::BackgroundIdle(Led)` is sent after each run. The PathCalculator (Task 12) waits for it.
- The status bar shows the LEDs in the order Aircraft, Stations, GLOBE, SRTM3, SRTM1, ASTER3, ASTER1.
- The app starts the service after the settings load (and after the wizard, Task 15), and restarts it on Options OK.

- [ ] **Step 1: Write the failing tests:**
  - All with mockito and a temp data dir:
    - `startup_run_sets_uptodate_led`.
    - `updater_error_sets_error_led`: HTTP 500.
    - `disabled_model_not_updated`: no request for SRTM3 when it is off.
    - `cancel_stops_updaters_quickly`: cancel during a slow mock, the thread ends within 1 s.
  - Plus `led_color_mapping`.
- [ ] **Step 2: Run** `cargo test -p airscout --test background` — Expected: FAIL.
- [ ] **Step 3: Implement.** HTTP clients use `as_net::user_agent()` and a 300 s timeout. Download errors are logged without URLs that carry query credentials (none here, but keep the rule).
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): background database updaters with status LEDs`.

### Task 12: PathCalculator

**Files:**
- Create: `crates/airscout/src/calculator.rs`
- Modify: `crates/airscout/src/services.rs` (busy flag), `crates/airscout/src/app.rs`, `crates/airscout/src/ui/status.rs`
- Test: `crates/airscout/tests/calculator.rs`

**Interfaces:**
- Port `PathCalculator.cs` (MapDlg.cs:34-304) per enabled model, only when `background.calculations_enable`.
  - It starts after `BackgroundIdle` of that model **and** of Stations.
  - It runs when the station table timestamp differs from the last stored run.
  - It reads `location_get_all(area)`.
  - For each location within `path_max_length_km` of My, and for each band in `band_settings`, it calls `engine.find_path`; if there is no path, it calls `create_path(save=true)`.
  - Station heights come from the QRV antenna height (0 → band default) plus ground elevation.
  - Ground elevation uses the best-case elevation when `path_best_case_elevation` is set. Port `Path_BestCaseElevation` (MapDlg.cs:3544-3642) exactly and add finding #88+ if it differs from the UI path.
  - It sleeps 100 ms per location.
- **Busy throttle:** `PathService` sets `Arc<AtomicBool> ui_busy` while it computes. The calculator waits 1000 ms per path while it is set, which ports `Background_Calculations_ThreadWait = 1000`.
- The C# bug where ASTER3/ASTER1 have no name or step width is **not** reproduced (finding: Not reproduced (GUI)).
- Progress: `ServiceEvent::Calculations(String)`, shown in the status bar as in `tsl_Calculations`.
- The last run's station timestamp is stored in `<data>/calculator-<model>.stamp`.

- [ ] **Step 1: Write the failing tests:**
  - `calculator_fills_missing_paths_for_all_bands`: two stations, three bands.
  - `calculator_skips_stations_beyond_max_length`.
  - `calculator_skips_when_stamp_unchanged`.
  - `calculator_waits_while_ui_busy`.
  - `calculator_cancel`.
- [ ] **Step 2: Run** `cargo test -p airscout --test calculator` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): background path calculator`.

### Task 13: Spectrum Lab spectrum and nearest-plane box

**Files:**
- Modify: `crates/as-signals/src/lib.rs`, `crates/as-signals/tests/signals.rs`
- Create: `crates/airscout/src/speclab.rs`, `crates/airscout/src/ui/spectrum.rs`
- Modify: `crates/airscout/src/app.rs`, `crates/airscout/src/ui/mod.rs`
- Test: `crates/airscout/tests/speclab.rs`

**Interfaces:**
- **`as_signals::speclab_url`:**
  - Joins `f2` with `&`. The `?` bug is GUI-layer and is not reproduced.
  - Update the existing test `speclab_url_reproduces_bug`, renamed `speclab_url_joins_with_ampersand`.
  - Update the finding to "Not reproduced (GUI)".
  - `parse_speclab_fft` keeps its 1:1 behaviour, since it decides the stored level.
- **`SpecLabService::start(settings: &SpecLab, signals: Arc<SignalDb>, record: Arc<AtomicBool>, tx, cancel)`:**
  - Polls every `max(update_s, 1)` s, only while playing in LIFE mode (the app starts and stops it with Play/Pause when `speclab.enabled`).
  - Sends `ServiceEvent::Spectrum { at, level }` or `SpectrumError(String)`.
  - While `record` is set it buffers samples and calls `bulk_insert_or_update_if_newer` every 60 samples (MapDlg.cs:7630-7833).
- **App:**
  - Sets `record` to `tick.nearest.is_some()`.
  - Keeps the last 600 levels (`SpectrumMaxPoints`).
- **Spectrum bottom tab:**
  - egui_plot line, goldenrod, width 3. X 0–600. Y −120..0 with autoscale.
  - A magenta background band over the samples taken while recording.
  - A status line.
  - The nearest box shows Call, Type, Cat, Alt, Angle, Dist of `tick.nearest`.
  - `gm_Nearest` is not ported (ledger).

- [ ] **Step 1: Write the failing tests:**
  - `speclab_url_joins_with_ampersand`.
  - `airscout` with mockito:
    - `poller_sends_levels`.
    - `records_every_60_samples_only_while_flag_set`.
    - `poller_error_reported_not_fatal`.
    - `ring_keeps_600`.
- [ ] **Step 2: Run** `cargo test -p as-signals` and `cargo test -p airscout --test speclab` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): Spectrum Lab spectrum and nearest plane box`.

### Task 14: Offline time and alarm sound

**Files:**
- Create: `crates/airscout/src/ui/dialogs/set_time.rs`, `crates/airscout/src/sound.rs`
- Modify: `crates/airscout/Cargo.toml` (feature `sound = ["dep:rodio"]`, on by default), `Cargo.toml` (workspace dep), `crates/airscout/src/app.rs`, `crates/airscout/src/ui/status.rs`
- Modify: `.github/scripts/ci-prepare.sh` (install `libasound2-dev` and `pkg-config` via `sudo -n apt-get` when missing)
- Test: `crates/airscout/tests/app.rs`

**Interfaces:**
- **SetTimeDlg (MapDlg.cs:3529):**
  - Opened by double-clicking the UTC field, only while paused.
  - Edits `time.online` and `time.offline` (date + time, UTC).
  - The UTC field is LightSalmon when online and Plum when offline (MapDlg.cs:3400).
  - Port the tick 1:1: in LIFE + Playing, planes update only when `time.online` (MapDlg.cs:5353-5372). Offline LIFE shows no plane updates, as in C#.
- **Sound:**
  - `pub trait Beeper { fn beep(&mut self); }`.
  - `RodioBeeper` plays a generated 880 Hz, 300 ms sine on the default output. Do not add decoder features. Output errors are ignored after one status message.
  - `NullBeeper` is used when the feature is off or no device exists.
- The alarm beeps once per newly alarming hex when `alarm_play_sound` is set. It does not beep every tick; port the C# condition (MapDlg.cs DrawPlanes alarm block).

- [ ] **Step 1: Write the failing tests:**
  - `offline_time_skips_plane_updates`.
  - `set_time_only_while_paused`.
  - `alarm_beeps_once_per_new_alarm` (fake `Beeper`).
  - `no_beep_when_sound_disabled`.
- [ ] **Step 2: Run** `cargo test -p airscout --test app` — Expected: FAIL.
- [ ] **Step 3: Implement.** Pin the latest rodio release with only the playback feature. Check that `cargo build --target x86_64-pc-windows-gnu -p airscout` still links locally in docker, or rely on CI.
- [ ] **Step 4: Run** the full gate — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): offline time and alarm sound`.

### Task 15: FirstRunWizard

**Files:**
- Create: `crates/airscout/src/ui/dialogs/wizard.rs`, `crates/airscout/assets/TERMSANDCONDITIONS` (verbatim copy of `target/upstream/AirScout/AirScout/TERMSANDCONDITIONS`)
- Modify: `crates/airscout/src/app.rs`, `crates/airscout/src/main.rs`
- Test: `crates/airscout/tests/wizard.rs`

**Interfaces:**
- The wizard shows when the settings file did not exist or `first_run == true`. Before it finishes, no services start: no feed, no background, no downloads.
- **Pages (survey §3 FirstRunWizard):**
  1. **Terms:** the text via `include_str!`. "Accept" is needed to continue.
  2. **Coverage:** area Min/Max Lat/Lon, with the same validation as Options General.
  3. **Elevation models:** the five checkboxes. Note that GLOBE is small and SRTM/ASTER are large downloads. At least one is required.
  4. **User:** call (`callsign::check`) and locator → lat/lon, or lat/lon → locator. No QRZ lookup.
  5. **Plane feed:** the combo plus the disclaimer flow from Task 10.
  6. **Finish:** sets `first_run = false`, saves, starts the services.
- Abort (window close or "Cancel") exits the app **without writing a settings file**.
- The wizard state is a pure `WizardState { page, draft: Settings }` with `next() -> Result<(), String>` and `back()`, testable without a display.

- [ ] **Step 1: Write the failing tests:**
  - `terms_must_be_accepted`.
  - `invalid_call_blocks_next`.
  - `no_model_blocks_next`.
  - `finish_clears_first_run_and_saves`.
  - `abort_writes_no_settings`.
  - `no_services_before_finish` (fake services).
- [ ] **Step 2: Run** `cargo test -p airscout --test wizard` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): first-run wizard`.

### Task 16: Docs, findings, CI check

**Files:**
- Modify: `README.md` (status rows: 3b done; the desktop app section covers MULTI, Options, LEDs, wizard, sound; the Linux build needs `libasound2-dev` or `--no-default-features`), `docs/logic-findings.md`

**Steps:**
- [ ] **Step 1:** Add findings from #88 for every port deviation and every "Not reproduced (GUI)" item of this stage, including those already named:
  - SpecLab `?f2`.
  - QRV saved on Leave.
  - PathCalculator ASTER name.
  - MapPreloader moved, citing the OSM tile policy.
  - Resolve #87.
- [ ] **Step 2:** Run the full gate locally on macOS and the Linux docker gate (`CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`; the image needs `apt-get install -y libasound2-dev pkg-config` first) — Expected: green.
- [ ] **Step 3: Commit** `docs: stage 3b README and logic findings`.
