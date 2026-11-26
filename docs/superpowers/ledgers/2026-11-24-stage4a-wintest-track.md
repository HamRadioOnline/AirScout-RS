# SDD ledger — plan: docs/superpowers/plans/2026-11-24-stage4a-wintest-track.md

Branch stage4a-wintest-track, BASE of branch: 211794d (main dfb5f19 + plan commit). Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md. Survey decisions: docs/superpowers/surveys/2026-11-24-integrations-survey.md.
Disk rule: cargo env prefix; cargo clean --profile dev before each dispatch when no agent builds.


## Pre-flight scan

Checked against code at BASE 211794d. Golden bytes of Task 1 recomputed (checksums 0xB6, 0xB9, 0xBA, 0x98, 0xEA, 0xDC, 0xB0, 0x85 and lengths 52/59 all correct). Findings table last row is #193 (ok). Confirmed existing: `Paths`/`my_station`, `ground_elevation`, `local_obstruction`, `ui_position`, `get_nearest_planes`, `PlaneInfoCache::get_all`, `find_or_create_path`, `max_elv_loc`, `Band::from_i32`, `callsign::{check,cut}` (as_stations), `maidenhead::check`, `Watchlist::{index_of_loc,insert_capped}` and `WatchlistItem.remove`, `TrackValues`, `valid`, `TrackStatus::Error`, `ServiceEvent::{Status,Error}`, `AppState::{play,pause,is_playing,path_mode,cache,feed_gen,alarm_focus,control_tab}`.

### Pair rows (shared file or interface)

| Tasks | Produces vs consumes | Found |
|---|---|---|
| 1 -> 3 | `encode`, `track::{set_azimuth,set_elevation,set_azel}`, `dotnet::fixed` | Signatures match. Task 3 adds the `as-wintest` dependency to `airscout/Cargo.toml` but has no non-`--locked` build step and does not list `Cargo.lock` (CONFLICT 1). |
| 1 -> 4 | `encode`, `as_nearest` | Match. |
| 1 -> 5 | `decode`, `Decoded`, `DecodeError` | Match. `Decoded` and `ShowPath`/`WatchEntry`/`RemoveWatch` have no derives in the plan text while tests compare them (CONFLICT 8). |
| 2 -> 3 | `TrackUdp`, `TrackFile`, `Track.udp*` | Match. |
| 2 -> 5 | `Server`, `watchlist_sync_with_kst` | `WinTestConfig::from_settings` text omits `sync_with_kst` (CONFLICT 9). |
| 2 -> 8 | `Server`, `TrackUdp`, `TrackFile`, sync flag | Match. |
| 3 -> 4 | `NetAddrs` | Task 4 lists it as consumed but `send_reply` takes `bind_ip`/`to`; unused. No defect. |
| 3 -> 5 | `NetAddrs`, `ServiceEvent` edit in `services.rs` and `app/mod.rs` | Both tasks add an arm and a variant; arms are placeholders replaced in 6 and 7. Consistent. |
| 3 -> 6 | `TrackService`, `TrackConfig`, `TrackStep`, `RotStatus`, `rot_colors`, `OutputGroup` | Match. `TrackService::stop(self)` consumes the service, so Task 3 test `service_stop_ends_output` cannot send a step after stop (CONFLICT 2). `TrackService::step`: "full channel drops" needs a bounded channel; plan says mpsc (CONFLICT 10). |
| 3 <-> 6 | `tracking.rs` | Task 3 adds `rot_colors` with "same constants as `status_colors`"; those constants are local to `status_colors`, so they would be duplicated (CONFLICT 11). |
| 4 -> 5 | `NearestSettings`, `parse_set_path`, `answer`, `send_reply`, `location_find_or_create` | Match. `n.band_settings[&band]` indexing can panic when a loaded settings map lacks a band (CONFLICT 12). |
| 4 -> 6 | `NearestSettings::from_settings` | Match. |
| 4 tests | `tests/common/mod.rs`, `horizon.rs`, `services.rs` | `data_dir_with_globe` exists in `horizon.rs` AND `services.rs`; plan moves only the first, leaving a duplicate (CONFLICT 7). |
| 5 -> 6 | `WinTestService::start`, `WinTestCtx`, `WinTestConfig` | `start` "binds at once" on the caller. `RealServices::start_integrations` runs on the UI thread (CONFLICT 4). |
| 5 -> 7 | `WinTestCommand`, `ShowPath`, `WatchEntry`, `RemoveWatch`, `ServiceEvent::WinTest` | Match, except derives (CONFLICT 8). |
| 6 -> 7 | `integration_gen`, `AppState` | `integration_gen` and `nearest_synced` are not `pub`; Tasks 6 and 7 integration tests read/build events with `integration_gen` (CONFLICT 5). |
| 6 <-> 7 | `app/mod.rs` | Task 6 arm and fields, then Task 7 arm: line growth cap "about 40" is shared; Task 6 reports <= 773, Task 7 adds more with no new count. Minor. |
| 6 <-> 8 | `ui/options/track.rs` | Both edit module docs / the 3c "stage 4" label (Task 6 docs, Task 8 label removal). Overlap, no contradiction. |
| 7 -> existing | `sync_loc_texts` | Private `fn` in `app/dialogs.rs`; `app/wintest.rs` is a sibling module and cannot call it (CONFLICT 6). |
| 7 <-> 8 | `manage_enabled` vs "Clear watchlist" | Manage uses `sync && server.activate`; Clear uses only "sync checked" (CONFLICT 13). |
| 6 <-> 9, 7 <-> 9 | findings 14, 16, 17 | Item 14 is an update of #181/#177, not a new finding, but gets a number in "assigned in list order" (CONFLICT 14). |

### Per-task self-consistency

| Task | Tests vs code, files created vs touched | Result |
|---|---|---|
| 1 | Goldens ok. `decode_errors` expects `b""` -> `Empty` but the algorithm checks `NoNameSeparator` first; order unspecified (CONFLICT 8). | minor |
| 2 | Ok. `track.rs` module doc update listed. | clean |
| 3 | `service_stop_ends_output` unreachable (CONFLICT 2). `Cargo.lock` missing (CONFLICT 1). | conflicts |
| 4 | `answer_end_to_end` asserts QRV rows exist, but `qrv_find_or_create_default` does NOT store ("not stored", db.rs:290) (CONFLICT 3). `undefined_band_writes_nothing` only calls `parse_set_path`; asserts nothing about the worker (CONFLICT 3). | conflicts |
| 5 | `bind_failure...` binds `192.0.2.1` against the loopback-only test rule (CONFLICT 4b). `two_receivers_share_a_port` asserts only `local_addr().is_some()`. Bind on UI thread (CONFLICT 4). | conflicts |
| 6 | Tests read `integration_gen` (CONFLICT 5). Heights-not-ready step in TRACK is unspecified (steps `values: None`?). | conflict |
| 7 | `sync_loc_texts` visibility (CONFLICT 6). | conflict |
| 8 | Ok; `ok_allowed_with_warning`, groups, no-serial tests fine. | clean |
| 9 | Item 14 numbering (CONFLICT 14). 21 items = #194..#214. | minor |

### Conflicts

CONFLICT 1: Task 3 Files/Step 3 — adds dependency `as-wintest` to `crates/airscout/Cargo.toml` but lists no `Cargo.lock` and no non-`--locked` build, so `--locked` clippy/test fails — add "run one `cargo build -p airscout` without `--locked`, commit `Cargo.lock`" to Task 3.
CONFLICT 2: Task 3 `fn stop(self)` vs test `service_stop_ends_output` — the test must send a step after `stop()`, impossible once the service is consumed — make `stop(&mut self)` (or `&self`) or have the test hold a cloned sender.
CONFLICT 3: Task 4 tests `answer_end_to_end_with_globe_tile`, `undefined_band_writes_nothing` — QRV rows "exist" after `answer`, but `qrv_find_or_create_default` never stores (as-stations db.rs:290, C# also); the second test asserts nothing a no-op could fail — drop the QRV-row assertion (keep both locations), and make `undefined_band_writes_nothing` run `dispatch`/`answer` path on a bad band and then check counts.
CONFLICT 4: Task 5 `WinTestService::start` "binds at once" (socket create + bind) is called from `RealServices::start_integrations` on the UI thread — violates Global "No socket I/O on the UI thread" — bind inside the thread and publish `local_addr` via `Arc<Mutex<Option<SocketAddr>>>` (tests wait for it), or add an explicit ruling that one non-blocking bind is allowed.
CONFLICT 4b: Task 5 test `bind_failure_reports_error_once_and_retries` binds `192.0.2.1` — contradicts Global Tests "bind 127.0.0.1 only through `NetAddrs::loopback()`" — add a Global-Constraint exception sentence for this one test (nothing is sent) or bind a taken `127.0.0.1` port without reuse.
CONFLICT 5: Task 6 `AppState.integration_gen` and `nearest_synced` have no `pub`; `tests/integrations.rs` and `tests/wintest_app.rs` (external crates) read/compare `integration_gen` — declare them `pub`.
CONFLICT 6: Task 7 `app/wintest.rs` calls `sync_loc_texts`, which is a private fn in `app/dialogs.rs` (dialogs.rs:294) — make it `pub(super)` and list `app/dialogs.rs` under Task 7 Files.
CONFLICT 7: Task 4 Files — moves `data_dir_with_globe` from `tests/horizon.rs` to `tests/common/mod.rs`, but an identical copy is in `tests/services.rs:18` — move both (services.rs uses `mod common;` too), or reviewer sees duplicated logic.
CONFLICT 8: Task 1 interfaces — `Decoded`, and Task 5 `ShowPath`/`WatchEntry`/`RemoveWatch` have no derives, but `WinTestCommand` derives `Clone, Debug, PartialEq` (`ServiceEvent` is `Debug, Clone`) and tests compare them; also decode order for `b""` (`Empty` vs `NoNameSeparator`) is unstated — add `#[derive(Clone, Debug, PartialEq, Eq)]` to those and "empty input is checked first".
CONFLICT 9: Task 5 `WinTestConfig::from_settings` text lists bind, reply_to, reply_bind_ip but not `server_name` / `sync_with_kst` — add `server_name = s.server.name.clone()`, `sync_with_kst = s.watchlist_sync_with_kst`.
CONFLICT 10: Task 3 `TrackService::step` "full or closed channel drops the step" — `mpsc::channel` is unbounded and never full — use `sync_channel(8)` + `try_send`.
CONFLICT 11: Task 3 `rot_colors` "same constants as `status_colors`" — they are local consts of `status_colors` (tracking.rs:145), so a copy duplicates them — hoist `DARK_GRAY`/`CONTROL` to module consts used by both.
CONFLICT 12: Task 4 `n.band_settings[&r.band]` / `[&n.band]` — BTreeMap indexing panics if a loaded settings file lacks a band, on network-triggered input — use `.get()` and return `Err`/silent drop.
CONFLICT 13: Task 8 "Clear watchlist disabled while the sync is checked" vs Task 7 `manage_enabled` (`sync && server.activate`) — with server off and sync still checked, Manage is enabled and Clear is disabled — use `manage_enabled(s)` for both (verify against OptionsDlg.cs:3171).
CONFLICT 14: Task 9 Step 2 item 14 is an update of #181/#177, yet "numbers in list order" gives it a new number and README says "#194 to #N" — mark item 14 as an update without a number (items then #194..#213) and say so.

## Pre-flight rulings
- Ruling: C1 Task 3 adds a non-locked `cargo build -p airscout` and commits Cargo.lock — gate runs --locked — cost if wrong: none.
- Ruling: C2 `TrackService::stop(&mut self)` (idempotent) — test must step after stop — cost: none.
- Ruling: C3 drop the QRV-row assertion; `undefined_band_writes_nothing` runs the full answer path with an undefined band and checks DB counts unchanged — cost: none.
- Ruling: C4 `WinTestService::start` spawns the thread and binds inside it; `local_addr()` reads a shared `Arc<Mutex<Option<SocketAddr>>>` set after bind; tests wait (bounded) for it — global rule no socket I/O on UI thread outranks plan — cost: tests poll briefly.
- Ruling: C4b keep binding 192.0.2.1 (TEST-NET, not local: bind fails with EADDRNOTAVAIL, nothing is sent) as a named exception to "bind 127.0.0.1 only" — reuse flags make a taken loopback port unreliable for forcing failure — cost: a platform where binding a foreign address succeeds would make the test flaky.
- Ruling: C5 `integration_gen` and `nearest_synced` are `pub` — cost: none.
- Ruling: C6 `sync_loc_texts` becomes `pub(super)`, dialogs.rs joins Task 7 files — cost: none.
- Ruling: C7 both `data_dir_with_globe` copies move to tests/common — cost: none.
- Ruling: C8 `Decoded`, `ShowPath`, `WatchEntry`, `RemoveWatch` derive `Clone, Debug, PartialEq`; `decode` checks empty input first — cost: none.
- Ruling: C9 `WinTestConfig::from_settings` carries `server.name` and `watchlist_sync_with_kst` — cost: none.
- Ruling: C10 TrackService uses `sync_channel(8)` + `try_send` — cost: none.
- Ruling: C11 hoist `DARK_GRAY`/`CONTROL` to module consts shared by `status_colors` and `rot_colors` — cost: none.
- Ruling: C12 band lookup uses `.get()`; an unknown band drops the request silently (no panic on network input) — cost: none.
- Ruling: C13 Manage and Clear-watchlist enablement follow OptionsDlg.cs:3171 exactly, implementer verifies; both use one predicate — cost: one button state.
- Ruling: C14 Task 9 item 14 updates #181/#177 without a number; new findings #194..#213 — cost: none.
Task 1: dispatched implementer (sonnet), BASE 211794d
Task 1: implementer DONE_WITH_CONCERNS 9cc2fda (no RED run recorded; 24/24); reviewer dispatched (sonnet)
Task 1: review approved; ⚠️ golden checksums verified by controller run (cargo test -p as-wintest, all pass); -000 behaviour pinned by plan ruling.
Task 1: minor (deferred): `-0.0` literal prints `-0.0`/`-000`; .NET Framework may print `0` — log as edge in the Task 9 finding.
Task 1: minor (deferred): WtKind::parse maps undefined numbers/comma lists to Unknown where Enum.Parse casts/ORs — ensure the Task 9 finding covers it.
Task 1: complete (commits 211794d..9cc2fda, review clean)
Task 2: dispatched implementer (sonnet), BASE 9cc2fda
Task 2: implementer DONE 7070fdf (RED recorded, 5/5, gate green); reviewer dispatched (sonnet)
Task 2: review approved; ⚠️ older-reader compat not required by brief (no action).
Task 2: complete (commits 9cc2fda..7070fdf, review clean)
Task 3: dispatched implementer (sonnet), BASE 7070fdf
Task 3: implementer DONE bef7f35 (RED recorded, 16 new tests, gate green); reviewer dispatched (opus: concurrency)
Task 3: review approved (opus). ⚠️ items routed to Task 6 dispatch: validity check before Some(values) (MapDlg.cs:8133-8136), Disabled(group) persists into settings, ROT reset to None on stop/thread exit.
Task 3: minor (deferred): connect error text discarded (`let _ = slot.out.connect()`); OS reason of a bind failure never shown.
Task 3: minor (deferred): "Tracking via file disabled." unreachable and not in C#; casing vs C# DDE/UDP/Serial texts; log as port note in Task 9.
Task 3: minor (deferred): per-output deviation emits Rot(Error) then Rot(Tracking) in the same step when UDP switches off and File follows; log with the per-output finding in Task 9.
Task 3: minor (deferred): thread spawn failure sends no Text/Rot(Error).
Task 3: minor (deferred): track_outputs tests use Ipv4Addr::LOCALHOST directly instead of NetAddrs::loopback().
Task 3: complete (commits 7070fdf..bef7f35, review clean)
Task 4: dispatched implementer (sonnet), BASE bef7f35
Task 4: implementer DONE b58ce5a (RED compile-time, 8 new tests, gate green); reviewer dispatched (sonnet)
Task 4: review approved (sonnet). ⚠️ C# check order of parse_set_path and helper bounds not re-read — left to final review.
Task 4: minor (deferred): undefined_band_writes_nothing second half may duplicate the first when the GUI default band is B144M (GUI-band-missing case possibly uncovered).
Task 4: minor (deferred): answer_path_uses_gui_band lacks f1/antenna-height asserts; best_case branch and cancel branch untested.
Task 4: minor (deferred): each valid request may insert up to 2 Location rows (1:1 upstream) — list in the Task 9 finding with the L4 band split.
Task 4: complete (commits bef7f35..b58ce5a, review clean)
Task 5: dispatched implementer (sonnet), BASE b58ce5a
Task 5: implementer DONE_WITH_CONCERNS e787ecd (Windows cfg path not type-checked locally; watchlist rows written before cap; 15 tests, gate 768/0); reviewer dispatched (opus)
Task 5: review (opus) Needs fixes — Important #1: watchlist cap must sit inside prepare_watchlist/prepare_add_watch loops (DB rows per datagram unbounded). Fix round 1 sent to implementer.
Task 5: Ruling: bind-failure test does not prove the 5 s retry (plan-mandated 2 s window) — keep the plan's test, add retry/Status coverage only if cheap later; deferred — cost if wrong: a broken retry would show only in manual use.
Task 5: minor (deferred): failed thread spawn sends no ServiceEvent::Error.
Task 5: minor (deferred): macOS 9 000-byte datagram fallback in large-datagram test (OS limit; Linux/Windows exercise 65 000).
Task 5: minor (deferred): DB/decode errors dropped silently (brief-named deviation; Task 9 finding).
Task 5: note for Task 7: RemoveWatch::Calls uncapped (~32k strings) — Task 7 must match via HashSet/dedupe, not O(n×m) on the UI thread; cap merged watchlist.
Task 5: fix round 1/5 (1 addressed, 0 open — watchlist cap inside loops; commits e787ecd..29300e5)
Task 5: complete (commits b58ce5a..29300e5, review clean)
Task 6: dispatched implementer (opus: 9-file integration), BASE 29300e5
Task 6: implementer DONE 878097c (RED x2, 16 new tests, gate 785/0); reviewer dispatched (opus)
Task 6: review approved (opus). ⚠️ #181 doc rewrite owned by Task 9 item 14; wire formats owned by Task 3 (done).
Task 6: minor (deferred): TRACK before heights arrive sends values None, ROT STOPPED for one round trip after Play (async-heights deviation from 3c reaching outputs) — Task 9 finding.
Task 6: Ruling: integrations.rs:1279 `assert_ne!(a2, a1)` can fail if the OS reuses the freed ephemeral port — fix it in Task 9 (drop the inequality, keep rebind + free check) because a flaky CI test costs every later push — cost if wrong: none.
Task 6: minor (deferred): integrations.rs:1045 weak `assert_ne!(st.status, "old")`.
Task 6: minor (deferred): status_bar_shows_rot_after_trk checks position, not colour.
Task 6: minor (deferred): sync_integrations pushes NearestSettings once before any start (harmless).
Task 6: minor (deferred): a Disabled event queued when Options opens is dropped by the generation bump, so that disable is not persisted.
Task 6: complete (commits 29300e5..878097c, review clean)
Task 7: dispatched implementer (sonnet), BASE 878097c
Task 7: implementer DONE_WITH_CONCERNS 3e955e1 (19 tests, gate green, RED compile-time)
Task 7: Ruling: C13 revised — two predicates, each 1:1 with its C# site: `manage_enabled` (MapDlg.cs:3902/:6049, `!Sync || !Server_Activate`) for the Multi-tab Manage button, `options_watchlist_buttons_enabled` (OptionsDlg.cs:3171-3172, `!SyncWithKST`) for the Options Manage/Clear buttons — port policy 1:1 outranks my pre-flight guess — cost if wrong: one button state.
Task 7: reviewer dispatched (sonnet)
Task 7: review (sonnet) Needs fixes — Important #1: sync_watchlist cap counts stale items before retain, full list + new KST set ends empty. ADDWATCH play() 1:1 confirmed. Fix round 1 sent.
Task 7: ⚠️ options_watchlist_buttons_enabled not yet wired — Task 8 dispatch must wire it.
Task 7: minor (deferred): remove_watch_calls_many_is_fast asserts < 1 s wall clock (CI flake risk).
Task 7: minor (deferred): no app-level test drives Watchlist through on_wintest; watchlist_dirty under MULTI unasserted.
Task 7: minor (deferred): app/wintest.rs:117-120 reads my/max_km/max for every command.
Task 7: fix round 1/5 (1 addressed, 0 open — sync cap counts kept items; commits 3e955e1..9768bfb)
Task 7: complete (commits 878097c..9768bfb, review clean)
Task 8: dispatched implementer (sonnet), BASE 9768bfb
Task 8: implementer DONE e8b1700 (RED compile, 10 tests, gate green); reviewer dispatched (sonnet)
Task 8: review approved (sonnet). ⚠️ predicate !SyncWithKST already confirmed 1:1 by Task 7 review; gate log claim accepted from report.
Task 8: minor (deferred): every_tab_renders lists Network but kittest clicks do not switch tabs (test checks less than its name).
Task 8: minor (deferred): ok_allowed_with_warning asserts only dialog closed.
Task 8: minor (deferred): port_warnings ignores the unselected output's port (harmless, only selected binds).
Task 8: minor (deferred): sync checkbox disabled while server off — a loaded file with sync on + server off leaves sync and Clear stuck (1:1 C#; note as finding).
Task 8: complete (commits 9768bfb..e8b1700, review clean)
Task 9: dispatched implementer (sonnet), BASE e8b1700
Task 9: implementer DONE_WITH_CONCERNS 0b53dfe (mac + linux docker 815/0/5 ignored; #194-#215; #215 C# designer not read); reviewer dispatched (sonnet)
Task 9: review (sonnet) Needs fixes — Important #1: four findings cite nonexistent AirScout/WinTest/WinTest.cs. #215 premise confirmed (Designer.cs:7755-7757). Fix round 1 sent.
Task 9: Ruling: brief-supplied citations #202 (:7871/:7884/:7887), #209 (:3891) and #215 (add Designer.cs:7755-7757) fixed in round 1 — citation accuracy is a global constraint, the plan's lines were wrong — cost if wrong: none.
Task 9: minor (deferred): #198 lacks MapDlg.cs:7030-7032 cite; #208 omits that Play() is conditional on a checked item; README "#194 to #215" sentence unwrapped.
Task 9: fix round 1/5 (4 addressed, 0 open — WinTest.cs paths, #202/#209/#215 cites; commits 0b53dfe..9dfc4a9)
Task 9: complete (commits e8b1700..9dfc4a9, review clean)
Final review (opus): ready with one fix. Important #1: HISTORY ON/OFF leaves Win-Test receiver + Track worker running; upstream stops/starts them (MapDlg.cs:6558/:6585, :2484-2486) — plan error (line 509), not 1:1.
Final: Ruling: Important #1 fixed 1:1 (stop on ON, start on OFF, Options in HISTORY still restarts) — port policy; the plan's line 509 was wrong — cost if wrong: Win-Test cannot steer the app during an analysis (as upstream).
Final: Ruling: fix wave also takes Minor 1 (receiver Error/Status without generation), Minor 2 (cancel check before ASNEAREST send), Minor 3 (restart test residual flake), Minor 4 (README 4c wording, OK-or-Cancel, #208 conditional Play, #198 cite, line wrap) — each cheap, two are flake/stale-event risks that cost every later CI run — cost if wrong: a slightly larger fix diff.
Final: minor (deferred): sync_integrations rebuilds NearestSettings (BTreeMap clone) every frame.
Final: minor (deferred): ports 0..=65535 accepted; a track port 0 prints a send error every second.
Final: minor (deferred): Disabled lost when Options opens — add a finding line (upstream writes the setting on the worker).
Final: minor (deferred): Task 7 <1 s timing assert could be raised to 5 s.
Final: declined-to-judge items (reviewer): DB growth rate per datagram unlimited (1:1); unbounded UI event channel under flood; watchlist dialog OK overwriting Win-Test changes (1:1); minimised-window stepping (ruled); test call signs AA1AA/BB1BB predate branch; Windows runtime of socket tests unverified; .NET -000 unverified.
Final: fix wave dispatched (sonnet), FIX_BASE 9dfc4a9
Final: fix wave DONE_WITH_CONCERNS add4b71 (mac gate green; Minor 1/2 race fixes without test). Scoped re-review dispatched (sonnet).
Final: re-review — all findings addressed (Important #1, Minor 1-4), no new Critical/Important; commits 9dfc4a9..add4b71.
Final: parked — Minor 1/2 race fixes have no covering test — Ruling: a deterministic test needs a production hook; the cancel check is read-verified — cost if wrong: a rare stale error line or one late ASNEAREST datagram after stop.
Final: minor (deferred): #208 cite :7455-7460 should be ~:7476 for the checked-item condition; one over-long README line.
Final: minor (deferred): plan text line 509 (HISTORY leaves integrations) is stale; plan is historical.
