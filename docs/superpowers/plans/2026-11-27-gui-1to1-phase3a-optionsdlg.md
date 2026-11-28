# GUI 1:1 — Phase 3A (OptionsDlg, first half) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the OptionsDlg frame and the General, Stations, Planes and Map pages, with working controls and an automated fidelity gate for those pages.

**Architecture:** Rebuild the frame and the four pages on `ui::forms`, keeping the existing Options draft and routing local DB, file and background work through the app or its services. Hand-port each page against the pinned Designer and follow the matching C# handlers through to the runtime consumers. The Phase 3A gate checks the frame and the four complete subtrees, and Phase 3B extends that gate to the full window.

**Tech Stack:** Rust, egui/eframe 0.37, egui_kittest 0.37, walkers 0.66, SQLite and TOML through existing dependencies.

**Spec:** `docs/superpowers/specs/2026-11-26-gui-1to1-design.md`. See Phase 3 and the split recommendation in `docs/superpowers/plans/2026-11-26-gui-1to1-windows-outline.md`.

## Global Constraints

- egui/eframe as today; no new crates except the one approved exception, `image = { workspace = true }` as a direct dependency of `crates/airscout` (Decision 8). The owner permits exactly one added line in `Cargo.lock` — the `"image"` entry in the `airscout` package's dependency list — because `image 0.25.10` is already resolved in `Cargo.lock` (via `eframe`/`walkers`/`arboard`) and only its already-resolved transitive crates are enabled, so no new package or version is added. `Cargo.lock` is otherwise unchanged; never `cargo update`.
- No new dependency edges or features. `tests/golden/*.json`, `upstream_commit` and `docs/gui-reference/*.json` stay unchanged.
- Upstream pinned at `91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5` (`tools/fetch-upstream.sh`). C# paths below are relative to `target/upstream/AirScout`.
- GPL-3.0-only. Texts copied from upstream Designer files are GPL like the rest; NOTICE stays accurate.
- Designer `Font` is ignored; egui default font. Control sizes may grow to fit the egui text, never shrink below the Designer size; relative arrangement stays.
- Windows are resizable. Below the scaled Designer size, containers scroll instead of clipping. Target: main window and Options fully usable at 1366×768.
- Designer strings are English; `\r\n` is normalised to `\n` before comparison. Strip mnemonics, retain deliberate literal ampersands, and record runtime text exceptions per control.
- Rust-only controls go after the original controls in the nearest container. Record each difference with an owner approval date. Use the recommended decision paths for planning and tests, and obtain approval before shipping a new difference or marking its row resolved.
- No disabled shells, no no-op buttons. Genuine capability or lifecycle guards continue to apply.
- Windows that belong to features we deliberately do not port (getkey.php, session keys, register, upload, plugin download, U4 token print #342). Their menu entries and buttons stay absent.
- Network features stay off by default and use `as_net::user_agent()`. Bulk downloads only on explicit user action. No call sign, position or email in requests or metadata, except the QRZ lookup, which sends only the call sign on explicit click (Decision 3). No credentials in logs, Debug output, errors or status.
- Synthetic test data only. Use TE1ST, TE2ST, JO50AA, JO60AA and localhost for station/network fixtures. No personal data or upstream LAN defaults.
- Preserve stage-6 prediction, feed, database and network semantics, including #155 worker/maintenance sequencing and #115 history-mode lifecycle. Add behavior only for the named gaps. Keep existing test scenarios and update label lookups when their controls move.
- No screenshots committed. Review transient local renders for the four tabs at 1366×768, 900×600 and 2× pixels-per-point. This follows the phase request on image artifacts.
- Cargo invocations use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` and `--locked` wherever supported. In each task, export these three variables before the first Cargo command. `cargo fmt --all --check` does not accept `--locked`.
- Per-task gates are `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked`. Final gates also include hamlib clippy, Linux and windows-gnu Docker gates, and green hosted CI. Sign task commits with `git commit -S`.

## Review Focus

1. At 900×600 and at 2× pixels-per-point, the right footer stays reachable and the longest page labels and inputs can be scrolled into view — Task 1 and Task 10.
2. A call lookup miss, a late elevation reply, or a late QRZ reply for the previous station leaves no stale position or elevation on screen — Task 3.
3. Switching bands or revisiting a station preserves pending QRV changes, and Cancel makes zero location/QRV writes — Tasks 3 and 4.
4. An invalid feed import containing a credential leaves the current feed (including its secret) intact and reports a sanitized error — Task 5.
5. A corrupt replacement overlay or reversed bounding box leaves the committed overlay intact, and turning it off removes its texture from rendering — Task 9.

---

## Scope and file structure

Join the gap inventory to the Designer by control name and walk the parent chain. There are 26 missing rows in Stations (including two QRZ buttons), 27 in Planes, 32 in Map and zero in General, for **85 missing rows**. All 15 overlay rows are in Map — `groupBox58` itself, its 13 children, and the one `cb_Options_ShowUserSuppliedOverlay` toggle in Map's `groupBox23`. `gm_Options_Coverage` is not a row in `gaps.md` (only `tab_Options_General`, `groupBox25`, the four labels and the four `tb_Coverage_*` boxes appear); the existing Rust General page has only the numeric inputs. Task 2 verifies that entry through the real page. The two Stations upload rows stay deliberately excluded. This keeps the proposed four-tab split and assigns the overlay to its actual Designer parent. Phase 3B owns Database (including the Map Data DB row), the five elevation pages, Path, Alarm, Network, Spectrum, Track, CAT, Watchlist, Misc. and Info. Phase 5 owns the full child-dialog fidelity ports.

- `crates/airscout/src/ui/options/mod.rs` owns the frame, draft, tab routing and child actions. `general.rs`, `stations.rs`, `planes.rs` and `map.rs` in that directory own the page controls. `fields.rs` owns draft validation.
- `crates/airscout/src/app/dialogs.rs`, `app/services_trait.rs`, `app/services_api.rs` and `ui/dialogs/mod.rs` connect the draft to local services and apply pending edits. New modules below isolate the file formats and worker operations.
- `crates/as-settings/src/{lib,gui}.rs` own additive serde-defaulted settings. `feed_service.rs`, `ui/map.rs`, `ui/map_track.rs` and `style.rs` consume committed settings. Follow `GuiSettings`'s existing redacted Debug convention.
- `crates/airscout/tests/gui_optionsdlg.rs` drives the real dialog. `tests/gui_ref/fidelity.rs` reuses the Phase 2 matcher. The Options exceptions live in `docs/gui-reference/OptionsDlg.allow.json` (consistent with `MapDlg.allow.json` and the `.allow.json` loader at `fidelity.rs:850-868`; the freeze applies only to the extractor-generated `<Window>.json` reference files, not the `.allow.json` allowlists). Reuse the existing `Allow` schema and strict matcher. Use `docs/gui-deviations.md` for the associated dated approvals.

### Task 1 — Designer frame, tabs and scoped fidelity

**Files**

- Modify `crates/airscout/src/ui/options/mod.rs`, `crates/airscout/src/ui/forms/widgets.rs`, `crates/airscout/tests/gui_ref/fidelity.rs`.
- Create `crates/airscout/tests/gui_optionsdlg.rs` and `docs/gui-reference/OptionsDlg.allow.json`.
- Test `crates/airscout/tests/gui_fidelity.rs` and `crates/airscout/tests/ui_options.rs`.

**Interfaces**

- Retain `OptionsDlg::show(&mut self, ctx: &egui::Context) -> Option<OptionsOutcome>`. Add `pub fn try_select_tab(&mut self, tab: OptionsTab) -> bool`. It validates the departing page when that page has a validator and records its errors in the status strip. Initial implementation accepts transitions. Task 4 adds Stations validation and Task 6 adds Planes validation (`tab_Options_Planes_Validating`, `OptionsDlg.cs:2215-2222`: `planes_max_alt_m < planes_min_alt_m`).
- Add `pub fn tab_control_with_guard(ui: &mut egui::Ui, name: &str, rect: egui::Rect, pages: &[(&str, &str)], selected: &mut usize, allow: impl FnMut(usize) -> bool, body: impl FnOnce(&mut egui::Ui, usize))` in `widgets.rs`. Existing tab functions delegate with their existing unconditional or boolean rules.
- In `gui_optionsdlg.rs`, add `fn trace_options(tab: OptionsTab, size: egui::Vec2, setup: impl FnOnce(&mut OptionsDlg)) -> Vec<Entry>` using a settled frame of the real dialog. Add `fn assert_options_phase_a(trace: &[Entry], include_bodies: bool)` backed by the existing strict `compare`. `false` checks shell/header only. `true` adds all four page subtrees. Task 10 supplies the merged trace. Keep trace output for unlabeled fields and custom controls so separate tests can assert they exist and are usable.
- In `fidelity.rs`, expose `pub fn load_allow(window: &str) -> Vec<Allow>` and `pub fn load_deviations() -> String` (currently private at `fidelity.rs:857,871`) so `gui_optionsdlg.rs` reuses the same loader, and have `assert_options_phase_a` filter the interim `Elevation` tab entry out of the projected trace before calling `compare` (`fidelity.rs:517-538` reports `Extra`/`WrongOrder` for a Rust-only tab inserted between `Map` and `Path`), then assert the header sits between `tab_Options_Map` and `tab_Options_Path` separately.

- [ ] **Step 1 — Write the failing tests.** In `gui_optionsdlg.rs`, `frame_and_tab_order` calls `assert_options_phase_a(&trace, false)` and asserts `frame.size() == egui::vec2(784.0, 561.0)` at default size, `status.bottom() == frame.bottom()` and `ok.bottom() < cancel.top()`. `small_frame_footer_reachable` checks both action rects are within the clipped client at 900×600 and at `pixels_per_point = 2.0`. `maintenance_still_blocks_exit` clicks both buttons with `maint_running = true` and asserts both frames return `None`.
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test gui_fidelity --test ui_options`. Expect FAIL for absent frame/trace and guard helpers.
- [ ] **Step 3 — Implement.** Use a resizable client titled “Air Scout Options” at 784×561 Designer points, keeping modal input capture and the existing close/maintenance guards. Use `designer_rect` with `tc_Options` at [12,12] size [679,524], OK at [697,471], Cancel at [697,501], and `ss_Options` docked Bottom at 22 pt. Route existing validation and web warnings to `tsl_Options_Status`, with initial “Status” and the complete messages available on hover. Scroll the body independently of the header and footer. Carry existing tab IDs and captions into the Designer order, with Spectrum as the displayed name of `SpecLab`. Per Decision 1, keep the existing combined Elevation page between Map and Path for Phase 3A. Generate neither inert tabs nor synthetic trace entries for Phase 3B pages. Compare the full frame and present original headers. In the reference projection omit the six deferred headers by exact ID (`tab_Options_GLOBE`, `tab_Options_SRTM3`, `tab_Options_SRTM1`, `tab_Options_ASTER3`, `tba_Option_ASTER1`, `tab_Options_Misc`) and Phase 3B bodies by exact subtree, and drop the interim `Elevation` tab header from the projected trace before `compare` (see Interfaces). Test the interim Elevation entry and its placement separately. In the tests merge repeated trace entries by `(path, name, kind)`; a tab header and its body container are distinct.
- [ ] **Step 4 — Run** the Step 2 command and the per-task gates. Expect PASS.
- [ ] **Step 5 — Commit** the named files with `git commit -S -m "feat(gui): restore the Options dialog frame"`.

### Task 2 — General covered area and map

**Files**

- Modify `crates/airscout/src/ui/options/{mod,general,coverage,fields}.rs` and `crates/airscout/src/tiles.rs`.
- Test `crates/airscout/tests/{gui_optionsdlg,ui_options}.rs`.

**Interfaces**

- Add `pub(super) struct AreaPreview` in `general.rs` with `Default` and `pub(super) fn show(&mut self, ui: &mut egui::Ui, s: &Settings, store: Option<&Arc<TileStore>>)` (it needs `s.gui.map_provider` and `s.gui.thunderforest_api_key` — `as-settings/src/gui.rs:89,122` — to build the `TileProvider` for `cached_tile_layer`). `OptionsDlg` owns one preview. Add `pub fn validate_area_bounds(a: &as_settings::Area) -> Vec<String>` in `fields.rs`, called by both `validate_area` and `covered_area` (`validate_area` currently takes `&Settings`, `fields.rs:44`); `pub(super) fn covered_area(a: &as_settings::Area) -> Option<GRect>` calls `validate_area_bounds` before producing a rectangle.
- Reuse `MapMemory`, `fit_view` and the coverage rectangle painter extracted from `coverage.rs`. Add `pub(crate) fn cached_tile_layer(provider: &TileProvider, store: Arc<TileStore>, ctx: &egui::Context) -> StyledSources` in `tiles.rs`, backed only by the existing local tile-store lookup and missing-tile data.

- [ ] **Step 1 — Write the failing tests.** In `gui_optionsdlg.rs`, `general_matches_designer` calls `assert_subtree("OptionsDlg", "tab_Options_General", None, &trace)` and asserts the trace has a positive rect for `gm_Options_Coverage`. A unit test in `general.rs` checks that `covered_area(&Area { min_lat: 50.0, max_lat: 51.0, min_lon: 10.0, max_lon: 11.0 })` yields those exact bounds, and reversed latitudes yield `None`. `coverage_preview_is_local` runs with no tile store and asserts zero tile/background work requests when selecting General or changing a bound.
- [ ] **Step 2 — Run** `cargo test -p airscout --locked general` and `cargo test -p airscout --locked --test gui_optionsdlg`. Expect FAIL because the General map or helpers are absent.
- [ ] **Step 3 — Implement.** Render `groupBox25`, the four `tb_Coverage_*` controls with their Designer labels/rects, and the coverage map in its Designer rect. Port `OptionsDlg.cs:100–112,230–274`: draw an unfilled 3 pt magenta rectangle, refit to the one-degree padded extent on valid bound changes and on entry, and leave the camera alone on unchanged frames. Skip drawing invalid drafts and retain validation on OK. Clamp the preview padding to valid projection bounds. Per Decision 2, use locally cached tiles for the active provider and a blank navigable basemap on cache miss. No downloads on entry or edit.
- [ ] **Step 4 — Run** the Step 2 commands and the per-task gates. Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "feat(gui): restore the General covered area map"`.

### Task 3 — Stations position, elevation and local writes

**Files**

- Modify `crates/airscout/src/ui/options/{stations,mod,fields}.rs`, `crates/airscout/src/ui/dialogs/mod.rs`, `crates/airscout/src/app/{dialogs,services_trait,services_api,station_fields,mod}.rs`, `crates/airscout/src/services.rs`, `crates/airscout/src/web_settings_json.rs`, `crates/as-settings/src/{gui,lib}.rs` and `docs/gui-deviations.md`.
- Test `crates/airscout/tests/{gui_optionsdlg,ui_stations,station_fields}.rs`.

**Interfaces**

- In `stations.rs`, add `pub(crate) fn locator_for_settings(pos: GPoint, gui: &GuiSettings) -> String` and use it on both Options and MapDlg station fields. Add `pub fn apply_call_match(&mut self, s: &mut Settings, who: Who, call: &str, pos: Option<GPoint>)` on `StationsState`. Add `StationAction::Lookup(Who)`; the call is read from that frame's draft.
- Add `Services::station_location_write(&mut self, locations: &[Location]) -> Result<usize, String>` using `StationDb::location_bulk_insert_or_update_if_newer`. Add `Services::request_options_station_details(&mut self, id: u64, who: Who, enabled: ElevationEnabled, pos: GPoint)` — `enabled`, not a single `ElevationModel`, because `ground_elevation` (`services.rs:391-400`) walks `MODEL_ORDER` with fallback to the next enabled model (only `local_obstruction_find` uses `effective_model()`, `OptionsDlg.cs:932`) — and `ServiceEvent::OptionsStationDetails { id: u64, who: Who, elevation_m: Option<i16>, obstruction: Option<bool> }`. Dispatch these using the existing worker/event pattern. Track request IDs across tabs/dialog instances at AppState level. Set the Obstructions readout to the exact C# texts (`OptionsDlg.cs:934-936`): `"Local Obstruction found for this location."` / `"Local Obstruction not found for this location."`.
- Add `Services::request_qrz_lookup(&mut self, id: u64, who: Who, url: String)` on the same worker/event pattern: a background GET to `url` with `as_net::user_agent()`, a 10 s timeout and a capped response size (1 MiB), parse `cs_lat = "…"` / `cs_lon = "…"` by extracting the quoted value up to the closing `"` (not the C# `s.IndexOf("\n") - 2`, which breaks on `\r\n`; `OptionsDlg.cs:662-665`) into a `GPoint`, and answer `ServiceEvent::QrzLookup { id: u64, who: Who, position: Result<GPoint, String> }`. Sanitize error messages (no URL query or userinfo).
- In `stations.rs`, add `fn apply_qrz_position(&mut self, s: &mut Settings, who: Who, pos: GPoint)` mirroring `OptionsDlg.cs:670-680, 828-838`: compute `loc_from_lat_lon(lat, lon, false, 3, false)`, compare its first 6 characters with `convert(draft_locator, false)`'s first 6, and on a match (draft locator at least 6 characters) set the draft lat/lon and refresh the locator/elevation readouts with the success message; otherwise show the failure message and change nothing. Both messages render in `tsl_Options_Status` (the frame's status strip, where web warnings already go; the C# `MessageBox` is not ported). Drop a `QrzLookup` whose `id` is not the pending one.
- Add to `GuiSettings` in `crates/as-settings/src/gui.rs`: `qrz_url_database: String` = `"http://www.qrz.com/db/"` (`Settings.settings:84`), and turn the `Locator_MaxLength`/`Locator_AutoLength` constants (`gui.rs:79-83`) into serde-defaulted fields `locator_max_length: i32` = 10 and `locator_auto_length: bool` = true (they become user-editable via the 6–14 digit spinner and auto-length check box). Keep the two `pub const`s (`gui.rs:79-83`) and their re-export (`lib.rs:21`) as the serde defaults of the new fields; switch every reader to the fields: `crates/airscout/src/web_settings_json.rs:12,267-268` and `crates/airscout/src/ui/options/stations.rs:15,137` (`LOCATOR_PRECISION` becomes `s.gui.locator_max_length / 2`, computed at use) (re-review N5).
- Add `OptionsDlg::pending_locations(&self, now: DateTime<Utc>) -> Vec<Location>`. Keep draft position, displayed locator and queued QRV refresh coordinated. Task 4 consumes the same `StationKey` representation and the existing `refresh_qrvs` entry point.
- Add an error-retention hook so a failed DB write on `OptionsOutcome::Ok` keeps the dialog open with the error in `tsl_Options_Status` (today `dialogs::show` drops `dlg` before `write_qrvs` at `ui/dialogs/mod.rs:52-58`); the draft survives for retry. Preserve the Rust-only My/DX antenna-height rows after the original `groupBox4`/`groupBox5` children with a `D-NNN` `Extra` allow entry (I8).

- [ ] **Step 1 — Write the failing tests.** With fake services, `options_lookup_hit_then_miss` edits My to TE1ST and asserts the JO50AA coordinates and a fake elevation of 123 m, then edits to a DB miss and asserts blank locator/elevation, invalid coordinates, and no reused TE1ST QRV key. `stale_station_details_are_ignored` delivers the older My response after the newer one and asserts the newer value remains. `station_map_stages_local_write` moves DX within JO60AA, asserts its staged `Location` has `GeoSource::FromUser`, then asserts zero writes on Cancel and one on OK. `locator_settings_keep_storage_key` changes 6 to 14 display digits, disables auto length and asserts 14 displayed chars and the same six-character QRV key. `qrz_lookup_hit_updates_position` points `qrz_url_database` at a localhost fake server (a `TcpListener`, never the real QRZ) serving `cs_lat = "50.01"` / `cs_lon = "10.03"` for TE1ST, sets the My locator to JO50AA, clicks the QRZ button, and asserts the My position becomes (50.01, 10.03) and the success message. `qrz_lookup_mismatch_keeps_position` serves a position whose 6-character locator differs from the draft and asserts the position is unchanged and the failure message is shown. `stale_qrz_reply_is_ignored` edits the call after the click and asserts the late reply does not overwrite the newer position.
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test ui_stations --test station_fields`. Expect FAIL for missing lookup/elevation/write path.
- [ ] **Step 3 — Implement.** Lay out `groupBox4`, `groupBox5`, `groupBox16`, `groupBox14` and `groupBox18` at the Designer rects. Port call hit/miss, locator and local-obstruction status from `OptionsDlg.cs:525–799,905–982,1130–1190`. Resolve the call from the local station DB only on real call edits (never the network); read ground elevation using the draft model through existing ground-elevation service logic, keyed on position/model and refreshed only on change. Elevation is a separate readout from the already-supported antenna height. Populate Obstructions status from the same local/model key as Manage. Keep Map, Horizon and Manage wired to the existing child dialogs. Do not stage a location for an unchanged call/point or for child Cancel. For confirmed changes use the six-character DB key, normalized call, and C# precision rule for FromUser versus FromLoc. Skip invalid rows and keep the draft open if a local write fails. Per Decision 4, stage the station Map result through outer OK and preserve the existing antenna-height control after the original group children. Add the 6–14 digit spinner (default 10, even steps), auto length (default true), and reuse `locator_small_letters_for_subsquares` and `location_restrict_to_area`'s existing settings/worker behavior. Use the settings on the live station displays and export their values in the existing web settings fields. Per Decision 3, port both QRZ buttons 1:1 (`OptionsDlg.cs:644-690, 802-848`), on explicit click only. Render `btn_MyCall_QRZ` at [89,134] and `btn_DXCall_QRZ` at [90,134], both 100×20 titled "QRZ lookup" with the Designer tooltip. A click starts a background `request_qrz_lookup` to `qrz_url_database + call` with `as_net::user_agent()`, parses `cs_lat`/`cs_lon`, and applies the position only when the parsed 6-character locator matches the draft locator; show the exact C# success message ("Position update from QRZ.com was performed succesfully.") on a match, otherwise the exact C# failure message ("Position query at QRZ.com failed or does not match with current locator. \nNo update was performed."), both in `tsl_Options_Status`. Never automatic, never on the UI thread, and a stale reply is dropped. Per Decision 5 retain the informational copy up to “Your local database is updated.” and omit both upload buttons under the binding non-goal.
- [ ] **Step 4 — Run** the Step 2 command and the per-task gates. Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "feat(gui): restore Options station positions and location settings"`.

### Task 4 — Stations band stepper and QRV lifecycle

**Files**

- Modify `crates/airscout/src/ui/options/{stations,mod}.rs`, `crates/airscout/src/ui/dialogs/mod.rs`, `crates/airscout/src/app/{dialogs,services_trait,services_api}.rs` and `crates/as-stations/src/db.rs`.
- Test `crates/airscout/tests/{gui_optionsdlg,ui_stations}.rs`.

**Interfaces**

- In `stations.rs`, add `pub enum QrvEdit { Upsert(Qrv), Delete { key: StationKey, band: Band, expected_updated_at: DateTime<Utc> } }`, `StationsState::qrv_edits(&self, now: DateTime<Utc>) -> Vec<QrvEdit>`, and `StationsState::validate_current_qrv(&self, band: Band) -> Result<(), &'static str>`. Retain `refresh_qrvs`, with a per-dialog cache keyed by station and band.
- Add `Services::write_options_station_edits(&mut self, locations: &[Location], edits: &[QrvEdit]) -> Result<(), String>`. Replace Task 3's location-only OK write in the normal entry point. In `as-stations`, give `StationDb` a transaction method `apply_station_edits(&self, locations: &[Location], upserts: &[Qrv], deletes: &[(&str, &str, Band, DateTime<Utc>)]) -> Result<StationEditCounts, StationError>` (returning rows upserted/deleted) that reuses the existing key normalization, conditional upserts and SQL delete predicate with the expected timestamp. `write_options_station_edits` stamps `QRV_USER_EDIT_MARKER` whenever any QRV row is upserted or deleted, exactly as `qrv_write` does today (`services_api.rs:342-347`), so the background calculator reruns on antenna-height edits.

- [ ] **Step 1 — Write the failing tests.** `band_step_preserves_qrv_edits` edits My antenna height/gain/power to 10/12/50, clicks ▲ and ▼, and asserts the original band again and the exact trio. `partial_qrv_refuses_tab_change` fills only power and asserts `!dlg.try_select_tab(OptionsTab::Map)` and no writes. `empty_qrv_deletes_only_loaded_row` clears a stored row with timestamp 2026-11-27 12:34:56 UTC, asserts the timestamp text before clearing, then asserts one conditional deletion on OK and none on Cancel. Include a storage test where a newer concurrent row survives the conditional deletion, and a test that `QRV_USER_EDIT_MARKER` advances on QRV upsert and on delete (and not when nothing changed).
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test ui_stations` and `cargo test -p as-stations --locked`. Expect FAIL for missing band and delete behavior.
- [ ] **Step 3 — Implement.** Render `groupBox44`, `groupBox45`, `groupBox46` as the simultaneous My/DX rows and shared band selector. Implement C# `:876–904` (BandDown/BandUp) and `:1006–1128` (MyQRV/DXQRV) using `Band::next/previous` clamping, validated text buffers, explicit empty versus zero, and timestamps formatted `%Y-%m-%d %H:%M:%S`. Both QRV trios must be fully empty or finite and complete before leaving Stations, changing bands or accepting Options. Cache pending changes by normalized station/DB locator and band across call, position and band changes. On OK write changed locations and QRV rows together and only then apply settings/restart workers. On write failure show a sanitized status and preserve the draft for retry. Per Decision 6 display absent rows empty, keep read-time RF defaults, and commit DB changes only on outer OK. Preserve the existing tests of independent My and DX edits and OK/Cancel through their new controls. Flip the Stations rows this task and Task 3 own in `gaps.md` to `ok`.
- [ ] **Step 4 — Run** the Step 2 commands and the per-task gates. Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "feat(gui): restore Options band and QRV controls"`.

### Task 5 — Planes feed import and export

**Files**

- Modify `crates/airscout/src/ui/options/{planes,mod}.rs`, `crates/airscout/src/ui/dialogs/mod.rs`, `crates/airscout/src/app/dialogs.rs`.
- Create `crates/airscout/src/feed_files.rs`.
- Test `crates/airscout/tests/{gui_optionsdlg,ui_feeds}.rs`.

**Interfaces**

- Extend `planes::FeedAction` with `Import(usize)`, `Export(usize)` and `Default(usize)`. `groupBox6` ("Plane Feeds") renders the three `btn_Options_PlaneFeed{N}_Default` (x 578), `_Import` (x 472) and `_Export` (x 520) buttons at their Designer rects (rows y 17/44/71), enabled exactly when the selected slot has a feed. `Default` restores the slot's feed to its built-in defaults (`OptionsDlg.cs:2523-2560`, matching the existing `FeedSettingsDlg::restore_defaults`); these three `_Default` buttons are the `ok` rows in `gaps.md:437-439` that today live only inside `FeedSettingsDlg`. `OptionsDlg::handle_feed_action` routes them (OptionsDlg.cs:2443-2521).
- Add `pub fn export_feed(name: &str, table: &toml::Table, path: &Path) -> Result<(), String>` and `pub fn import_feed(path: &Path) -> Result<(String, toml::Table), String>` in `feed_files.rs`, plus `pub fn feed_file_name(name: &str, export: bool) -> String`. The file holds the existing `FeedSettings` TOML table (the same serialisation `as-settings` already reads/writes), one feed per file. Because `Secret` serializes as a plain string (`as-settings/src/secret.rs:4-5`, `#[serde(transparent)]`; only `Debug` is redacted), the exported file necessarily contains credentials in plaintext: write it with `as_settings::write_private_atomic` (0600 on Unix, matching `settings.toml`) and redact credentials only in errors, status and `Debug` output — never in the file. This resolves the C5 contradiction: secrets survive the round-trip in the file, and the redaction boundary is errors/status/logs.
- Import replaces `draft.feed_settings[name]` only after a successful, non-empty parse; on any error the current table (including its secret) is untouched and the status strip shows a sanitized message (Review Focus 4). Export writes the slot's current table. `draft.feed_settings` is keyed by feed name (not slot index); import applies the parsed table to the feed currently selected in the target slot (via `feed.load_settings`), and the round-trip test selects the same feed in slot 2 before importing.
- File selection follows Decision 7: a path text field plus a directory listing of `<data_dir>/Tmp` (the existing "Load Positions from File" pattern), no native dialog.

- [ ] **Step 1 — Write the failing tests.** `feed_export_import_roundtrip` exports slot 1's table to a temp file, selects the same feed in slot 2, and re-imports it, asserting the table and a secret survive. `feed_import_error_keeps_current_feed` imports a malformed file (and a file whose table carries a credential) and asserts the slot's existing settings and secret are unchanged and the status strip reports the error. `feed_buttons_enabled_per_feed` asserts the Import/Export/Default buttons are disabled while a slot has no feed. `feed_default_restores_defaults` asserts the Default button restores the slot's built-in defaults. Update the `ui_feeds.rs` tests that click `"Feed settings 1..."` for the new buttons/labels (I7).
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test ui_feeds`. Expect FAIL: the buttons and `feed_files` helpers are absent.
- [ ] **Step 3 — Implement.** Render the nine buttons (three each of Default/Import/Export); wire them to `restore_defaults`/`export_feed`/`import_feed`; carry the pending action through `FeedAction` and apply it in the app. Use the `feed_settings`/`load_settings` tables so an unedited feed is exportable from its defaults. Set the nine `btn_Options_PlaneFeed{N}_*` rows in `gaps.md` to `ok`.
- [ ] **Step 4 — Run** the Step 2 command and the per-task gates. Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "feat(gui): restore Options plane feed import and export"`.

### Task 6 — Planes Debug group and general feed settings

**Files**

- Modify `crates/airscout/src/ui/options/{planes,mod,fields}.rs`, `crates/as-settings/src/lib.rs`, `crates/airscout/src/feed_service.rs`, `crates/as-feeds/src/{pipeline,worker}.rs`, `crates/airscout-planeserver/src/service.rs`, `crates/as-cli/src/main.rs`.
- Test `crates/airscout/tests/{gui_optionsdlg,ui_feeds,ui_options}.rs` and `crates/as-feeds/tests/worker.rs`.

**Interfaces**

- Render `groupBox36` "Debug" (its `cb_Options_TracePositions` check box) and, inside `groupBox48`, the four missing controls at their Designer rects: `cb_Options_Planes_ExtendedPlausibilityCheck_Enabled`, `int32TextBox1` (label `label33`, `Planes_ExtendedPlausibilityCheck_MaxErrorDist`, range 10..=600), `tb_Options_Planes_Interval` (label `label22` "Interval [sec]:", range 10..=600), and `cb_Options_Planes_LogErrors`.
- Add additive serde-defaulted `Settings` fields with the C# defaults: `planes_log_errors: bool` = false (`Planes_LogErrors`) and `planes_trace_positions: bool` = false (`Planes_TracePositions`). Reuse the existing `planes_interval_s` (60), `planes_extended_plausibility` (true) and `planes_max_error_dist` (30.0). Implement `tab_Options_Planes_Validating` (`OptionsDlg.cs:2215-2222`: `planes_max_alt_m < planes_min_alt_m`) in `fields::validate` (assigned to this task, I1).
- `planes_log_errors` and `planes_trace_positions` reach the feed worker without touching `FeedArgs` (re-review N1: `FeedArgs` has 15 exhaustive literal sites). `log_errors` stays on the existing `as-feeds::pipeline::PipelineConfig::log_errors` (`pipeline.rs:45`, never read today): set it from `planes_log_errors` at `feed_service.rs:103-107` (currently hardcoded `false`) and read it in the pipeline. Add `trace_positions: Option<PathBuf>` (the plane-positions directory, `None` = off) to `as-feeds::worker::WorkerConfig` (`worker.rs:24-28`) and update its four literal sites: `feed_service.rs:100` (from the setting), `airscout-planeserver/src/service.rs:262` and `as-cli/src/main.rs:447` (`None`, their own settings have no trace option), and `as-feeds/tests/worker.rs:113` (`None`). Do **not** export the two booleans in `web_settings_json.rs` — `tests/web_api.rs:56-127` asserts `/settings.json` has exactly the 45 curated keys, so adding them breaks the stage-6 network preservation gate. When `planes_log_errors` is true, emit a `FeedEvent`/warning log for each rejected position without credentials; when `planes_trace_positions` is true, write one CSV row per position to `<tmp_dir>/PlanePositions/<hex>.csv` exactly as `PlaneFeed.cs:409-465` does (header + 20 columns, filename sanitized from the hex).

- [ ] **Step 1 — Write the failing tests.** `debug_group_matches_designer` asserts the controls exist under `groupBox48`/`groupBox36` at their rects. `settings_defaults` asserts `planes_log_errors == false`, `planes_trace_positions == false` and `Settings::default().planes_interval_s == 60`. `log_errors_reaches_worker_config` builds the worker config from `Settings` and asserts `pipeline.log_errors` and `trace_positions` follow the settings. `planes_alt_order_validated` (in `ui_options.rs`) sets `planes_max_alt_m < planes_min_alt_m` and asserts `fields::validate` rejects it with the C# message (`OptionsDlg.cs:2215-2222`), and accepts equal values (re-review N2).
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test ui_feeds --test ui_options` and `cargo test -p as-feeds --locked --test worker`. Expect FAIL.
- [ ] **Step 3 — Implement.** Lay out the controls; bind them to the settings; add the two fields; set `PipelineConfig::log_errors` from the setting and add `WorkerConfig::trace_positions` at its four sites (do not touch `FeedArgs` or `web_settings_json.rs`); add the Planes altitude check to `fields::validate`. Set the eight rows (`groupBox36`, `label33`, `label22`, `cb_Options_TracePositions`, `int32TextBox1`, `cb_Options_Planes_ExtendedPlausibilityCheck_Enabled`, `tb_Options_Planes_Interval`, `cb_Options_Planes_LogErrors`) in `gaps.md` to `ok`.
- [ ] **Step 4 — Run** the Step 2 command and the per-task gates. Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "feat(gui): restore Options Planes debug and feed settings"`.

### Task 7 — Planes Database Maintenance and live position filters

**Files**

- Modify `crates/airscout/src/ui/options/{planes,mod}.rs`, `crates/as-settings/src/lib.rs`, `crates/airscout/src/app/{dialogs,services_trait,services_api}.rs`, `crates/airscout/src/background.rs`, `crates/as-aircraft/src/positions.rs`.
- Test `crates/airscout/tests/{gui_optionsdlg,ui_feeds}.rs`.

**Interfaces**

- Render `groupBox40` "Database Maintenance": `ud_Options_Planes_Position_DatabaseLifetime`, `cb_Options_Planes_KeepHistory`, and labels `label32` "Keep aircraft positions for:" / `label55` "Aircraft positions older than above entered value will be deleted on next startup" / `label53` "days.". Add `aircraft_database_max_days_lifetime: i64` = 7 (upstream `AircraftDatabase_MaxDaysLifetime`, `Settings.settings:517-518` and `OptionsDlg.Designer.cs:7381`; matches `MaintainerConfig::default().max_days == 7`, `positions.rs:410`), range 1..=365 (`Designer.cs:7384-7393`). Wire keep-history and lifetime to the existing position-history/cleanup worker: read the setting in `background.rs:317` (currently hardcodes `MaintainerConfig::default()`) and pass it into `MaintainerConfig`. Guard `positions.rs:435` with `if cfg.max_days > 0` so a stale `0` never deletes every position, and preserve stage-6 `MaintainerConfig::default()` for the other fields.
- Render the live filters in `groupBox26` at their rects: `tb_Options_Planes_Filter_MinAlt` (label `label93` "Minimal Altitude (Filter OFF = 0:", unit `label92` "m") and `tb_Options_Planes_Filter_Max_Circumcircle` (label `label95` "Maximal Path Midpoint Circumcircle (Filter AUTO = 0) (Filter OFF = -1):", unit `label94` "km"), plus the unit label `label220` "m". These bind to the existing `gui.planes_filter_min_alt_m` and `gui.planes_filter_max_circumcircle_km` — the same fields the main window's `gb_Map_Filter` edits — so the two stay in sync.

- [ ] **Step 1 — Write the failing tests.** `database_maintenance_matches_designer` asserts the group and its controls. `live_filters_share_main_window_settings` edits the Planes-tab field and asserts `gui.planes_filter_min_alt_m` changes. `lifetime_default_matches_csharp` asserts `aircraft_database_max_days_lifetime == 7` and the range 1..=365. `lifetime_zero_skips_age_delete` asserts `maintain` with `max_days == 0` deletes no rows by age (the `positions.rs:435` guard).
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test ui_feeds`. Expect FAIL.
- [ ] **Step 3 — Implement.** Lay out the controls; add `aircraft_database_max_days_lifetime`; bind the live filters to the shared fields; route the lifetime/keep-history to the cleanup worker (`background.rs:317`, `positions.rs:435`). Set the 13 rows (`groupBox40`, `label32`, `label53`, `label55`, `cb_Options_Planes_KeepHistory`, `ud_Options_Planes_Position_DatabaseLifetime`, `label92`, `label93`, `label94`, `label95`, `label220`, `tb_Options_Planes_Filter_MinAlt`, `tb_Options_Planes_Filter_Max_Circumcircle`) in `gaps.md` to `ok`.
- [ ] **Step 4 — Run** the Step 2 command and the per-task gates. Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "feat(gui): restore Options Planes database maintenance and live filters"`.

### Task 8 — Map overlay toggles, gauges and multi-path

**Files**

- Modify `crates/airscout/src/ui/options/{map,track,mod}.rs`, `crates/as-settings/src/gui.rs`, `crates/airscout/src/ui/{map,map_track}.rs`, `crates/airscout/src/app/{mod,services_trait,services_api}.rs`, `crates/airscout/src/services.rs`.
- Test `crates/airscout/tests/{gui_optionsdlg,ui_options}.rs`.

**Interfaces**

- `groupBox23` "General": render `cb_Options_Distances_Activated` "Distance Circles", `cb_Options_Locators_Activate` "Locator Grid", `cb_Options_Watchlist_Activate` "Watchlist Locations" and `cb_Options_Airports_Activate` "Airports" at their rects (`cb_Options_Beamwidths_Activate` and `cb_Options_ShowUserSuppliedOverlay` share the box). Add additive serde-defaulted `GuiSettings` bools with the C# defaults (`Settings.settings:332,386,721-732`): `map_show_distances` = false, `map_show_locators` = false, `map_show_watchlist` = true, `map_show_airports` = true.
- `groupBox49` "Multi-Path": render the group, `cb_Options_Map_SmallMarkers` "Use Small Markers for all Path Marks" (default true) and `cb_Options_Map_LabelCalls` "Show Labels with Callsign at Locations" (`loc=[18,35]` in `OptionsDlg.json`; today it renders as a free-standing check box in `map.rs:52`). Reuse the existing `s.web.map_small_markers` (`as-settings/src/web.rs:16,35`, served at `web_settings_json.rs:298`) — do **not** add a duplicate `gui.map_small_markers`.
- `groupBox22` "Tracking Gauges": render `ud_Options_Gauges_Width`, `cb_Options_Gauges_ForeColor` (a DropDownList of the known colors, `OptionsDlg.cs:1215-1344`) and labels `label188`/`label198`, and move `cb_Options_Gauges_Show` here from `track.rs:108-111` (removing the duplicate there). Add `gauge_width: u32` = 175 (`Map_TrackingGaugeWidth`, range 175..=1000, `Designer.cs:6604-6617`) and `gauge_forecolor: String` = "Black" (`Map_TrackingGaugeColor`) with the C# `KnownColor`→RGB mapping.
- Every new toggle drives the existing `crate::overlays`/`ui/map.rs` renderer through the same path `map_show_beam_width`/`map_label_calls` use. `distance_rings`/`locator_grid` are already drawn unconditionally (`map.rs:602-603`), so only gate them on `map_show_distances`/`map_show_locators`. Airports are **not** loaded anywhere in `AppState`/`Services` today (only `web_api.rs:152` queries `AircraftDb::airport_get_all()`; C# loads them in `bw_AirportMapper`, `MapDlg.cs:9875-9908`), and watchlist coordinates are not resolved for `MapView` (`MapDlg.cs:5235-5259`): add the airport loader and watchlist-coordinate resolution here, wired through `app/{mod,services_trait,services_api}.rs`, `services.rs` and `ui/map_track.rs` (the gauge painter). The gauge controls drive the gauge painter in `map_track.rs`.
- Decision 10: omit `groupBox39` (the empty placeholder group) and allowlist it.

- [ ] **Step 1 — Write the failing tests.** `map_overlay_toggles_match_designer` asserts the toggles (Distances/Locators/Watchlist/Airports), `cb_Options_Map_SmallMarkers` + `cb_Options_Map_LabelCalls` in `groupBox49`, and the gauge controls + `cb_Options_Gauges_Show` under `groupBox22`. `toggles_reach_renderer` flips each toggle and asserts the renderer's overlay flags change (`distance_rings`/`locator_grid` gate on the flags). `map_defaults_match_csharp` asserts `map_show_distances == false`, `map_show_locators == false`, `map_show_watchlist == true`, `map_show_airports == true`, `map_small_markers == true`, `gauge_width == 175`. `gauge_controls_match_designer` asserts the width spin range 175..=1000 and the forecolor combo.
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test ui_options`. Expect FAIL.
- [ ] **Step 3 — Implement.** Lay out the controls; add the fields; move `cb_Options_Gauges_Show` out of `track.rs` and `cb_Options_Map_LabelCalls` into `groupBox49`; wire the renderer and add the airport loader. Add the `groupBox39` allow entry. Set the 12 covered rows (`groupBox49`, `cb_Options_Map_SmallMarkers`, `cb_Options_Map_LabelCalls`, `cb_Options_Distances_Activated`, `cb_Options_Locators_Activate`, `cb_Options_Watchlist_Activate`, `cb_Options_Airports_Activate`, `groupBox22`, `cb_Options_Gauges_Show`, `ud_Options_Gauges_Width`, `cb_Options_Gauges_ForeColor`, `label188`, `label198`) in `gaps.md` to `ok`.
- [ ] **Step 4 — Run** the Step 2 command and the per-task gates. Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "feat(gui): restore Options map overlay toggles and gauges"`.

### Task 9 — User-supplied overlay image

**Files**

- Modify `Cargo.toml` (root `[workspace.dependencies]`), `crates/airscout/Cargo.toml`, `crates/airscout/src/ui/options/{map,mod}.rs`, `crates/as-settings/src/gui.rs`, `crates/airscout/src/ui/map.rs`.
- Create `crates/airscout/src/overlay_image.rs`.
- Test `crates/airscout/tests/{gui_optionsdlg,ui_options}.rs`.

**Interfaces**

- Render `groupBox58` "User supplied Overlay": `tb_Options_UserSupplied_Filename`, `btn_Options_UserSupplied_Filename` "Select", `pc_Options_UserSupplied_Opacity`, the four bound fields (`cb_Options_UserSupplied_MaxLat`/`tb_Options_UserSupplied_MinLat`/`cb_Options_UserSupplied_MaxLon`/`cb_Options_UserSupplied_MinLon`) and labels `label34/200/204/208/209/210`. Render `cb_Options_ShowUserSuppliedOverlay` in `groupBox23`.
- Add additive serde-defaulted `GuiSettings` fields with the C# defaults: `overlay_filename: Option<PathBuf>` = None (`Map_UserSuppliedOverlay_Filename`), `overlay_opacity: f32` = 0.5, `overlay_min_lat` = 49, `overlay_max_lat` = 59, `overlay_min_lon` = -14.5, `overlay_max_lon` = 6, and `overlay_enabled: bool` = false (`Map_ShowUserSuppliedOverlay`).
- `overlay_image.rs` owns the decoded texture: `pub struct OverlayImage { texture: egui::TextureHandle, bounds: GRect }`, `pub fn load(path: &Path, bounds: GRect, ctx: &egui::Context) -> Result<OverlayImage, String>` (Decision 8), and `pub fn draw(&self, painter: &egui::Painter, projector: &walkers::Projector<'_>, opacity: f32)` that projects `self.bounds` to screen with the same `walkers::Projector` the map layers use (`ui/map.rs:244-247`) (re-review N3) and paints the texture at the overlay opacity. The map renderer draws it after the tile layer and before the vector overlays, only while `overlay_enabled` and zoom is in 1..=20 (`Map_UserSuppliedOverlay_FromZoomLevel`/`_ToZoomLevel`). `MapView`/`AppState` keys the loaded texture by `(path, bounds)` and reloads only when that key changes (never per frame); a load/validation failure is reported through `AppState.status` and keeps the previous committed texture.
- A missing file, decode error, or invalid/reversed bounding box keeps the previously committed overlay intact and shows a sanitized status; turning the toggle off removes the texture from rendering (Review Focus 5). The texture is released on file change or toggle-off. File selection follows Decision 7.

- [ ] **Step 1 — Write the failing tests.** `overlay_group_matches_designer` asserts the group, bounds fields, opacity and Select button. `overlay_load_and_draw` loads a small fixture and asserts the texture and projected rect. `corrupt_overlay_keeps_committed` and `reversed_bounds_keeps_committed` assert the committed overlay survives. `toggle_off_removes_texture` asserts the texture is dropped.
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test ui_options`. Expect FAIL.
- [ ] **Step 3 — Implement.** Add `image = { version = "0.25.10", default-features = false, features = ["png", "jpeg", "bmp", "tiff"] }` to root `Cargo.toml` `[workspace.dependencies]` and `image = { workspace = true }` to `crates/airscout/Cargo.toml` (Decision 8; the only approved dependency). The four features map to crates already in `Cargo.lock` (`png` → `png 0.18.1`, `jpeg` → `zune-core`/`zune-jpeg`, `tiff` → `tiff`, `bmp` → no extra crate), so `image 0.25.10`'s own dependency list does not change. Verify the `Cargo.lock` diff is **exactly one line** — `"image",` added to the `airscout` package's dependency list — with no new package and no version change, then build with `--locked`. Lay out the controls; add the fields; implement `overlay_image`; draw it in the map renderer. Set the 15 overlay rows (`groupBox58`, its 13 children, and `cb_Options_ShowUserSuppliedOverlay`) in `gaps.md` to `ok`.
- [ ] **Step 4 — Run** the Step 2 command and the per-task gates, plus `cargo build --locked` after the one-line `Cargo.lock` update (assert `git diff --stat Cargo.lock` shows a single added line). Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "feat(gui): restore Options user-supplied overlay"`.

### Task 10 — Info window font/tooltip, visibility labels, whole-window fidelity

**Files**

- Modify `crates/airscout/src/ui/options/{map,planes,mod}.rs`, `crates/airscout/tests/gui_optionsdlg.rs`, `docs/gui-reference/OptionsDlg.allow.json`, `docs/gui-reference/gaps.md`, `docs/gui-deviations.md`, `docs/superpowers/plans/2026-11-26-gui-1to1-windows-outline.md`.
- Test `crates/airscout/tests/{gui_optionsdlg,gui_fidelity}.rs`.

**Interfaces**

- In `groupBox7` "Info Window Elements", move the whole `InfoWin` block from `planes.rs:51-72` onto `tab_Options_Map` and rebuild it as the Designer lays it out: the three column boxes `groupBox29`/`groupBox33`/`groupBox34` (empty titles) hold the `InfoWin` check boxes, with `label74` "Always visible" / `label75` "Visible only in detailed window when crossing" / `label76` "Visible only in detailed window and when tracking" as children **inside** their column box at the **bottom** (`y=136`, `OptionsDlg.json`/`Designer.cs`), not above the boxes. Switch `InfoWin_Metric`/`InfoWin_Imperial` from the current check boxes to mutually-exclusive `widgets::radio` (they are `RadioButton`s in the Designer). Render the `tb_Options_Map_ToolTipFont`/`label62` "Font:"/`btn_Options_SelectFont` row. Remove the `InfoWin` controls from `planes.rs` in this same task so no gap or duplicate remains between Tasks 5–7 and 10.
- `cb_Options_InfoWin_Speed` ("Plane Speed") is a child of `groupBox7` at `loc=[275,310]` but `groupBox7` is `size=[394,291]`, so it falls outside the group's clip rect. Place it inside `groupBox33` with a `D-NNN` `"moved"` entry (Speed is a detailed-mode value, `style.rs:134`).
- Decision 9 (unambiguous): keep `label62` "Font:" and render `tb_Options_Map_ToolTipFont` read-only showing `"egui default"`, and omit/allowlist only `btn_Options_SelectFont` (egui single font, D-004). Step 1's font-row assertion covers this.
- `fn trace_options_merged(...)` merges the per-tab traces (frame + General/Stations/Planes/Map) and `assert_options_phase_a(&trace, true)` compares the four complete subtrees with the strict matcher. The allowlist holds only D-NNN entries.

- [ ] **Step 1 — Write the failing tests.** `info_window_matches_designer` asserts the three captions inside their column boxes, the Speed control in `groupBox33`, and the font row. `whole_options_phase_a_matches_reference` merges the traces and calls `assert_options_phase_a(&trace, true)` (frame plus the four complete subtrees). `allowlist_only_deviations` asserts every allow entry carries a `D-NNN` id.
- [ ] **Step 2 — Run** `cargo test -p airscout --locked --test gui_optionsdlg --test gui_fidelity`. Expect FAIL (captions/font row absent; merged trace not implemented).
- [ ] **Step 3 — Implement.** Lay out the three captions inside their column boxes and the font row; wire the merged trace; add the deviation(s) and the allow entries; set the `groupBox7` rows in `gaps.md` to `ok <path>:<line>` using the `gaps.md:9-11` status syntax, and update the Summary table at `gaps.md:1047`. Update the outline's OptionsDlg counts and `docs/gui-deviations.md`.
- [ ] **Step 4 — Run** the Step 2 command, `cargo test --workspace --locked` and the final gates. Expect PASS.
- [ ] **Step 5 — Commit** with `git commit -S -m "test(gui): Options passes the Phase 3A fidelity check"`.

---

## Open owner decisions

These must be decided before (or while) the plan runs; none is guessed here. Tasks
depending on one reference its number and default to the recommendation.

### Decision 1 — Keep the combined Elevation page for Phase 3A (Task 1)

C#: `OptionsDlg.Designer.cs` — five elevation tabs (`tab_Options_GLOBE`, `tab_Options_SRTM3`,
`tab_Options_SRTM1`, `tab_Options_ASTER3`, `tba_Option_ASTER1`) between Map and Path.

1. Keep the existing combined `Elevation` page between Map and Path; the five per-model pages
   and their copyright/info groups land in Phase 3B. **(Recommended)**
2. Split the five elevation pages now; 3A grows well past the other three tabs.

**Decided: option 1.**

### Decision 2 — General covered-area map tiles (Task 2)

C#: `OptionsDlg.cs:100-112, 230-274` (`tab_Options_General_Update`), `gm_Options_Coverage`.

1. Draw from locally cached tiles for the active provider and a blank, navigable basemap on a
   cache miss; no network on entry or edit. **(Recommended)**
2. Download the area's tiles on entry (network on by default — contradicts the binding
   network-off rule).

**Decided: option 1.**

### Decision 3 — QRZ lookup buttons (Task 3)

C#: `btn_MyCall_QRZ_Click` / `btn_DXCall_QRZ_Click` (`OptionsDlg.cs:644-690, 802-848`), network lookup.

1. Omit both buttons; QRZ is an external callsign lookup and a binding non-goal.
2. Port both buttons 1:1 against QRZ, on explicit click only — a background request to
   `QRZ_URL_Database` + call sign with `as_net::user_agent()`, the C# `cs_lat`/`cs_lon` parsing
   and locator match, and the C# success/failure messages; never automatic, never on the UI
   thread.
3. Port the lookup against a local station-DB-only fallback (no network).

**Decided: option 2.**

### Decision 4 — Stage the station Map result through outer OK (Task 3)

C#: `btn_Options_MyMap_Click` / `btn_Options_DXMap_Click` call
`LocationInsertOrUpdateIfNewer` immediately.

1. Stage the Map result in the draft and write it to the station DB only on outer OK, matching
   the dialog's transactional draft. **(Recommended)**
2. Write through immediately like the C# (breaks Cancel's no-write guarantee).

**Decided: option 1.**

### Decision 5 — Informational copy, no upload buttons (Task 3)

C#: `label52` text; `btn_Options_MyUpdate_Click` / `btn_Options_DXUpdate_Click` (`Send Update!`).

1. Keep the label up to "Your local database is updated." and omit both `Send Update!` buttons
   (sharing QRV data is a binding non-goal). **(Recommended)**
2. Keep the full original label including the sharing sentence.

**Decided: option 1.**

### Decision 6 — Absent rows empty; read-time RF defaults; commit on outer OK (Task 4)

C#: QRV load/save, `OptionsDlg.cs:984-1128`.

1. Show absent QRV rows empty, apply the band RF defaults only at read time, and commit DB
   changes only on outer OK. **(Recommended)**
2. Show defaults for absent rows (matches today's grid) and write on tab leave like the C#.

**Decided: option 1.**

### Decision 7 — File selection without a native dialog (Tasks 5, 9)

C#: `OpenFileDialog`/`SaveFileDialog` in the feed and overlay handlers.

1. A path text field plus a listing of `<data_dir>/Tmp` (the existing "Load Positions from
   File" pattern); no new dependency. **(Recommended)**
2. Add `rfd` for a native dialog (new dependency edge — contradicts the constraint).

**Decided: option 1.**

### Decision 8 — Overlay image decoding (Task 9)

C#: `btn_Options_UserSupplied_Filename_Click` accepts `*.jpg;*.jpeg;*.png;*.bmp;*.tiff`.

1. Add `image = { version = "0.25.10", default-features = false, features = ["png", "jpeg", "bmp", "tiff"] }`
   to root `[workspace.dependencies]` and `image = { workspace = true }` as a direct dependency
   of `airscout`; decode the listed formats. The four features use only crates already in
   `Cargo.lock` (`png` → `png 0.18.1`, `jpeg` → `zune-core`/`zune-jpeg`, `tiff` → `tiff`; `bmp`
   has none), so `Cargo.lock` gains exactly one line — the `"image"` entry in `airscout`'s
   dependency list — with no new package or version change. **(Recommended)**
2. Hand-decode PNG only (no new edge, but limited and no interlaced/palette support).
3. Restrict to SVG via `egui_extras` (already default), changing the accepted filter.

**Decided: option 1.**

### Decision 9 — Font selection and tooltip font (Task 10)

C#: `btn_Options_SelectFont_Click` (`OptionsDlg.cs:1280-1319`), `tb_Options_Map_ToolTipFont`.

1. Keep `label62` "Font:" and render `tb_Options_Map_ToolTipFont` read-only showing
   `"egui default"`; omit and allowlist only `btn_Options_SelectFont` (egui single font, D-004).
   **(Recommended)**
2. Port a font picker over a fixed list of egui fonts (misleads: egui has one font).

**Decided: option 1.**

### Decision 10 — Empty placeholder group `groupBox39` (Task 8)

C#: `OptionsDlg.Designer.cs` — `groupBox39` (title "groupBox39", no children) inside Map's
`groupBox23`.

1. Omit it and allowlist it (empty placeholder, no children, placeholder title). **(Recommended)**
2. Render an empty group titled "groupBox39" (no-op shell).

**Decided: option 1.**

### Owner answers (2026-11-27, binding)

- Decision 3: **option 2 — port the QRZ lookup 1:1, on explicit click only.** The request sends only the call sign to `QRZ_URL_Database` with `as_net::user_agent()`, runs off the UI thread, and never runs automatically. Errors and mismatches go to the same messages as the C#. No deviation entry for QRZ.
- Decision 4 and Decision 6: **write only on outer OK** (recommended options), documented as D-NNN deviations.
- Decision 8: **option 1 — add `image` to root `[workspace.dependencies]` with `default-features = false` and features `["png", "jpeg", "bmp", "tiff"]`, and `image = { workspace = true }` as a direct dependency of `airscout`.** The owner permits **exactly one added line in `Cargo.lock`**: the `"image"` entry under the `airscout` package's dependency list (no new package, no version change). Task 9 must verify that diff and build with `--locked`. (This supersedes the earlier "Cargo.lock must stay unchanged" wording for Task 9's `image` addition only; `Cargo.lock` is otherwise unchanged and `cargo update` stays forbidden.)
- Decisions 1, 2, 5, 7, 9, 10: recommended options approved.

### Rows deliberately left out of this plan

- `btn_Options_MyUpdate`, `btn_Options_DXUpdate` (Stations, "not ported: upload") — Decision 5.
- Every other `missing`/`not ported` row outside the four tabs (Database, the five elevation
  pages, Path, Alarm, Network, Spectrum, Track, CAT, Watchlist, Misc., Info, and
  `btn_Options_Open_PluginDirectory`) belongs to Phase 3B and is not in this plan's scope.

## New deviation entries (to add on implementation)

Each control omission chosen above becomes a `D-NNN` entry in `docs/gui-deviations.md` with an
`Approved:` date and a matching entry in `docs/gui-reference/OptionsDlg.allow.json` (the
`.allow.json` loader, not a TOML file): upload buttons (Decision 5), `groupBox39` (Decision 10),
and the font picker (Decision 9). The two write-timing differences — the station Map result
staged through outer OK (Decision 4) and QRV changes committed only on outer OK (Decision 6) —
also get `D-NNN` entries in `docs/gui-deviations.md`, with no allow entry (no control is omitted).

Also record, with `D-NNN` + allow entries where a control is traced (I8):

- Rust-only Map controls (`map.rs:73-86`): the preloader tile-estimate label (asserted by
  `tests/ui_options.rs:182-208`) and the `"Thunderforest API key"`/`"show"` row — placed after
  the original `groupBox23`/`groupBox22` controls in their nearest container.
- Rust-only Stations controls: the My/DX antenna-height rows (`stations.rs:354-357`), placed
  after the original `groupBox4`/`groupBox5` children (a traced `Extra` with a `D-NNN` `"extra"` allow entry, as Task 3 and Task 10 state; `fidelity.rs:543-552`).
- Runtime placeholder texts (`tsl_Options_Status` `"Status"`, `lbl_Options_LocalObstructions`
  `"Status is unknown."`, `lbl_Options_MyLastUpdated`/`lbl_Options_DXLastUpdated` `"LastUpdated"`)
  and the trimmed `label52` copy (Decision 5) need `"text"` allow entries with `D-NNN` IDs (like
  `D-012` in `MapDlg.allow.json`).

### Plan review fixes (2026-11-27)

Every Critical (C1–C7) and Important (I1–I8) finding from
`.superpowers/sdd/plan-review-phase3a.md` was verified against the code and fixed in place, plus
the cheap, factually confirmed Minor fixes. What changed per finding:

- **C1** — Fixed. Global Constraints and Decision 8 now state the owner's binding allowance of
  exactly one added `Cargo.lock` line (`"image"` in `airscout`'s dependency list); Task 9 Files
  adds root `Cargo.toml`, pins `image 0.25.10` with `default-features = false` and features
  `["png", "jpeg", "bmp", "tiff"]` (verified against `~/.cargo/registry/.../image-0.25.10/Cargo.toml`:
  `png`→`png 0.18.1`, `jpeg`→`zune-core`/`zune-jpeg`, `tiff`→`tiff`, `bmp`→no crate), and Task 9
  Step 3/4 assert the one-line diff and build `--locked`. The "Cargo.lock unchanged" claim is
  removed for Task 9 only. Owner answer added.
- **C2** — Fixed. `.allow.toml` → `.allow.json` everywhere (Scope, Task 1 Files, Task 10 Files,
  New deviations); `fidelity.rs:850-868` loads `<window>.allow.json`, and the JSON freeze applies
  only to extractor-generated `<Window>.json` files.
- **C3** — Fixed. Task 5 adds `btn_Options_PlaneFeed{N}_Default` + `FeedAction::Default(usize)`;
  Task 8 moves `cb_Options_Gauges_Show` from `track.rs` into `groupBox22` and `cb_Options_Map_LabelCalls`
  into `groupBox49`; Task 10 moves the `InfoWin` block from `planes.rs` onto `tab_Options_Map` with
  `planes.rs` in its Files.
- **C4** — Fixed. Task 10 places `label74/75/76` inside `groupBox29/33/34` at the bottom (`y=136`),
  not above them; `cb_Options_InfoWin_Speed` moves into `groupBox33` with a `D-NNN` `"moved"` entry;
  Decision 9 is now unambiguous (keep `label62` + read-only `tb_Options_Map_ToolTipFont`, omit only
  `btn_Options_SelectFont`).
- **C5** — Fixed. Task 5 now states that `Secret` serializes in plaintext (`secret.rs:4-5`,
  `#[serde(transparent)]`), so the exported feed file is written with `write_private_atomic` (0600)
  and credentials are redacted only in errors/status/Debug; the round-trip test selects the same
  feed in slot 2.
- **C6** — Fixed. Task 7 uses `aircraft_database_max_days_lifetime: i64` = 7 (upstream
  `AircraftDatabase_MaxDaysLifetime`, min 1 max 365), adds `background.rs` + `positions.rs` to
  Files, reads the setting in `background.rs:317`, and guards `positions.rs:435` with `if cfg.max_days > 0`.
- **C7** — Fixed. Task 6 drops `web_settings_json.rs` (which would break `tests/web_api.rs`'s exact
  45-key assertion) and adds `feed_service.rs` + `as-feeds/src/{plugin,pipeline,worker}.rs`, plus
  the exact `FeedArgs` fields, the `log_errors` sink and the `PlaneFeed.cs:409-465` 20-column CSV
  trace spec.
- **I1** — Fixed. Task 1 declares the `pub` `load_allow`/`load_deviations` helpers, filters the
  interim `Elevation` tab out of the projected trace before `compare`, and reassigns
  `tab_Options_Planes_Validating` to Task 6.
- **I2** — Fixed. `AreaPreview::show` takes `&Settings` (for `map_provider`/`thunderforest_api_key`);
  `fields.rs` gains `validate_area_bounds(&as_settings::Area)`.
- **I3** — Fixed. Task 3 adds `locator_max_length`/`locator_auto_length` as `GuiSettings` fields
  (updating `web_settings_json.rs:267-268`), fixes `location_restrict_to_area`, passes
  `ElevationEnabled` (not `ElevationModel`), and specifies the C# obstruction texts, the
  `tsl_Options_Status` QRZ message surface, the `dialogs::show` error-retention hook, and the
  antenna-height `Extra` allow entry.
- **I4** — Fixed. Task 4 stamps `QRV_USER_EDIT_MARKER` via `write_options_station_edits` and adds a
  marker-advance test.
- **I5** — Fixed. Task 8 reuses `s.web.map_small_markers` (no duplicate `gui` field), states the
  C# defaults (`map_show_distances`/`map_show_locators` false, `map_show_watchlist`/`map_show_airports`
  true, `gauge_width` 175..=1000, `gauge_forecolor` "Black"), notes `distance_rings`/`locator_grid`
  already draw unconditionally, and adds `map_track.rs` + the `app/*`/`services.rs` airport loader.
- **I6** — Fixed. `OverlayImage::load(path, bounds, ctx)` and `draw(&self, painter, bounds, opacity)`,
  with the `(path, bounds)` reload key and `AppState.status` reporting.
- **I7** — Fixed. `ui_options_4a` → `ui_stations` (Tasks 3/4) and `ui_planes` → `ui_feeds`
  (Tasks 5/6/7); Task 5 notes the `ui_feeds` label updates.
- **I8** — Fixed. New deviations section lists the Rust-only Map controls, the My/DX antenna-height
  rows, and the runtime placeholder/`label52` texts with `D-NNN` + allow entries.
- **M1** — Fixed. Scope no longer claims `gm_Options_Coverage` is an `ok` row in `gaps.md`.
- **M2** — Fixed. Tasks 3/4 use corrected C# ranges and now instruct flipping the Stations rows to `ok`.
- **M3** — Fixed. Task 3 specifies extracting the quoted `cs_lat`/`cs_lon` value (not `IndexOf("\n") - 2`),
  a 1 MiB response cap, and sanitized errors.
- **M4** — Fixed. Task 6 states the `10..=600` range for `tb_Options_Planes_Interval`/`int32TextBox1`.
- **M5** — Fixed. Task 10 uses the `gaps.md:9-11` status syntax and the Summary table.
- **M6** — Fixed. Task 1 Step 1 asserts `small_frame_footer_reachable` at `pixels_per_point = 2.0`.

### Plan re-review fixes (2026-11-27)

Source: `.superpowers/sdd/plan-rereview-phase3a.md` (all C/I/M findings ADDRESSED; five new Important items).

- N1: `FeedArgs` unchanged; `log_errors` via existing `PipelineConfig::log_errors`; `trace_positions` on `WorkerConfig` with its four literal sites listed (Task 6).
- N2: `fields.rs` added to Task 6 Files, with test `planes_alt_order_validated` and an implementation step.
- N3: `OverlayImage::draw` takes `&walkers::Projector` and uses `self.bounds` (Task 9).
- N4: antenna-height rows get a `D-NNN` `extra` allow entry, consistent with Tasks 3 and 10.
- N5: locator constants stay as serde defaults; readers in `web_settings_json.rs` and `stations.rs` switch to the fields; `as-settings/src/lib.rs` added to Task 3 Files.
