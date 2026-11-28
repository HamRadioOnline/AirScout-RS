# GUI 1:1 — Phase 2 (MapDlg, the main window) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Rust main window has the control tree, texts and arrangement of the upstream
`MapDlg`. Every MapDlg control works, including the 9 rows `docs/gui-reference/gaps.md` lists
as missing. The whole window passes `assert_window("MapDlg", …)`.

**Architecture:** `ui::show` is rebuilt on `ui::forms`:
- `$this` holds a Rust-only menu bar (D-002) on top, `ss_Main` at the bottom and `sc_Main`
  filling the rest.
- `sc_Main.Panel1` holds `sc_Map`: `tc_Map` (Map, Latest News) over `tc_Main` (Path Profile,
  Spectrum, Analysis, then the Rust-only Planes tab).
- `sc_Main.Panel2` holds the control column: `pa_CommonInfo`, `tc_Control`, `gb_Map_Zoom`,
  `gb_Map_Buttons`.

Controls sit at their Designer `Location`/`Size` inside their container (`designer_rect`).
They follow their `Anchor` on resize (`anchored`) and grow, never shrink, to fit egui text.
Missing logic is ported 1:1 from `MapDlg.cs` at the pinned commit, test-first. It goes into
`AppState` or a service; `ui/*` only draws. The existing renderers (map, profile, spectrum
plot, analysis, planes) move into their panes unchanged.

**Tech Stack:** Rust, egui/eframe 0.37, walkers, egui_kittest 0.37 (no new features).

**Spec:** `docs/superpowers/specs/2026-11-26-gui-1to1-design.md`. Phase-1 groundwork:
`docs/superpowers/plans/2026-11-26-gui-1to1-phase1.md`. Window themes:
`docs/superpowers/plans/2026-11-26-gui-1to1-windows-outline.md` (Phase 2).

## Global Constraints

- **Dependencies:**
  - No new crates, no new dependency edges, no new features: `Cargo.lock` unchanged; never `cargo update`.
  - In particular `image` is not a dependency of `airscout`, and adding it would change `Cargo.lock`.
  - `tests/golden/*.json`, `upstream_commit` and `docs/gui-reference/*.json` stay unchanged.
  - The reference JSONs are generator output only.
- **Upstream source:**
  - Pinned at `91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`, fetched into `target/upstream/AirScout`.
  - C# line numbers below refer to `AirScout/MapDlg.cs` and `AirScout/MapDlg.Designer.cs` there.
- **Layout and text:**
  - Designer `Font` is ignored (D-004).
  - Controls take their Designer rect and may grow to fit their text, never shrink.
  - Windows are resizable; containers scroll below the Designer size (D-005).
  - Target: fully usable at 1366×768.
  - Texts are the Designer texts, with mnemonics stripped. Runtime texts follow the C# code that sets them.
  - Visible strings are English.
- **Rust-only extras:**
  - Only after the original controls of their container.
  - Each one is listed in `docs/gui-reference/MapDlg.allow.json` with a `## D-NNN` id from `docs/gui-deviations.md`.
  - New deviation entries D-006..D-013 are approved: `Approved: 2026-11-27`.
- **Data:** synthetic test values only (TE1ST, TE2ST, JO50AA, JO60AA, 3C0001, localhost). No personal data, no LAN addresses.
- **Network:**
  - Honest User-Agent via `as_net::user_agent()`.
  - No credentials in logs, Debug output, status or errors.
- **Existing behaviour:**
  - Prediction, feed, database and network behaviour merged in stage 6 is unchanged.
  - Existing kittest tests that find widgets by their old Rust labels are updated to the original texts, in the task that renames them.
  - Their assertions stay the same; no test is deleted or weakened.
- **Cargo:**
  - Prefix `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` and use `--locked`.
  - Gates per task: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`.
  - Commits signed (`git commit -S`).

## Review Focus

1. **Small windows:** at 1366×768 and at 900×600 every control of the control column stays reachable (scroll, no clipping, no overlap) — test in Task 3.
2. **Play mode:** tab changes of `tc_Map`, `tc_Main` and `tc_Control` are refused while playing (`*_Selecting`), and the station fields are read-only — tests in Tasks 1 and 3.
3. **Unknown or invalid calls:** an invalid call or a DB miss in `cb_MyCall`/`cb_DXCall` clears the locator and never keeps the old position — test in Task 4.
4. **Network down:** the news check runs in the background and never blocks a frame; errors land in the status (`Say`), never as a dialog — test in Task 7.
5. **Screenshot names:** a call with `/` (e.g. `TE1ST/P`) never creates a sub-directory, and a missing Tmp directory is created — test in Task 2.

---

### Task 1: Window skeleton, status strip, subtree fidelity helper

**Files:**
- Modify: `crates/airscout/src/ui/mod.rs`, `crates/airscout/src/ui/status.rs`, `crates/airscout/src/main.rs`, `crates/airscout/src/app/mod.rs` (tab state)
- Modify: `crates/airscout/tests/gui_ref/fidelity.rs` (add `assert_subtree`)
- Create: `crates/airscout/tests/gui_mapdlg.rs`, `docs/gui-reference/MapDlg.allow.json`
- Modify: `docs/gui-deviations.md`, existing tests that click the old bottom tabs

**Interfaces:**
- Produces (test side, `tests/gui_ref/fidelity.rs`):
  - `pub fn assert_subtree(window: &str, root: &str, depth: Option<usize>, trace: &[Entry])`.
  - It compares only `root` and its descendants up to `depth` parent hops. The hop `child → X.PanelN → X` counts as one.
  - Trace entries are kept if `name == root` or their path contains `root`.
  - `root`'s own dock/position against its parent is not checked.
  - It uses the window's allowlist and deviations like `assert_window`.
- Produces (test side, `tests/gui_mapdlg.rs`):
  - `fn trace_mapdlg(setup: impl FnOnce(&mut AppState)) -> Vec<Entry>`.
  - It runs `ui::show` in a `Harness` of `MAPDLG_TEST_SIZE = (1008.0, 754.0)` with tracing on and returns the trace of the second frame.
  - It reuses the test-`Services` pattern of `tests/ui_analysis.rs`.
- Produces (app):
  - `pub enum MapTab { Map, News }` and `pub map_tab: MapTab` on `AppState`.
  - `BottomTab` order becomes `Profile, Spectrum, Analysis, Planes`.
  - `pub fn set_map_tab(&mut self, t: MapTab)` and `pub fn set_bottom_tab(&mut self, t: BottomTab)` both refuse while playing, like `set_control_tab`.
  - `pub fn lower_panel_collapsed(&self) -> bool`: true in MULTI (`control_tab == Multi`) or on the News tab (`tp_Map_Enter`/`tp_News_Enter`, MapDlg.cs:6955-6967).
- Produces (ui):
  - `pub const MENU_HEIGHT: f32 = 24.0`.
  - Default viewport inner size `[1008.0, 730.0 + MENU_HEIGHT]` when no window size is saved.

- [ ] **Step 1: Write the failing tests**
  - In `tests/gui_fidelity.rs`: `assert_subtree_ignores_entries_outside_root`, `assert_subtree_detects_missing_descendant`, `assert_subtree_respects_depth`.
  - In `tests/gui_mapdlg.rs`:
    - `skeleton_matches_reference`: `assert_subtree("MapDlg", "sc_Main", Some(0), &t)`, `("sc_Map", Some(2))` and `("ss_Main", None)`.
    - `default_split_matches_designer`: at the test size, `sc_Main.Panel2` is 152±1 pt wide (1008 − 852 − 4) and `sc_Map.Panel1` is 342±1 pt high.
    - `tab_change_refused_while_playing`: `set_map_tab`/`set_bottom_tab`/`set_control_tab` keep the old tab.
    - `lower_panel_collapsed_in_multi_and_news`: covers both triggers.
- [ ] **Step 2: Run** `cargo test -p airscout --locked --test gui_mapdlg --test gui_fidelity` → FAIL (missing items).
- [ ] **Step 3: Implement**
  - **Window frame:** `trace::record_window` for the client rect. The Rust-only menu bar has File → "Download data for the area…" (D-002, unchanged behaviour).
  - **`ss_Main`:**
    - Draw its items in Designer order: `tsl_Status` (spring: takes the free width), `tsl_Dummy`, `tsl_Calculations`, `tsl_Database`.
    - Then the 8 LEDs `tsl_Database_LED_*`: a drawn LED whose hover text is the Designer `Text`; record that `Text`. Exception: SRTM1 says "SRTM1 database status LED" (D-011).
    - Then `tsl_Track`, `tsl_Rot`, `tsl_CAT`.
    - The Rust-only fields (settings error, plane counts, feed status) go after `tsl_CAT` (D-010).
    - The alarm text keeps its place in `tsl_Status`.
  - **`sc_Main`:**
    - `split_container`, Vertical.
    - Panel2 width is persisted in `settings.gui.side_panel_width`, default 152 and never below 152.
  - **`sc_Map`:**
    - Horizontal; Panel2 height in `bottom_panel_height`, default 360.
    - Panel2 is skipped when `lower_panel_collapsed()`.
  - **Tab controls:**
    - `tc_Map` pages are `tp_Map` "Map" (draws `MapView`) and `tp_News` "Latest News" (empty body until Task 7).
    - `tc_Main` pages are "Path Profile", "Spectrum", "Analysis" and then "Planes" (D-006); bodies call the existing renderers.
  - **Allowlist:** `MapDlg.allow.json` gets entries for `gm_Cache` (missing, D-003) and the D-006/D-010/D-011 extras.
  - **Deviations:** add D-006, D-010, D-011 (texts below).
  - Update the kittest tests that click the old bottom tabs.
- [ ] **Step 4: Run** the tests of Step 2 and `cargo test --workspace --locked` → PASS.
- [ ] **Step 5: Commit** `feat(gui): MapDlg window skeleton and status strip as in the original`

### Task 2: Map screenshot save (`btn_Map_Save` logic)

**Files:**
- Create: `crates/airscout/src/map_save.rs`
- Modify: `crates/airscout/src/lib.rs`, `crates/airscout/src/app/mod.rs`, `crates/airscout/src/app/shell.rs` (screenshot event)
- Test: `crates/airscout/tests/map_save.rs`

**Interfaces:**
- Produces:
  - `pub fn map_save_file_name(band: Band, my_call: &str, dx_call: &str, t: DateTime<Utc>) -> String`.
    - Format: `"{BAND}_{MY}_{DX}_{yyyyMMdd}_{HHmmss}.png"`.
    - `{BAND}` is the C# enum member name (`BAND.ToString()`, e.g. `B1_2G`).
    - `/` in calls becomes `_` (MapDlg.cs:3506-3513, D-009 for `.png`).
  - `pub fn encode_png(image: &egui::ColorImage) -> Vec<u8>`: RGBA8 PNG with uncompressed deflate blocks, hand-written CRC-32 and Adler-32 (no new dependency).
  - `AppState::request_map_save(&mut self)`: sets `pub map_save_pending: bool`.
  - `AppState::save_map_image(&mut self, image: &egui::ColorImage, now: DateTime<Utc>) -> Result<PathBuf, String>`.
    - It writes into `<data_dir>/Tmp` (created if missing), using `current_time(now)` like C# `CurrentTime`.
    - On error it writes `status`; the C# logs and swallows errors, so the Rust shows the error in the status instead.
  - The shell sends `ViewportCommand::Screenshot` when `map_save_pending` is set. On `egui::Event::Screenshot` it calls `save_map_image` and clears the flag.

- [ ] **Step 1: Write the failing tests**
  - `file_name_replaces_slashes`: TE1ST/P, TE2ST, B1_2G, 2026-11-27 12:34:56 → `B1_2G_TE1ST_P_TE2ST_20261127_123456.png`.
  - `png_roundtrip_header_and_crc`: signature, IHDR fields, IEND CRC = `AE 42 60 82`, IDAT zlib header `78 01`; an Adler-32 known vector; a 2×1 image decodes by hand-parsing the stored block.
  - `save_creates_tmp_dir_and_file`: temp dir.
  - `save_error_goes_to_status`: Tmp path is a file.
- [ ] **Step 2: Run** `cargo test -p airscout --locked --test map_save` → FAIL.
- [ ] **Step 3: Implement** as in Interfaces. Add D-009 to `docs/gui-deviations.md`.
- [ ] **Step 4: Run** → PASS; `cargo test --workspace --locked` → PASS.
- [ ] **Step 5: Commit** `feat(gui): save a map screenshot to the Tmp directory (btn_Map_Save)`

### Task 3: Control column (`sc_Main.Panel2`) 1:1

**Files:**
- Modify: `crates/airscout/src/ui/control.rs`, `crates/airscout/src/ui/multi.rs`, `crates/airscout/src/ui/map.rs` (zoom readout), `crates/airscout/src/app/mod.rs`
- Modify: `crates/as-settings/src/gui.rs` only if a field below is missing
- Test: `crates/airscout/tests/gui_mapdlg.rs`, existing `ui_multi.rs` and other tests using old labels

**Interfaces:**
- Consumes: Task 1 `trace_mapdlg`, `assert_subtree`; Task 2 `request_map_save`.
- Produces:
  - `pub map_zoom: f64` on `AppState`, written by `MapView::show` every frame (`tb_Zoom.Text = gm_Main.Zoom`, MapDlg.cs:5420).
  - `pub fn toggle_box(&mut self, b: ControlBox)` with `pub enum ControlBox { Info, Zoom, Filter, Alarm }`. It flips `settings.gui.show_info_box`/`show_zoom_box`/`show_filter_box`/`show_alarm_box`: a click on the group title minimizes or restores the box (MapDlg.cs:1760-1910, 5770-5781; port the exact minimize rule).

- [ ] **Step 1: Write the failing tests** in `gui_mapdlg.rs`:
  - `control_column_matches_reference`: `assert_subtree` for `pa_CommonInfo`, `tc_Control` (one trace per tab Single/Multi/Opt), `gb_Map_Zoom`, `gb_Map_Buttons`, all `None`.
  - `zoom_readout_shows_map_zoom`: `tb_Zoom` shows `map_zoom` as an integer.
  - `group_title_click_toggles_box`.
  - `control_column_reachable_at_small_sizes`: at 1366×768 and 900×600 every `tc_Control`/`gb_Map_*` button rect lies inside the window or is reachable by scrolling — no zero-size and no overlapping control rects.
  - `station_fields_read_only_while_playing`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**
  - **`pa_CommonInfo` (Top):** `label1` "UTC", `tb_UTC` (LightSalmon/Plum, double click opens SetTime as today), `label8` "Band", `cb_Band` (refused while playing).
  - **`gb_Map_Buttons` (Bottom):**
    - `btn_Options` "Options" (paused only).
    - `btn_Map_Save` "Save" → `request_map_save`.
    - `btn_Map_PlayPause`: text "Play"/"Pause"; the C# draws an image here (D-007).
  - **`gb_Map_Zoom` (Bottom):** "+", `tb_Zoom` (read-only), "-", `cb_AutoCenter` "Auto Center".
  - **`tc_Control` (Fill):**
    - Single: `gb_Map_Info` (Top), with labels "My Call", "My Loc", "DX Call", "DX Loc", "QRB", "QTF" and editable text fields at the combo rects (dropdowns come in Task 4). `tb_QRB`/`tb_QTF` hold the values as `UpdateStatus` formats them (MapDlg.cs:3400-3460).
    - Multi: `lv_Control_Watchlist` (Fill) with column headers "Call" and "Loc", then `btn_Control_Manage_Watchlist` "Manage Watchlist" (Bottom).
    - Opt: `gb_Map_Filter` "Planes Filter" with `pa_Planes_Filter` ("Max. Circ.:", "km", "Min. Alt.:", "m", "Min. Cat:"), then `gb_Map_Alarms` "Alarms" with `cb_Alarms_Activate` "Activate Alarms" (Plum background while an alarm is shown).
  - Keep all existing behaviour; only the arrangement and texts change.
  - Add D-007 to `docs/gui-deviations.md`.
  - Update `MapDlg.allow.json` and the tests that used the old labels ("Zoom +", "Auto-center", "Alarm on", "My call", …).
- [ ] **Step 4: Run** → PASS; workspace tests → PASS.
- [ ] **Step 5: Commit** `feat(gui): MapDlg control column as in the original`

### Task 4: Station call and locator boxes (`cb_MyCall`, `cb_MyLoc`, `cb_DXCall`, `cb_DXLoc`)

**Files:**
- Create: `crates/airscout/src/app/station_fields.rs`
- Modify: `crates/airscout/src/app/services_trait.rs`, `crates/airscout/src/services.rs`, `crates/airscout/src/ui/control.rs`
- Test: `crates/airscout/tests/station_fields.rs`

**Interfaces:**
- Produces (`Services`, default impls return nothing):
  - `fn location_find(&self, call: &str) -> Option<GPoint>`: the latest stored location (`StationData.Database.LocationFind`).
  - `fn location_find_all(&self, call: &str) -> Vec<GPoint>`: all stored locations, most recent first.
  - The real services implement them with the station DB used by `web_stations::location_find`/`location_find_all`.
- Produces (`AppState`):
  - `pub fn my_call_changed(&mut self, text: &str)` and `dx_call_changed`: the `cb_*Call_TextChanged` port (MapDlg.cs:5785-5808, 5909-5932).
    - Set the call and clear the locator text.
    - If `Callsign.Check` passes and the DB has the call, take its lat/lon; else the position is invalid.
    - Then `UpdateStatus`.
  - `pub fn my_call_selected(&mut self, call: &str)` and `dx_call_selected`: the `*_SelectedIndexChanged` port (5810-5835, 5934-5959).
    - On a DB hit: the locator text comes from `LocFromLatLon` with the `Locator_*` settings, and the height becomes 10.
    - Else: invalid position and height 0.
  - `pub fn my_call_items(&self) -> Vec<String>`: `settings.gui.my_calls` in order (`cb_MyCall_DropDown`).
  - `pub fn dx_call_items(&self) -> Vec<String>`: the watchlist calls, sorted (`cb_DXCall_DropDown`).
  - `pub fn my_loc_items(&self) -> Vec<(String, GPoint)>` and `dx_loc_items`: `LocationFindAll` of the current call, formatted with the `Locator_*` settings.
  - `pub fn my_loc_selected(&mut self, p: GPoint)` and `dx_loc_selected`: set the position and the locator text (`*_SelectedIndexChanged`, 5890-5902, 6024-6036).
  - `set_my_loc_text`/`set_dx_loc_text` keep today's behaviour and add the C# rule: a valid locator of length ≥ 6 sets the position and height 10 (5848-5870, 5982-6004).
  - Elevation: the C# stores `GetElevation` in `MyElevation`/`DXElevation`. Rust settings have no such field because the path request computes elevation. No new field.

- [ ] **Step 1: Write the failing tests** with a fake `Services` holding TE1ST at JO50AA and TE2ST at JO60AA and JO61AA:
  - `call_text_hit_sets_position`.
  - `call_text_miss_clears_position_and_locator`.
  - `invalid_call_never_looks_up`.
  - `call_selected_sets_locator_and_height_10`.
  - `dx_items_are_sorted_watchlist_calls`.
  - `loc_items_list_all_locations_most_recent_first`.
  - `loc_selected_sets_position`.
  - `short_locator_keeps_position`.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**
  - Logic as in Interfaces.
  - UI: each `cb_*` is an editable text field with a dropdown button. The dropdown lists the `*_items`; picking one calls `*_selected`.
  - Fill colours keep today's `loc_fill` (PaleGreen when precise, PaleRed when invalid).
  - Update the `MapDlg` rows in `docs/gui-reference/gaps.md` to `ok`.
- [ ] **Step 4: Run** → PASS; workspace tests → PASS.
- [ ] **Step 5: Commit** `feat(gui): call and locator boxes look stations up as in the original`

### Task 5: Lower tabs (`tc_Main`) 1:1

**Files:**
- Modify: `crates/airscout/src/ui/profile.rs`, `crates/airscout/src/ui/spectrum.rs`, `crates/airscout/src/ui/planes.rs`, `crates/airscout/src/ui/analysis.rs`
- Test: `crates/airscout/tests/gui_mapdlg.rs`, `crates/airscout/tests/ui_analysis.rs`, other tests using old labels

**Interfaces:**
- Consumes: Task 1 tabs and helpers.
- Produces: `pub fn nearest_texts(p: &PlaneInfo) -> [String; 6]` for Call, Type, Category, Altitude, Angle and Distance, in the C# formats (MapDlg.cs:7810-7817):
  - `Alt_m` as `F0` + `"m"`.
  - `Angle` in degrees as `F0` + `"°"`.
  - `IntQRB` as `F0` + `"km"`.
  - Category via `PlaneCategories.GetStringValue`.

- [ ] **Step 1: Write the failing tests**
  - `lower_tabs_match_reference`: `assert_subtree` for `tp_Elevation`, `gb_Spectrum_Status`, `gb_Spectrum`, `gb_Spectrum_NearestInfo` and `tp_Analysis`, all `None`.
  - `nearest_texts_formats`: synthetic plane, 10668.4 m, 0.1 rad, 123.6 km → "10668m", "6°", "124km".
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**
  - **Path Profile:** the profile renderer fills the page.
  - **Spectrum:**
    - `gb_Spectrum_Status` (Top-Left-Right anchors) holds `tb_Spectrum_Status`, with the status and the error colours of today.
    - `gb_Spectrum` holds the plot.
    - `gb_Spectrum_NearestInfo` holds label pairs at the Designer rects ("Call:", "Type:", "Category:", "Altitude:", "Angle:", "Distance:") with `nearest_texts`; "0" while there is no nearest plane, as in the Designer.
  - **Analysis:**
    - `panel1` (Top) holds `gb_Analysis_Mode` "Analysis" (ON/OFF as buttons), `gb_Analysis_Database` "Database", `gb_Analysis_Functions` "Functions" and `gb_Analysis_Time` "Time" (`tb_Analysis_Time`, "Stepwidth:", `tb_Analysis_Stepwidth`, "sec.").
    - `panel2` (Fill) holds `gb_Analysis_Player` "Player" (min/max date fields, `sb_Analysis_Play`) and `gb_Analysis_Controls` "Controls" (5 player buttons as glyphs ⏮ ◀ ⏸ ▶ ⏭ with the old names as hover text, D-007).
    - The Rust file list of "Load Positions from File" stays inside `gb_Analysis_Database`, after the original controls.
  - **Planes:** the Rust-only tab, unchanged (D-006).
  - Update the tests that used the old labels.
- [ ] **Step 4: Run** → PASS; workspace tests → PASS.
- [ ] **Step 5: Commit** `feat(gui): MapDlg lower tabs as in the original`

### Task 6: Nearest plane map (`gb_NearestPlaneMap`, `gm_Nearest`)

**Files:**
- Modify: `crates/airscout/src/ui/map.rs`, `crates/airscout/src/ui/mod.rs`, `crates/airscout/src/ui/spectrum.rs`, `crates/airscout/src/app/shell.rs`
- Test: `crates/airscout/tests/gui_mapdlg.rs`, `crates/airscout/tests/ui_map.rs` (or the existing map test file)

**Interfaces:**
- Produces:
  - `pub struct Maps { pub main: map::MapView, pub nearest: map::MapView }`, and `ui::show(ui, state, maps: &mut Maps)` replaces the `map` parameter.
  - `MapView::show_nearest(&mut self, ui: &mut egui::Ui, state: &AppState)`:
    - Centre on the nearest plane's `IntPoint` at zoom 10, with the plane marker only (`gmo_NearestPlanes`).
    - The nearest path routes `gmr_NearestFull`/`gmr_NearestVisible` (MapDlg.cs:4170-4225, 4334).
    - Drag is allowed; wheel zoom never moves markers (MapDlg.cs:1386-1408).
    - Without a nearest plane: My position at zoom 6.
  - `pub fn nearest_view(state: &AppState) -> (GPoint, u8)`: the centre and zoom rule, pure.

- [ ] **Step 1: Write the failing tests**
  - `nearest_view_follows_plane`.
  - `nearest_view_defaults_to_my_position`.
  - `spectrum_tab_matches_reference`: `assert_subtree("MapDlg", "tp_Spectrum", None, …)`, now including `gb_NearestPlaneMap` "Nearest Plane Map".
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**
  - Implement as in Interfaces; the shell owns both views.
  - Remove the `gm_Nearest` "not ported" note in `ui/spectrum.rs`.
  - Set the `gb_NearestPlaneMap` row in `gaps.md` to `ok`.
- [ ] **Step 4: Run** → PASS; workspace tests → PASS.
- [ ] **Step 5: Commit** `feat(gui): nearest plane map in the Spectrum tab`

### Task 7: Latest News (`tp_News`, news check)

**Files:**
- Create: `crates/airscout/src/news.rs`, `crates/airscout/src/ui/news.rs`
- Modify: `crates/as-settings/src/gui.rs`, `crates/airscout/src/app/services_trait.rs`, `crates/airscout/src/services.rs`, `crates/airscout/src/app/mod.rs`, `crates/airscout/src/ui/mod.rs`
- Test: `crates/airscout/tests/news.rs`

**Interfaces:**
- **Settings:** new `GuiSettings` fields, all `#[serde(default)]` with the upstream defaults (Settings.settings:263-271, 754-756):
  - `news_url: String` = `"http://airscout.eu/index.php/news"`.
  - `news_interval_s: u64` = 3600.
  - `news_last_update: DateTime<Utc>` = 2015-01-31 15:21:00 UTC.
  - `news_feed_enabled: bool` = **false** (owner decision 2026-11-27; upstream default is true, D-013).
- **Pure function:** `pub fn news_due(last_check: Option<Instant>, now: Instant, interval_s: u64, enabled: bool) -> bool`.
- **Service:**
  - `fn check_news(&mut self, url: &str)` (default no-op) runs in the background.
  - It reads the page's last-modified time like `GetWebCreationTimeUtc`; port it from `AutoDecompressionWebClient`.
  - The result comes back as `ServiceEvent::News(Result<DateTime<Utc>, String>)`.
- **AppState:**
  - `poll_news(now)` calls `check_news` when due.
  - On `Ok(dt)` with `dt > news_last_update` it opens `pub news_prompt: Option<DateTime<Utc>>`.
  - On `Err(e)` it sets `status` to `"[HH:mm:ss] Error while reading the website <url>: <e>"` (MapDlg.cs:8571-8576).
- **Prompt:**
  - Text: "There are news on the website. Latest update: <dt>\n Do you want to read it now?", buttons Yes/No (MapDlg.cs:8598-8640).
  - Yes selects `MapTab::News` and opens the URL in the system browser (`ctx.open_url`).
  - Both answers store `news_last_update = dt` and stop the checks, as the C# stops `bw_NewsFeed`.
  - The implementer must confirm both points against the C# lines and port what they do.
- **`tp_News` body:** the URL as a link plus "Open in browser". There is no embedded browser (D-008).

- [ ] **Step 1: Write the failing tests**
  - `news_due_rules`: never when disabled, at once without a last check, then every interval.
  - `news_check_off_by_default`: `Settings::default().gui.news_feed_enabled == false`, and a fresh `AppState` never calls `check_news`.
  - `newer_page_opens_prompt`.
  - `older_page_opens_nothing`.
  - `error_goes_to_status_not_prompt`.
  - `settings_without_news_keys_read_defaults`.
  - `check_runs_off_the_ui_thread`: `poll_news` returns at once with a blocking fake service.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**
  - Implement as in Interfaces.
  - Add D-008 and D-013 (`Approved: 2026-11-27`) to `docs/gui-deviations.md`.
  - Set the `tc_Map` and `tp_News` rows in `gaps.md` to `ok`.
- [ ] **Step 4: Run** → PASS; workspace tests → PASS.
- [ ] **Step 5: Commit** `feat(gui): Latest News tab and news check as in the original`

### Task 8: Whole-window fidelity and inventory

**Files:**
- Modify: `crates/airscout/tests/gui_mapdlg.rs`, `docs/gui-reference/MapDlg.allow.json`, `docs/gui-reference/gaps.md`, `docs/superpowers/plans/2026-11-26-gui-1to1-windows-outline.md`, `docs/gui-deviations.md`

- [ ] **Step 1: Write the test** `mapdlg_matches_reference`:
  - Merge the traces of every tab selection (Map/News × Profile/Spectrum/Analysis/Planes × Single/Multi/Opt, deduplicated).
  - Call `assert_window("MapDlg", &merged)`.
  - The allowlist holds only entries with a `D-NNN` id and a reason. Runtime placeholder texts ("No Messages.", "Calculations.", "Database status.", the `lbl_Nearest_*` "0") use D-012.
- [ ] **Step 2: Run** → it must PASS without new allow entries beyond those listed. Each mismatch is fixed in the UI, not in the reference.
- [ ] **Step 3: Update the docs**
  - `gaps.md`: every MapDlg row is `ok` (Summary recomputed by the generator's merge mode).
  - Outline: correct the counts to "24 windows plus `PercentageControl` (25 reference files)" and "OptionsDlg: 19 tabs".
  - Add D-012.
- [ ] **Step 4: Run** all gates, `cargo test --workspace --locked` → PASS.
- [ ] **Step 5: Commit** `test(gui): MapDlg passes the whole-window fidelity check`

## New deviation entries (approved by the owner 2026-11-27)

- **D-006 Rust-only "Planes" tab:** MapDlg `tc_Main`. An extra tab after Analysis with the plane list (stage-3 feature, no original counterpart).
- **D-007 Glyphs instead of bitmaps:**
  - `btn_Map_PlayPause` shows "Play"/"Pause" text.
  - The 5 Analysis player buttons show ⏮ ◀ ⏸ ▶ ⏭.
  - The original uses images from `il_Main`.
- **D-008 No embedded web browser:**
  - `tp_News` shows the news URL as a link and an "Open in browser" button.
  - The original embeds a `WebBrowser`; no egui HTML renderer exists without a new dependency.
- **D-009 Screenshot as PNG:** `btn_Map_Save` writes `.png`, not `.jpg`, because no JPEG encoder is reachable without a new dependency.
- **D-010 Rust-only status fields:** after `tsl_CAT`. They hold the settings-load error, the plane counts (shown/active) and the feed status.
- **D-011 SRTM1 LED tooltip:** says "SRTM1 database status LED". The upstream text says "SRTM3", a copy-paste slip.
- **D-013 News check off by default (approved 2026-11-27):** `NewsFeed_Enabled` defaults to false; the original checks airscout.eu every hour by default. Owner decision.
- **D-012 Runtime placeholder texts:** Designer placeholder texts that the original replaces at runtime show the runtime text in Rust, for example `tsl_Status` "No Messages.", `tsl_Calculations`, `tsl_Database`, `lbl_Nearest_*` "0".
