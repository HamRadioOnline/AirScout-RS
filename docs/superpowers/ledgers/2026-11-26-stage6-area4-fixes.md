# Stage 6 area 4 implementation ledger

[Implementation plan](../plans/2026-11-26-stage6-area4-fixes.md) · [Design](../specs/2026-11-25-stage6-logic-fixes-design.md) · [Implementation records](../../logic-fixes.md#area-4-implementation-records)

Branch `stage6-area4-gui` from `cca828a`. Signed fix plan: committed with this ledger.

## Task results

| Task and commit subject | RED → GREEN | Gates and golden deltas |
|---|---|---|
| 1. Win-Test wire (#194, #202, #200), `fix(as-wintest): validate datagrams and resolve the requested band` | RED: `decode_bit7_only_corruption_fails_checksum` accepted a byte corrupted only in bit 7; `corrupt_datagram_is_dropped_before_any_handler` let a corrupt datagram reach the handler; `encode_set_azimuth_wraps_359_6_to_000`/`encode_set_elevation_clamps_negative_to_000` encoded 359.6 as `360` and −5 as `-005`; `answer_path_uses_requested_band` resolved the GUI band. GREEN: `decode` compares the raw trailing byte, `dispatch` drops a bad-checksum datagram before any handler, azimuth wraps mod 360 and elevation clamps 0..=90 before `zero3`, and ASSETPATH resolves the whole path on the requested band. | Format and as-wintest/airscout Clippy (`-D warnings`) passed. as-wintest tests: 26 passed, 0 failed, 0 ignored. airscout wintest_service + wintest_nearest tests: 25 passed, 0 failed. No golden change. |
| 2. Track outputs and rotator (#203, #206, #236, #237, #239), `fix(airscout): bound track-output errors and rotator positions` | RED: `successful_step_resets_the_group_error_counter` switched the output off at the 11th total error; `serial_disabled_at_eleventh_format_error` (formerly `...sixth...`) disabled at the 6th; `set_command_wraps_and_clamps` emitted `M360`/`W010 -005`/`M-000`; `serial_io_error_counts_toward_switch_off` never disabled; `azel_dat_is_replaced_atomically` saw the rewrite in place. GREEN: a success resets the group counter, the serial counter increments once (switch-off at the 11th), the GS-232A set command wraps azimuth mod 360 and clamps elevation 0..=90 (non-finite → 0), serial timeouts/I-O errors count once toward switch-off (open failures stay uncounted), and `azel.dat` is written via a same-directory temp file + rename. | Format and as-wintest/airscout Clippy (`-D warnings`) passed. as-wintest tests: 26 passed, 0 failed. airscout full suite: 720 passed, 0 failed. No golden change. |
| 3. Web JSON scrubbing (#257, #277), `fix(airscout): scrub feed credentials from public JSON` | RED: `scrub_public_url_cases` served `http://localhost/a?lat=%MINLAT%&api_key=SECRET-TEST` with the credential still in the query; `query_credentials_never_served` served the VRS `URL` with `key=SECRET-TEST` and `KST_Websocket` with `token=SECRET-TEST`. GREEN: `scrub_public_url` strips userinfo and drops the query except the `%MINLAT%`…`%MAXDISTKM%` placeholders the page substitutes; the VRS `URL`/`URL2` and `KST_Websocket` serve no userinfo or query credential while the placeholders survive. | Format and airscout Clippy (`-D warnings`) passed. airscout full suite: 722 passed, 0 failed. No golden change. |
| 4. Web stations and paths (#262, #263, #264, #265), `fix(airscout): honor the requested band and keep remote lookups read-only` | RED: `propagation_honors_band_param` answered 1.296 GHz for `band=144M` (the settings band); `propagation_bad_band_text` served JSON for `band=7M`; `propagation_does_not_store_locations`/`answer_end_to_end_with_globe_tile`/`set_path_reply_received`/`stopped_api_writes_nothing` stored 2 `Location` rows per request; `best_case_without_tiles_keeps_position` moved the station to (0, 0, 0); `web_elevation_asks_aster_first` skipped ASTER and answered 0 m. GREEN: the requested `band` drives the QRG, F1 clearance and QRV defaults (empty → settings band; invalid/`None`/`All` → `"Error: <band> is not a valid band value!"`); web and ASSETPATH station lookups are read-only (in-memory `FromLoc` centre, `Location` table unchanged); ASTER is in the web lookup order; a best-case shift with no tile leaves the station at its locator position and source. | Format and airscout/as-stations Clippy (`-D warnings`) passed. airscout full suite: 723 passed, 0 failed. as-stations tests: 31 passed. No golden change. |
| 5. Maintenance (#155, #385), `fix(airscout): isolate maintenance deletes from background work` | RED: `options_open_stops_background_and_close_restarts` left the background calculators/updaters/preloader running while Options was open (only the integrations stopped), so a Delete All could be followed by a calculator pass writing paths back; `tables_job_hides_internal_import_tables` listed `StationImportState`/`StationArchiveValidators`/`StationImport` as `[not found]` rows; `clear_internal_table_is_refused` (both the app job and `StationDb`) cleared an internal import table. GREEN: `open_options_dlg` stops the background workers alongside the integrations, and `options_closed` (OK or Cancel, in LIFE) restarts them while HISTORY keeps them stopped; the Tables dialog lists only `Location`, `QRV` and `Table_last_updated`, and `StationDb::clear_table` refuses the three internal import tables. | Format and airscout/as-stations Clippy (`-D warnings`) passed. airscout full suite: 726 passed, 0 failed. as-stations tests: 32 passed. No golden change. |
| 6. Tile generation (#293, #294), `fix(elevation-tile-gen): georeference SRTM tiles and skip all-void tiles` | RED: `srtm_sample_reads_the_cell_south_row` read the cell's north-edge row (`frows - 2 - …`); `srtm_void_tile_after_data_tile_is_not_written` wrote an all-void tile after a data tile (`MinElv` 32767, `MaxElv` −32768); the N50E011 integration tests (`cli_writes_srtm3_tree`, `round_trip_through_as_elevation`, `existing_tiles_kept_unless_overwrite`, `directory_argument_expands_sorted`, the `zip_*`/`catalogue_*`/`self_hosted_tree_feeds_the_updater` pack tests and `preview_places_tiles`) expected the old 287-written/1-void layout. GREEN: the SRTM sample row is `frows - 1 - j*rows - l` (GLOBE-georeferenced, #294) and `all_void` is decided per tile via `any_data` (#293); N50E011 writes 288 tiles / 0 void (JO50MA now written — its bottom row holds data — and JO50MB cell (0, 0) reads the void band); both synthetic tests hand-derive their expected samples from the fill formula, including the `frows - 1` cell edge case. | Format and elevation-tile-gen Clippy (`-D warnings`) passed. elevation-tile-gen tests: 38 passed, 0 failed. as-golden tests: 1 passed. Golden exception: the `elevation_tilegen.json` `srtm_tile` group skips C# equality (one-row shift changes 863/864 opaque-hash cases; owner-approved 2026-11-26, recorded in `docs/logic-fixes-area4/README.md`). `tests/golden/*.json`, `Cargo.lock` and `upstream_commit` unchanged. |
| 7. CAT rig and OmniRig (#221, #223, #246, #249), `fix(as-cat): restore split and correct OmniRig capability checks` | RED: `leave_restores_split_when_entered_in_split` left a split-entered rig in simplex (`set_simplex_mode` switched split back off); `every_active_doppler_strategy_needs_split` and `every_active_doppler_strategy_needs_split_and_vfoequal` reported `Online` for strategy B (split checked only under A); `set_frequency_writes_when_writeable` never wrote (the `call_missing` `IsWritable` seam always errored); `status_mapping` mapped an undefined status (7) to `Online`. GREEN: `leave` restores a saved `SplitOn` with `set_split_mode(rx, tx, mode)` and only uses `set_simplex_mode` otherwise; `get_status` checks split suitability for every `is_active()` strategy in both `CatRig` and `OmniRig`; `OmniRig` frequency writes use `is_writeable` (removing the dead `call_missing` seam) and an unknown OmniRig status maps to `Offline`. | Format and as-cat Clippy (`-D warnings`) passed. as-cat tests: 78 passed, 0 failed. No golden change. |
| 8. Best-case station shifts (#94, #105), `fix(airscout): keep void tiles and shifts from corrupting station position` | RED: `calculator_skips_a_void_tile_but_shifts_on_a_real_maximum` shifted a stored all-void tile (`max_elv == ELV_MISSING`) to its stored `(0, 0)`; `compute_finds_the_obstruction_at_the_unshifted_position` looked the obstruction up at the best-case moved position and found none (`local_obstruction == f64::MIN`). GREEN: `calculator_shift` filters on `max_elv != ELV_MISSING`, so a void tile keeps the station at its locator position; the path service, calculator, history, ASSETPATH and web routes all look the local obstruction up at the unshifted settings position against the unshifted bearing. | Format and airscout Clippy (`-D warnings`) passed. airscout full suite: 727 passed, 0 failed. No golden change. |
| 9. Watchlist and history exports (#103, #141, #142), `fix(airscout): bound the watchlist and per-aircraft export speeds` | RED: `watchlist_insert_capped_and_sort` refused the DX station when the list was full and appended instead of inserting at the front; `full_watchlist_inserts_dx_at_front_and_drops_oldest` did not add the DX station to a full list; `plane_history_same_second_writes_zero_speed` wrote `Infinity`/`NaN` for a zero hour difference; `plane_history_by_call_mixes_hexes` and `plane_history_by_call_differences_within_a_hex` diffed across the different hexes of one call. GREEN: `insert_capped` inserts at index 0 and truncates to `Watchlist_MaxCount` (newest-first, oldest dropped); `write_plane_csv` differences each row against the previous row of the same hex and writes `av_speed = 0` for a non-positive hour difference. | Format and as-settings/airscout Clippy (`-D warnings`) passed. as-settings tests: 66 passed, 0 failed. airscout full suite: 728 passed, 0 failed. No golden change. |
| 10. Audit, full gates and ledger, `docs: stage 6 area 4 fix ledger` | Audit (not a fix). Verified all 60 area-4 rows: the 27 `fix` rows each carry **Fixed in stage 6** with an area-4 implementation record in `docs/logic-fixes.md` (C# location at fbadef0f, the fix in C# terms, a proving test), and all 39 recorded proving-test names grep to existing `#[test]` functions (0 missing). Added the missing **Kept** + approval-list pointer to the 33 `keep` rows, and corrected the stale `#104` ("to be fixed in stage 6") and `#386` ("Open … fix row") endings that contradicted their keep decisions. `git diff --stat cca828a..HEAD -- tests/golden Cargo.lock upstream_commit` is empty. No settings/DB schema change and no computed cache key change, so no migration and no algorithm-version bump. Golden handling: the single `elevation_tilegen.json:srtm_tile` exception is recorded in `docs/logic-fixes-area4/README.md` (owner-approved 2026-11-26) and `golden-deltas.tsv` holds no rows. | Full macOS gates green (table below). |

## Full gates (task 10)

Every command prefixed `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --all --check` | clean |
| Workspace Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean, no warnings |
| Workspace tests | `cargo test --workspace --locked --no-fail-fast` | 1551 passed, 0 failed, 5 ignored |
| Hamlib Clippy | `cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings` | clean, no warnings |
| Featureless as-cat Clippy | `cargo clippy -p as-cat --all-targets --locked -- -D warnings` | clean, no warnings |

## Blocked rows

None.

## Recovery

Completed tasks 1–10. Fix commits: task 1 `8b16eaf`, 2 `2c3f1a9`, 3 `f05dfed` (the earlier `5ed0804` was a stale hash and is corrected here), 4 `6730f03`, 5 `2280048`, 6 `110d699`, 7 `3fa9ef7`, 8 `e14bd9c`, 9 `b79f546`. Task 10 is this ledger commit (`docs: stage 6 area 4 fix ledger`). No blocked rows.

## Final review (Gemini + DeepSeek, one round)

The whole-branch final review — Gemini **Changes requested** (0 Critical, 4 Important, 4 Minor), DeepSeek **Approve** (0 Critical, 0 Important, 2 Minor) — is closed in one fix wave. Every claim was verified against the code first; none was false, so none was skipped. Each finding is fixed and proved by a named test:

| Finding | Outcome | Proving test |
|---|---|---|
| G1 (#257/#277): query credentials survive on malformed/non-HTTP URLs with `@` in the query | fixed — the query is dropped before any userinfo/`@` handling | `scrub_public_url_drops_query_before_userinfo` |
| G2 (#257/#277): `is_placeholder_only` accepts separator-only values; `KST_Websocket` keeps VRS placeholders | fixed — a separator-only value is not a placeholder; `KST_Websocket` drops its whole query | `scrub_public_url_separator_only_value_is_dropped`, `kst_websocket_drops_its_entire_query` |
| G3 (#257/#277): path-token warning missing from docs and UI | fixed — warning added to Options → Network, the VRS feed dialog, `docs/network.md` and `README.md` | docs/UI text only |
| G4 (#155): detach-only stop; Options can close while a maintenance job runs | fixed — `BackgroundService::stop` waits (bounded) for workers; a delete/clear is refused while workers run (`background_jobs() > 0`); OK/Cancel stay disabled while a destructive job runs | `stop_waits_for_running_workers`, `delete_all_paths_after_cancel_leaves_nothing`, `maintenance_refused_while_background_workers_run`, `options_ok_cancel_disabled_while_destructive_job_runs` |
| G5 (#200): stale GUI-band check drops valid ASSETPATH requests | fixed — only the requested band is gated | `undefined_band_writes_nothing` (extended) |
| G6 (#202/#236): `whole_degrees`/`wrap_azimuth` emits `NaN` for `f64::MAX` | fixed — non-finite or overflow input becomes `0.0` | `whole_degrees_rejects_out_of_range` |
| G7 (#103): `insert_capped` returns `true` at cap 0 | fixed — cap 0 clears the list and returns `false` | `watchlist_insert_capped_zero_keeps_nothing` |
| G8: stale doc comments, ledger count, weak assertions | fixed — `best_case.rs`/`track_outputs.rs` doc comments corrected; ledger count 38→39; the #105 obstacle is placed at a single azimuth sector; the #221 split test asserts restored RX/TX | `compute_finds_the_obstruction_at_the_unshifted_position` (tightened), `leave_restores_split_when_entered_in_split` (tightened) |
| D-minor1 (#257): placeholder match is case-sensitive | fixed — placeholder names are matched case-insensitively | `scrub_public_url_lower_case_placeholders_kept` |
| D-minor2 (#265): `/propagationpath.json` `source` flips to `FromBest` | fixed — recorded as intended in the #265 record; no code change | docs only |

The 27 fix rows now record 50 proving-test names (39 before this wave + 11 added), all of which grep to existing `#[test]` functions. `git diff --stat cca828a..HEAD -- tests/golden Cargo.lock upstream_commit` is empty.

## Final-review gates

Every command prefixed `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --all --check` | clean |
| Workspace Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean, no warnings |
| Workspace tests | `cargo test --workspace --locked --no-fail-fast` | 1561 passed, 0 failed, 5 ignored |
| Hamlib Clippy | `cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings` | clean, no warnings |

## Re-review fix wave 2 (Gemini re-review of the final-review fix wave)

A scoped re-review (`a4-rereview-gemini.md`, range `a3a9ee9..c0ec794`) found that the final-review fix wave itself introduced regressions: two new credential leaks in `scrub_public_url` (`?` in a password, `#` before `?`), and a `#155` fix that froze the egui UI thread for up to 5 s, overwrote the running-worker counter on restart, silently failed read-only `Sizes` on Options open, and whose proving test passed unchanged on the pre-fix commit. Five important findings (I1–I5) plus one minor (Minor 1) are closed here:

| Finding | Outcome | Proving test |
|---|---|---|
| I1 (#257/#277): `scrub_public_url` leaks userinfo on `?` in a password and query credentials on `#` before `?` | fixed — parse with the `url` crate and fail closed (empty string) on a non-absolute URL, more than one `@`, or an ambiguous `?`/`#`/numeric-port credential boundary; clear userinfo + fragment, keep only placeholder-only query params | `scrub_public_url_fails_closed_on_ambiguous_credentials`, `scrub_public_url_serves_the_placeholder_literally`, `scrub_public_url_clears_percent_encoded_userinfo_and_keeps_ipv6` |
| I2 (#155): `BackgroundService::stop` blocks the egui UI thread up to 5 s | fixed — `stop` is signal-only (set cancel, return at once); the counter only decreases when a worker actually exits | `stop_is_signal_only` |
| I3 (#155): `start_background` overwrites the running-worker counter, losing a timed-out generation | fixed — one shared `Arc<AtomicUsize>` on `RealServices` passed into every `BackgroundService::start`, so `background_jobs()` is cumulative across generations | `background_jobs_is_cumulative_across_generations` |
| I4 (#155): `request_sizes()` fails silently and the refusal message is never shown | fixed — read-only `Sizes` is not gated on `background_jobs() > 0` (it runs anyway), and the Database tab shows "waiting for N background workers to stop" while destructive buttons are disabled | `sizes_read_runs_while_background_workers_stop` |
| I5 (#155): `delete_all_paths_after_cancel_leaves_nothing` never exercised `BackgroundService::stop` (passed on `a3a9ee9`) | fixed — the test drives `RealServices` start/stop/`background_jobs` with channel-based (not fixed-sleep) coordination | `delete_all_paths_after_cancel_leaves_nothing` (rewritten) |
| Minor 1 (#155): stale doc comments on `stop`/`stop_background` | fixed — module doc, trait doc and `BackgroundService::stop` all agree stop is signal-only | docs only |

The three touched implementation records (`#155`, `#257`, `#277` in `docs/logic-fixes.md`) were rewritten to describe the corrected implementation. `git diff --stat cca828a..HEAD -- tests/golden Cargo.lock upstream_commit` remains empty.

## Re-review fix wave 2 gates

Every command prefixed `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --all --check` | clean |
| Workspace Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean, no warnings |
| Workspace tests | `cargo test --workspace --locked --no-fail-fast` | 1566 passed, 0 failed, 5 ignored |
| Hamlib Clippy | `cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings` | clean, no warnings |

## Re-review 2 fixes (controller)

Gemini 4 Neon re-review 2 of `c0ec794..758f27d`: 0 Critical, 3 Important, 1 Minor.

| Finding | Disposition | Test |
|---|---|---|
| I1 (#257/#277): the `@`-outside-userinfo guard required a numeric port, so `http://SECRET-USER:80/SECRET-PASS@host/x` served `http://secret-user/SECRET-PASS@host/x` | fixed — any `@` outside the parsed userinfo fails closed, whatever the port; a benign `@` in a query is no longer served either (fail closed by design) | `scrub_public_url_fails_closed_on_ambiguous_credentials` (default, empty and no port, `\`, query `@`) RED→GREEN; `scrub_public_url_drops_query_before_userinfo` updated |
| I2 (#155): the three spawn helpers decremented the count on a failed spawn although the dropped closure's `RunningGuard` already does | fixed — the explicit decrement is removed; `RunningGuard` is the only decrement | Ruling: no regression test — a failed `std::thread::Builder::spawn` cannot be injected without a test seam; the fix is a deletion verified by reading (the closure owning the guard is dropped on `Err`). Cost if wrong: none, the remaining path is the guard drop. |
| I3 (#155): `delete_all_paths_after_cancel_leaves_nothing` stops only after the calculator finished | rescoped — doc comment now states it is an end-to-end drain/delete check and that the race is pinned by `stop_is_signal_only`, `background_jobs_is_cumulative_across_generations`, `maintenance_refused_while_background_workers_run` | Ruling: the rendezvous unit tests already hold a worker mid-flight; a second mid-write integration test needs a calculator test seam. Cost if wrong: an integration-only regression in the stop wiring would be caught by the unit tests' `RealServices` paths, not this test. |
| Minor: "waiting for 1 background workers" | fixed — singular for one worker | wording only |

Gates after the fixes: `cargo fmt --all --check` clean; `cargo clippy -p airscout --all-targets --locked -- -D warnings` clean; `cargo test -p airscout --locked --no-fail-fast` 741 passed, 0 failed.

## Close-out

| Gate | Tree | Result |
|---|---|---|
| macOS fmt, workspace Clippy, workspace tests, hamlib Clippy | `758f27d` | Pass; 1,566 passed, 0 failed, 5 ignored |
| macOS fmt, airscout Clippy and tests, hamlib Clippy | `5624ae7` | Pass; airscout 741 passed, 0 failed |
| Linux Docker (`clippy`, `test --no-fail-fast`, hamlib pair) | `5624ae7` | LINUX_OK; 1,566 passed, 0 failed, 5 ignored |
| Windows mingw | `5624ae7` | WINGNU_OK |

- Implementation: DeepSeek V4 Pro (list revisions, plan, tasks 1–10, fix waves 1–2); controller (re-review 2 fixes `5624ae7`).
- Owner decisions: #155 A, #194 A, #202 A, #203 A, #239 A, #257 B, #264 A, #265 A, #277 B, #294 A, #386 B; #294 golden handling option 2 (documented `srtm_tile` exception, `docs/logic-fixes-area4/README.md`).
- Final review: DeepSeek V4 Pro approved (0 Important, 2 Minor, both fixed); Gemini 4 Neon (blocking) requested changes (4 Important), fixed in wave 1 (`646566a`..`c0ec794`); re-review 1 (5 Important) fixed in wave 2 (`f7cc984`..`758f27d`); re-review 2 (3 Important, 1 Minor) fixed in `5624ae7`; re-review 3 approved (0 findings).
- `git diff --stat cca828a..HEAD -- tests/golden Cargo.lock upstream_commit`: empty.
- Deferred: none open from the reviews. Known trade-off: a benign `@` outside the userinfo of a feed URL (e.g. in a query) is served as an empty string (fail closed).
- Merged to `main` by fast-forward and pushed (owner authorization for per-area merges, 2026-11-25).
