# SDD ledger — plan: docs/superpowers/plans/2026-11-24-stage4c-rotator-dde-omnirig.md

Branch stage4c-rotator-dde-omnirig from 5900919; plan commit 960d511. Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md.
Owner: use a little Gemini 4 Neon (antigravity) — controller uses it as task reviewer on a few tasks.

## Pre-flight scan
# Pre-flight scan — stage 4c plan

Checked against code at 960d511: `as-cat` `worker.rs` (recreate branch :93-103, `finish` :179-186, `old_strategy`), `rig.rs` (`Rig` trait, `DopplerSession::saved/enter/leave`), `lib.rs` (unsafe attrs, re-exports), `config.rs` (`Engine`, `make_rig(&CatConfig)`, `factory`), `tests/worker.rs` (FakeRig log strings, `ENTER`/`TICK`, `cfg()` helper, `rig_lost_mid_doppler_keeps_saved_state`), `tests/common` (`#[path]` includes); `airscout` `tracking.rs` (`track_values(.., &Settings, ..)`, `valid`, `rx_tx`, `TrackValues`, `TrackStatus`, `TrackStep`), `app/tracking.rs` (`TrackRt{gen,requested,heights,last,old}`, private `TRACK_STEP`, `track_tick`, `track_step_values`, `track_station_heights`), `track_service.rs` (`TrackFanout`, `MAX_ERRORS`, `group_name`, `TrackService::start(cfg, tx, gen)`/`step`, outputs built on the `track` thread, no spawn-failure event), `track_outputs.rs` (`OutputGroup{Udp,File}`, `TrackOutput`, `TrackConfig` fields, `build_outputs` 4c marker comments), `cat_service.rs` (`cat_config`, `doppler_of`, `apply_step`, `CatIntegration{service,doppler,strategy}` + `step`), `app/services_api.rs` (516 lines, `Services` trait :16-132, `track_step`, `request_ground_elevations(&mut self)`, `start_integrations` order wintest/track/cat), `app/mod.rs` (761 lines, `pub use services_api::{..}`), `app/shell.rs` (`track_tick` call :91, repaint :98-101), `app/integrations.rs` (`Disabled` match, `TrackStatus::Error`), `ui/options/mod.rs` (521 lines; `positive`, `non_negative`, `validate`, `validate_area`, `num`, `num_step`, `text` present), `ui/options/track.rs` ("not available yet" line :113), `ui/options/cat.rs` (`engines()`, missing-engine text); airscout tests `tracking.rs`, `integrations.rs`, `track_service.rs`, `track_outputs.rs`, `cat_service.rs`, `ui_options_4b.rs` (`no_omnirig_or_serial_entries`), `tests/common/mod.rs` (`t0()` = 2024-05-01); `as-settings` `track.rs`, `cat.rs` (`lenient_engine` string-only, case-sensitive), `lib.rs` re-exports, `radius_km`, `planes_position_ttl_min: i64`; `as-geo` `Band::to_hz -> f64`; `as-aircraft` `PlaneInfoCache::get(hex, at, ttl_min: i64)`, `TrackMode: Default`; `as-wintest::dotnet::{fixed, zero3}` (negative → `-005`); `Cargo.toml` (`members = ["crates/*"]`), `Cargo.lock` (`windows` 0.62.2, `windows-sys` 0.61.2 present, no `serial2`); `.github/workflows/ci.yml` (self-hosted linux, `windows-cross`, hosted macos/windows test); README :139-144; `docs/logic-findings.md` last row #234; upstream `MapDlg.cs` :7838-7962, :7995-8050, :8195-8250, :8313, :8315-8321, `CATWorker.cs` :171-176, :301-318, `OmniRigX.cs` :14-58, :130-141, :233, :842-928, :978-1061, :1113-1257, `OptionsDlg.cs` :2663-2679, `OptionsDlg.Designer.cs` :5671-5672, `OptionsDlg.resx` OmniRig text, `Settings.settings` :275-307. Spec `2026-11-23-airscout-rs-design.md` (as-cat/as-dde rows, anti-goal, stage 4). Rubric: `reviewer-instructions.md` :150-155.

## Table

| # | Tasks | Produces vs consumes | Finding |
|---|---|---|---|
| 1 | T1 – T7 | T1 changes `CatWorker::pass`/`finish` and adds tests to `as-cat/tests/worker.rs`; T7 adds `Engine::OmniRig`, `omnirig_available`, and `omnirig_engine_off_windows_is_norig` to the same test file | Same file, sequential, no interface clash. T7's test text calls `make_rig(Engine::OmniRig{..})`, but the signature is `make_rig(&CatConfig)` → C11 |
| 2 | T1 – T9 | T1's restore behaviour vs T9 finding 18 text | Consistent (saved state and pending leave survive; exit re-opens once). The no-second-enter part of T1 breaks an existing 4b test → C1 |
| 3 | T2 – T3 | T2 produces `Tracker`, `TrackInput`, `TrackShown`, `TrackParams`, `track_input()`; T3 consumes them on the worker and in `track_sync` | Names and derives line up (`TrackMode`/`TrackStatus` have `Default`). `track_input(&self)` cannot issue the heights request (`&mut self` needed) → C2. T2 moves the AppState tests to `tests/tracker.rs`, but T3 rewrites them in `tests/tracking.rs` → C3. After T3, `TrackRt.tracker`/`last` are dead → C7 |
| 4 | T3 – T4 | T3 rewrites `TrackService::start(cfg, ctx)` and the step loop; T4 adds `group_name` arms, the Serial double count and `TrackConfig` fields | Sequential edits of `track_service.rs`/`tests/track_service.rs`; OK. T4's `Disabled(Serial/Dde)` sets `TrackStatus::Error`, which T3's per-frame shown apply overwrites next frame → C5 |
| 5 | T3 – T5 | T3 keeps `build_outputs` inside the `track` thread; T5's `Ddeml` SAFETY note relies on that | Holds (`track_service.rs:140` builds inside the spawned closure). DDE poke timeout vs the Threads bound → C12 |
| 6 | T3 – T7 | T3 rewrites `CatIntegration` (drops `strategy`, `step`; adds `cell()`) in `cat_service.rs`; T7 adds the `CatEngine::OmniRig` arm to `cat_config` there and `omnirig_rig_clamped` in `tests/cat_service.rs` | Disjoint functions; OK. `cat_config` is the only exhaustive `CatEngine` match in the app |
| 7 | T3 – T9 | T3 replaces README :141-144; T9 adds the 4c README text and finding 9 | Finding 9 matches T2/T3. README :139-140 ("Serial and DDE rotators … not available yet") is removed by no task → C14 |
| 8 | T4 – T5 | T4 adds `OutputGroup::{Dde, Serial}`, `TrackConfig.dde`, the Serial `build_outputs` arm, `serial2` in Cargo; T5 adds `DdeOutput`, the `cfg(windows)` DDE arm, `as-dde`/`windows` in Cargo | Consistent: order `Dde, Udp, Serial, File` and the 4c marker comments exist. Both edit `Cargo.toml`/`Cargo.lock`, sequential. T4's `serial2` Windows path is first cross-checked in T5 → C15 |
| 9 | T4 – T7 | T4 creates `as-settings/tests/settings_4c.rs`; T7 adds tests to it and changes `as-settings/src/cat.rs` | Create-before-touch order holds; no shared types |
| 10 | T4 – T8 | T4 produces `TrackSerial`, `TrackDde`, `serial_port`, `serial_baudrate` (4800); T8's Track group and `BAUDRATES` consume them | Names match; 4800 is in `BAUDRATES`; C# validation text verified (`OptionsDlg.cs:2670-2678`) |
| 11 | T4 – T9 | T4 behaviour vs findings 1-6 | Consistent with `MapDlg.cs:7897-7962` (`NewLine = "\r"` at :8042, `Substring(2,3)`, `serialerr++` at :8234/:8246) |
| 12 | T5 – T7 | T5 adds `windows = "0.62"` to the workspace; T7 adds `windows` features for `as-cat` | Order holds (T5 before T7); 0.62.2 already locked, features do not change the lock |
| 13 | T5 – T8 | T5's `cfg(windows)` DDE output vs T8's `cfg(windows)` DDE group | Consistent; off Windows `dde = "hrd"` loads and builds no output |
| 14 | T5 – T9 | T5 behaviour vs findings 7-8 | Consistent (`MapDlg.cs:7838-7861`, `TryConnect` :8021) |
| 15 | T6 – T7 | T6 produces `OmniDispatch`, `OmniOpen`, `OmniRig`, `OmniProp`; T7's `omnirig_com::opener()` and `ComOmni` consume them | `OmniOpen` returns `Box<dyn OmniDispatch>` while the Send argument needs `OmniDispatch: Send`; `OmniProp` lacks `Ord` for the fake's `BTreeMap` → C10. Both edit `as-cat/src/lib.rs`, sequential |
| 16 | T6 – T9 | T6 behaviour vs findings 10-12, 14, 15, 17 | Consistent with `OmniRigX.cs` (constants :14-58, impossible version test :134/:233, `IsWritable` :842-928, branch orders :978-1061/:1113-1257) |
| 17 | T7 – T8 | T7 produces `CatEngine::OmniRig`, `omnirig_rig`, `omnirig_available()`; T8's `engines()` and OmniRig group consume them | Consistent. The 4b test `no_omnirig_or_serial_entries` becomes `cfg(not(windows))` as planned |
| 18 | T7 – T9 | T7 behaviour vs findings 13, 16 | Consistent (`CATWorker.cs:301-306`) |
| 19 | T8 – T9 | T8 static rig list vs finding 16 and README OmniRig text | Consistent |
| 20 | T1 | Tests vs code | Tests 1-3 fail today and test 4 passes ("first three FAIL" holds). The plan omits that the existing `rig_lost_mid_doppler_keeps_saved_state` asserts `ENTER == 2` after re-creation → C1 |
| 21 | T2 | Tests vs code; files created vs touched | `TrackParams` field types match the settings (`band.to_hz()` f64, `offset_s`/`ttl` i64). Fixture "move to tests/common" clashes with `common::t0()` (2024 vs 2026) → C3. `track_input(&self)` vs request → C2 |
| 22 | T3 | Tests vs code; files | Existing `track_service.rs` tests use `TrackService::start(c, tx, gen)` and `svc.step`, and `integrations.rs` has six `track_tick`/`track_step` tests. The plan lists no rewrite of `TrackService::step` uses → C4. `app/mod.rs ≤ 761` vs the new `mod services_trait;` and the rev state → C6. The "spawn-failure event" does not exist in `TrackService` → C8 |
| 23 | T4 | Tests vs code; files | Byte and reply cases agree with C#. `FakeSerial` is needed in two test binaries, and the `build_outputs` tests are placed in `track_service.rs` though they live in `track_outputs.rs` → C9 |
| 24 | T5 | Tests vs code; files | `as-dde` joins via `members = ["crates/*"]`. The 11-step switch-off and the connect count agree with the fanout (`connect` each step while unconnected) |
| 25 | T6 | Tests vs code | The golden IC-746 enter/leave logs and TS-570/simplex logs follow from the stated branch order. Checked against `DopplerSession::enter/leave` (`rig.rs:213-265`). Type defects → C10 |
| 26 | T7 | Tests vs code | `make_rig` call shape → C11. The step 2 multi-package `--test` command is valid; `lenient_engine` change matches the 4b deferred minor |
| 27 | T8 | Tests vs code; files | All seven moved helpers exist. `positive`/`non_negative` are private and used only by `validate` → C16. `serial_group_disabled_without_tracking` states no assertion (rubric: test that asserts nothing) → C17 |
| 28 | T9 | Findings vs tasks; files | Next number #235 is correct (last row #234). README :139-140 → C14 |
| 29 | Plan – Spec | Plan scope vs spec rows and anti-goal | `as-dde` row says "`cfg(windows)` only", but the plan compiles `as-dde` (trait, consts) and the pure DDE/OmniRig logic on all OS → C13. Std threads instead of tokio predate 4c (4a/4b); no new conflict |

## Conflicts and rulings

1. **C1 — T1 breaks an existing 4b test.** `as-cat/tests/worker.rs::rig_lost_mid_doppler_keeps_saved_state` asserts `rig.count(ENTER) == 2` after a re-creation with an unchanged active target. T1's `active_target_survives_recreate_without_second_enter` requires no second enter.
   Ruling: T1 updates that test to `ENTER == 1` (one enter, one tick per Online pass) and keeps its restore assertions — the parked 4b case is an integration-layer fix logged as finding 18, and the saved state is the original one either way — cost if wrong: after a rigctld restart the rig mode is not re-set until the leave; the per-pass tick still sets split and both frequencies.
2. **C2 — `track_input(&self)` cannot issue the heights request.** `request_ground_elevations` takes `&mut self` and `TrackRt.requested` must be set.
   Ruling: `fn track_input(&mut self) -> TrackInput` (`pub(super)`). It requests the heights once per Play, only when LIFE, `track.activate` and mode TRACK hold — this keeps `tracking_off_in_history` and `track_tick_needs_activate_and_plane` at 0 requests — cost if wrong: none.
3. **C3 — T2 test placement vs T3.** T2 moves the three `AppState::track_tick` tests to `tests/tracker.rs`. T3 then rewrites "the `tracking.rs` tests" and lists only `tests/tracking.rs`. Moving `t0()` to `tests/common` also clashes with `common::t0()` (2024-05-01, not 2026-11-23).
   Ruling: AppState-driven tests stay in `tests/tracking.rs` (T3 rewrites them there). `tests/tracker.rs` holds only `Tracker` unit tests, including `old_dropped_for_another_plane` at the `Tracker` level. The fixtures `settings()`/`plane()`/`t0()` are copied into `tracker.rs`, not moved to `common` — test fixtures are not a logic block — cost if wrong: two copies of three small fixtures.
4. **C4 — T3 omits the rewrite of existing `TrackService::step` users.** `tests/track_service.rs` `service_reports_rot_stopped_and_writes_file` and `service_stop_ends_output` call `TrackService::start(c, tx, gen)` and `svc.step`. `tests/integrations.rs` has `single_while_paused_steps_outputs`, `paused_none_mode_sends_invalid_step`, `track_without_heights_sends_invalid_step`, `track_step_once_per_second`, `no_steps_without_activate_or_in_history` and `real_services_track_writes_azel` on `track_tick`/`track_step`.
   Ruling: T3 rewrites all of them onto `TrackCtx` plus the input cell (or `track_input`/`track_sync`). Each keeps its assertion in meaning; none is deleted — the plan's Files header already asks to rewrite these uses — cost if wrong: none.
5. **C5 — The per-frame shown apply erases ERROR.** `on_track_event(Disabled)` sets `TrackStatus::Error` (`app/integrations.rs:77`, test `track_disabled_clears_udp_setting_and_sets_error`). T3 re-applies the matching `TrackShown` every frame, so ERROR disappears on the next frame. Today and in C# it stays until the next step (≤ 1 s).
   Ruling: `track_sync` applies a `TrackShown` only when its rev matches **and** it differs from the last applied one. ERROR then holds until the worker's next step, as today — cost if wrong: one stored `TrackShown` and one compare per frame.
6. **C6 — The `app/mod.rs ≤ 761` cap vs T3.** T3 needs `mod services_trait;` and a split re-export, plus state for `track_rev` and the last pushed input.
   Ruling: `track_rev` and the last pushed `TrackInput` live in `TrackRt` (`app/tracking.rs`). `app/mod.rs` may grow only by the module declaration and re-export lines (≤ 763), with no new logic or fields — the cap guards against logic growth in `mod.rs` — cost if wrong: two lines over the old count.
7. **C7 — Dead `TrackRt` fields after T3.** `TrackRt.tracker` (from T2) and `TrackRt.last` are no longer read once the worker owns the clock. `clippy -D warnings` then fails on dead code.
   Ruling: T3 removes `tracker` and `last` from `TrackRt`. `oldvalues` lives only in the worker's `Tracker` (finding 9) — cost if wrong: none.
8. **C8 — A "spawn-failure event" that does not exist.** T3 says the Track worker's spawn-failure event stays "as in 4a/4b". `TrackService::start` only sets `cancel` on a spawn error (`track_service.rs:156-158`) and sends nothing. `CatService` is the one that sends an event.
   Ruling: keep 4a behaviour (no new event); the sentence is a wording slip — cost if wrong: a failed `track` spawn stays silent, as in 4a.
9. **C9 — Duplicated `FakeSerial` and misplaced build tests (rubric: verbatim duplication).** `serial_disabled_at_sixth_format_error` (in `tests/track_service.rs`) needs the `FakeSerial` defined in `tests/rotator_gs232.rs`. `outputs_in_csharp_order` and `dde_ignored_off_windows` test `build_outputs`, whose tests live in `tests/track_outputs.rs`.
   Ruling: `serial_disabled_at_sixth_format_error` goes into `tests/rotator_gs232.rs`, which already drives `TrackFanout`. The two `build_outputs` tests go into `tests/track_outputs.rs`. `FakeSerial` is defined once — cost if wrong: none.
10. **C10 — T6 type gaps.** `FakeOmni.props: BTreeMap<OmniProp, i32>` needs `Ord`, but `OmniProp` derives only `Clone, Copy, Debug, PartialEq, Eq`. `OmniOpen` returns `Box<dyn OmniDispatch>`, while the plan's own `Send` argument needs `OmniDispatch: Send`.
    Ruling: `OmniProp` also derives `PartialOrd, Ord, Hash`, and the trait is declared `pub trait OmniDispatch: Send` — this is what the plan's `Rig: Send` argument assumes — cost if wrong: none.
11. **C11 — The T7 test calls `make_rig` with the wrong shape.** The plan says `make_rig(Engine::OmniRig { rig: 1 })`, but the signature is `make_rig(c: &CatConfig)` (`config.rs:71`).
    Ruling: `make_rig(&cfg(Engine::OmniRig { rig: 1 }))`, using the existing helper `cfg()` (`tests/worker.rs:438`) — cost if wrong: none.
12. **C12 — The DDE poke timeout vs the Threads bound.** Two HRD pokes at `POKE_TIMEOUT` = 10 s (1:1) can block a step for 20 s. The Threads constraint bounds only the serial exchange (2 × 1 s) and promises "cancel within 250 ms".
    Ruling: keep 10 s (1:1, D12). The 250 ms cancel holds outside an exchange. A DDE step is bounded by 2 × `POKE_TIMEOUT`, and T5 documents that in the `DdeOutput` doc. An old worker blocked after an Options restart is detached; its events are dropped by generation, and its Doppler writes go to the old (unused) cell — cost if wrong: with a hung HRD, an old `track` thread lingers up to 20 s after Options.
13. **C13 — Spec `as-dde` row "`cfg(windows)` only" vs all-OS compile.** The plan builds `as-dde` (`DdeLink`, consts), `DdeOutput` and the OmniRig semantics on every OS. It also keeps `TrackDde::Hrd`/`CatEngine::OmniRig` in the settings everywhere.
    Ruling: accept — the spec's binding intent is the anti-goal: no DDE/COM calls in the shared code, and the app works fully without them. The DDEML/COM calls and the output and engine construction are `cfg(windows)`. The I/O-free logic is compiled everywhere so that it is tested, and settings from other OS must load (Review Focus 5) — cost if wrong: a few hundred lines of unused code in the macOS/Linux binaries.
14. **C14 — No task removes the stale README line.** README :139-140 says "Serial and DDE rotators (stage 4c) are not available yet". T3 replaces only :141-144.
    Ruling: T9 Step 1 replaces that line and keeps "the web server follows in stage 4d" — cost if wrong: none.
15. **C15 — The `serial2` Windows path is checked one task late.** T4 adds `serial2` (with `windows-sys` on Windows), but the first Windows check runs in T5.
    Ruling: T4 Step 4 also runs the Windows check from Global Constraints — cost if wrong: one extra docker run (`target/wingnu` counts toward the disk budget).
16. **C16 — Re-exporting private helpers.** T8 says `mod.rs` re-exports all moved helpers, but `positive`/`non_negative` are private and used only by `validate`.
    Ruling: they move into `fields.rs` as private functions. `mod.rs` re-exports `validate`, `validate_area` (pub) and `num`, `num_step`, `text` (`pub(crate)`) under the current paths — cost if wrong: none.
17. **C17 — A test with no stated assertion (rubric: test that asserts nothing).** T8 `serial_group_disabled_without_tracking` names no assertion.
    Ruling: with `track.activate = false`, the test asserts that the "GS-232A Az/El" radio is disabled (accesskit disabled state), and that a click leaves `track.serial == TrackSerial::None` after OK — cost if wrong: none.

## Pre-flight rulings (controller)
- C1–C7, C9–C17: proposed rulings in the scan adopted as written.
- Ruling: C8 overridden — Task 3 sends ServiceEvent::Error("Track: cannot start the worker thread: {e}") on a spawn failure, like 4b's CatService — closes the 4a deferred minor instead of keeping a silent failure — cost if wrong: none.

Task 1: dispatched implementer (sonnet), BASE 960d511
Task 1: implementer DONE 865d6c5 (as-cat worker 22/22, gate rc=0). finish_without_saved_state_opens_nothing passed before the fix (guard only); 4th RED was the C1-updated old test.
Task 1: reviewer dispatched (sonnet)
Task 1: review (sonnet) spec ✅, Approved, 0 Critical/Important.
Task 1: minor (deferred): finish() re-open can exceed EXIT_WAIT on a dead rig (best effort); finish() ignores get_status result.
Task 1: note for Task 9: per C1 an unchanged active target is not re-entered after a rig recreate (finding text).
Task 1: complete (commits 960d511..865d6c5, review clean)
Side: CI on 5900919 red on Linux (cancel_stops_updaters_quickly 1 s bound) and Windows (cleanup guard test used "/d", relative on Windows) — fixed on branch fix/ci-test-portability (worktree ../AirScout-RS-cifix), not pushed.
Task 2: dispatched implementer (sonnet), BASE 865d6c5; reviewer planned: Gemini 4 Neon
Task 2: implementer DONE 3251586 (gate 918/0; RED 6/6 tracker tests with step forced to None, GREEN 6/6). TrackInput.rev = Play gen counter (implementer choice, Task 3 may change). tests/tracking.rs 642 lines (rustfmt).
Task 2: reviewer dispatched (Gemini 4 Neon via antigravity)
Task 2: review (Gemini 4 Neon) spec ✅, Approved, 0 Critical/Important; no files touched.
Task 2: minor (deferred): shown_carries_input_rev does not cover the missing-heights early-return path.
Task 2: complete (commits 865d6c5..3251586, review clean)
Task 3: dispatched implementer (opus, integration), BASE 3251586
Task 3: implementer DONE_WITH_CONCERNS 5ab5619 c6659e6 (gate 927/0/5, hamlib clippy clean; app/mod.rs 763, services_api.rs 414, services_trait.rs 130). rev = TrackRt.rev (+1 per changed input push, never reset). oldvalues reset on worker restart (= C# local of bw_Track, MapDlg.cs:7997). track_step_once_per_second now checks one push / no UI step; cadence in steps_once_a_second.
Task 3: Ruling: C5 realised as "apply when (rev, status, values.timestamp) differs" (NaN values make TrackShown equality unusable) — ERROR from Disabled then holds while the worker keeps answering the same stopped state, not only until the next step — the error stays visible longer, which no user loses by; C# overwrote it within 1 s — cost if wrong: ERROR shown longer than upstream after an output switched itself off.
Task 3: reviewer dispatched (opus)
Task 3: review (opus) spec ✅, Needs fixes — Important I1: lagging UI track_status lets a CAT Info with the Doppler-shifted rx_hz overwrite dial_freq_hz (certain after a minimised window); C# sets TrackStatus and DopplerStrategy together (MapDlg.cs:8390-8405).
Task 3: Ruling: I1 fixed at the source — the CAT worker tags each RigInfo with whether a Doppler target was active for that poll (`doppler_active: bool`), and the app writes the dial only when it is false (plus the existing TRACKING check and C10/MAX_FREQ_HZ bounds) — the rig side knows the truth, the UI status can lag — cost if wrong: one field on RigInfo.
Task 3: Ruling: M2 (comment at tracking.rs:683 says "until the worker's next step", wrong after the C5 realisation) joins the fix round — a false comment on the revision logic misleads the next reader — cost if wrong: none.
Task 3: minor (deferred): M1 first step after an Options restart uses the pre-Options input (≤1 s); M3 NaN in params re-pushes every frame; M4 loose cadence tests, no late-step burst test; M5 redundant params check in run.
Task 3: fix round 1 sent (resume implementer)
Task 3: fix d75e559 (worker 23/23, cat_app 14/14, gate 929/0/5); re-review dispatched (Gemini 4 Neon)
Task 3: fix round 1/5 (2 addressed, 0 open — RigInfo.doppler_active gates the dial; track_sync comment; commits c6659e6..d75e559), re-review Gemini 4 Neon.
Task 3: complete (commits 3251586..d75e559, review clean)
Task 4: dispatched implementer (sonnet), BASE d75e559
Task 4: implementer DONE_WITH_CONCERNS b4bb041 (settings_4c 3, rotator_gs232 13, track_outputs 10, track_service 14; gate, hamlib clippy, Windows docker clippy+build green). Concerns: settings_4c no RED run; parse_dotnet_float accepts NaN/Infinity (.NET Convert.ToDouble); findings (serial double count, .NET float parsing) for Task 9.
Task 4: reviewer dispatched (sonnet)
Task 4: review (sonnet) spec ✅, Approved, 0 Critical/Important. NaN from parse_dotnet_float only feeds reply_ok is_some(), never a command (C# discards too).
Task 4: minor (deferred → Task 9 findings): set_dtr/set_rts errors abort the open (.NET would open); 1 s read deadline per line vs C# ReadTimeout per read; no stale-byte flush before the query (as C#); "\0+0123" accepted by C# culture StartsWith, refused here.
Task 4: minor (deferred): settings_4c no RED run; two tests wait ~1 s real deadline.
Task 4: complete (commits d75e559..b4bb041, review clean)
Task 5: dispatched implementer (opus, Windows FFI), BASE b4bb041
Task 5: implementer DONE_WITH_CONCERNS 6ef4a3e (macOS gate 954/0/5 incl. track_dde 7; hamlib clippy; Windows docker clippy+build 0 warnings; connect_without_hrd_fails_cleanly runs only in hosted Windows CI). Concerns: extra windows feature Win32_Security (DdeConnect needs it); reconnect every step vs C# once per outer pass; HRD exit not detected (callback no-op → is_connected stays true, pokes fail silently, never switched off; C# counts and switches off after 11); C# writes the culture decimal (comma on German Windows), port writes a dot; C# line cites corrected.
Task 5: reviewer dispatched (opus)
Task 5: review (opus) spec ✅, Needs fixes — Important I1: DDE instance on the never-pumping track thread (C# NDde runs its own message loop, DdeContext.cs:629-757): HRD exit never seen, the 1:1 "[DDE]:" error count + switch-off after 11 steps lost, hidden DDE windows unpumped (can hang other programs' DDE broadcasts).
Task 5: Ruling: I1 overrides the brief's no-op callback — drain the thread's message queue at each step, detect disconnect (DdeQueryConvInfo ST_CONNECTED or DMLERR_NO_CONV_ESTABLISHED on a failed poke), disconnect also DdeUninitialize, fake-link test connected → server gone → 11 counted steps → disabled — integration-layer deviation, restores the C# behaviour — cost if wrong: a small PeekMessage loop on the track thread.
Task 5: minor (deferred → Task 9 findings): reconnect every step vs C# once per pass (brief-mandated, harmless); dot instead of culture comma (HRD parse unknown, likely <1° worst case — ask a German HRD user); DdeConnect has no timeout (doc bound); unsafe impl Send without thread-id guard; C# "Client not initialized" branch unreachable, not ported.
Task 5: fix round 1 sent (resume implementer)
Task 5: fix a154e7a (track_dde 8/8, gate 955/0/5, Windows docker clean). Concern: DDE windows unpumped while tracking is stopped and HRD connected (pump only on valid steps). Re-review dispatched (opus).
Task 5: fix round 1/5 (I1 partly — lost conversation counted, switch-off after 11, DdeUninitialize on disconnect/Drop/failed connect; open: DDE windows unpumped while connected and idle — no values (track_service.rs:78-81) or outside LIFE — can hang plain SendMessage DDE broadcasts for hours; commits 6ef4a3e..a154e7a)
Task 5: minor (deferred → Task 9 findings): HRD crash may give DMLERR_POSTMSG_FAILED/POKEACKTIMEOUT instead of NO_CONV_ESTABLISHED; then counted only once DdeQueryConvInfo sees it gone.
Task 5: fix round 2 sent (resume implementer): TrackOutput::idle() default, DdeOutput pumps, TrackFanout::idle() in the no-values branch and on every run pass incl. CANCEL_POLL sleeps, fake-link idle test.
Task 5: fix round 2 commit 09d9044 (track_dde 10/10, track_service 17/17, gate 960/0/5, Windows docker clean; TrackService::start_with for fake outputs); re-review dispatched (Gemini 4 Neon)
Task 5: fix round 2/5 (idle pump ADDRESSED for a live conversation; new Important: a conversation lost while idle leaves the DDE instance alive (drop_conversation keeps inst/DDEMLMom), idle then returns early at a null hconv and never pumps; commits a154e7a..09d9044). Re-review Gemini 4 Neon.
Task 5: fix round 3 sent (resume implementer): a dead conversation tears down the whole instance (DdeUninitialize), either in Ddeml or by DdeOutput::idle calling disconnect when !is_connected; test.
Task 5: fix round 3 commit fa21be7 (track_dde 12/12, track_service 17/17, gate 962/0/5, Windows docker clean); re-review dispatched (opus)
Task 5: fix round 3/5 (ADDRESSED — dead conversation tears the instance down once on idle, pump whenever an instance exists; whole-state check of 6 states clean; commits 09d9044..fa21be7)
Task 5: minor (deferred): ddeml.rs:9-10 comment "no instance without a conversation" now relies on DdeOutput teardown; DdeDisconnect error on a server-ended conversation ignored; tests/track_service.rs 545 lines.
Task 5: complete (commits b4bb041..fa21be7, 3 fix rounds, review clean)
Task 6: dispatched implementer (sonnet), BASE fa21be7
Task 6: implementer DONE_WITH_CONCERNS ec2e381 (omnirig 14/14, gate + hamlib clippy rc 0). RED only as missing module (E0583), implementation written first. ident() before open "OmniRig"/"OmniRig"; getters/setters before open return NONE/0/false (only get_status opens; C# opens per call); rig 5 on a 4-rig engine text added; interface_version called in open; L17 pinned (RigMode::Other writes nothing).
Task 6: reviewer dispatched (opus)
Task 6: review (opus) spec ✅, Needs fixes — Important: ident sent once before the engine opens (worker.rs:98-100), so "OmniRig V2.x"/"[OmniRig] Rig n" never reach the tooltip (omnirig.rs:476-482). Golden enter/leave and all 7 simplex + 6 split branches match C#.
Task 6: Ruling: fix the ident in the generic worker — re-send Ident whenever rig.ident() differs from the last sent one (checked after each status poll) — fixes OmniRig and any engine whose ident fills in late — cost if wrong: one compare per pass.
Task 6: Ruling: Minor "only 2/7 simplex and 2/6 split branches tested" joins the fix round as one table test per branch — these sequences drive a real radio and a reorder would pass unnoticed — cost if wrong: a few test rows.
Task 6: minor (deferred → Task 9 findings): rig without FREQ/VFOA/VFOAA writeable — leaving Doppler clears split but stays on the shifted RX (1:1 C#); rig 0 / >4 text (C# true, port false); frequencies read once vs twice in C#.
Task 6: minor (deferred): omnirig.rs 545 lines.
Task 6: fix round 1 sent (resume implementer)
Task 6: fix 4f736f0 (omnirig 18, worker 24, gate + hamlib clippy rc 0); re-review dispatched (sonnet)
Task 6: fix round 1/5 (2 addressed, 0 open — ident re-sent on change; table tests for 7 simplex + 6 split branches; commits ec2e381..4f736f0)
Task 6: complete (commits fa21be7..4f736f0, review clean)
Task 7: dispatched implementer (opus, COM FFI), BASE 4f736f0
Task 7: implementer agent affed091e655a57f6 running
Task 7: DONE_WITH_CONCERNS d15510e (COM compiled only; settings_4b test uses flrig as unknown; omnirig on non-Windows → NoRig, Options shows in Task 8). Concerns 1/3 acknowledged: hosted CI + README note per owner decision; 3 is Task 8 scope. Review: opus + gemini-4-neon
Task 7: review dispatched — opus acbcee879b1dc9d6c + gemini-4-neon delegate (task-7-review.md, task-7-review-gemini.md)
Task 7: opus review — spec ✅, quality approved, 0C/0I/4M. Minors: cite OmniRigX.cs:113→:114; GetIDsOfNames per Invoke (2 RPC per prop); EXCEPINFO not captured; Send invariant not compiler-enforced. Waiting on gemini-4-neon (blocking).
Task 7: gemini-4-neon review — spec PASS, quality NEEDS FIXES, 0C/2I/4M. I1: `cArgs: u32::from(argv.is_some())` reads argv after rgvarg raw ptr from argv.as_mut(). I2: ComGuard CoUninitialize without owner ThreadId check (ddeml has one); same as opus Minor 4.
Task 7: Ruling: I1 kept Important-in-loop as a one-line fix (`arg.is_some()`), though under Stacked Borrows a shared read of the Option leaves the SharedRW raw pointer valid in practice — cost if wrong: one line.
Task 7: Ruling: I2 Important (both reviewers saw it; consistency with ddeml owner check) — record ThreadId in ComGuard; on a foreign-thread drop debug_assert and skip CoUninitialize — cost if wrong: a few lines.
Task 7: minor (deferred): Var::drop + windows 0.62 VARIANT Drop double VariantClear (harmless on VT_EMPTY), manual i4 vs From<i32>; PROPERTYPUT+None invariant unguarded; GetIDsOfNames per Invoke (2 RPC); EXCEPINFO dropped; no Windows assertion of omnirig_available()/make_rig in tests/omnirig_com.rs.
Task 7: fix round 1 — I1, I2, plus cite OmniRigX.cs:113→:114 (trivial, same file)
Task 7: fix a285dc5 (as-cat 76 pass, clippy/hamlib/wingnu ok); re-review dispatched (sonnet)
Task 7: fix round 1/5 (3 addressed, 0 open — cArgs from arg; ComGuard owner ThreadId; cite :114; commits d15510e..a285dc5)
Task 7: complete (commits 4f736f0..a285dc5, review clean)
Task 8: dispatched implementer (sonnet, Options UI), BASE a285dc5
Task 8: implementer agent a411ea96a0a7c004a running
Task 8: DONE 7eb6e6d, 4b25ada (991 pass; mod.rs 440; deleted 4a test no_serial_or_dde_controls as contradicting; omnirig_rig_choice Windows-only, unrun). Review dispatched (sonnet)
Task 8: review (sonnet) — spec ✅, approved, 0C/0I/6M. ⚠️ hosted Windows CI unverifiable: deferred to Task 9 / push; windows-cross log present (task-8-wingnu.log, Finished).
Task 8: minor (deferred): Baudrate field range 0..=u32::MAX vs C# MaxValue 115200; cfg!(windows) vs omnirig_available() in cat.rs; recommended-settings text separate label vs same paragraph; no kittest for Windows OmniRig group visibility / bad baud keeps dialog open / engines text; selected_text shows unclamped omnirig_rig; three "Port:" labels on Track tab.
Task 8: complete (commits a285dc5..4b25ada, review clean)
Task 9: Ruling: hosted Windows CI needs a push; pushing waits for the owner — Task 9 runs macOS + Linux docker + Windows docker gates locally, hosted CI runs after the owner allows the push — cost if wrong: Windows-only tests stay unrun until then.
Task 9: dispatched implementer (sonnet, docs+gates), BASE 4b25ada
Task 9: implementer agent aee9a61d166491b5b running
Task 9: DONE 51ef93b (991/0/5 macOS + linux docker, hamlib, wingnu clean; findings #235-#253, minors folded). Review dispatched (sonnet)
Task 9: review (sonnet) — spec ✅, approved, 0C/0I/2M.
Task 9: minor (deferred): HRD range guards (az 0-360, el 0-90) ported in hrd_pokes but no finding; #242 "reconnects every step" — code reconnects only while not connected.
Task 9: complete (commits 4b25ada..51ef93b, review clean)
Final: whole-branch review dispatched (opus), merge-base 5900919
Final: review (opus) — with fixes; 0C/1I/9M. I1 README "done" + "CI proves" before hosted Windows CI ran.
Final: Ruling: re-grade M1 (first step after Options OK uses the old input → rig may enter Doppler ~1 s after Doppler was switched off) to Important — it drives a real radio — cost if wrong: a few lines.
Final: Ruling: README 4c row says done with Windows device paths not yet run by hosted CI until the owner-approved push makes windows-latest green; then the controller updates the row — cost if wrong: one doc line.
Final: Ruling: the one fix wave also takes cheap M2 (step bound text in #239/constraints, or per-read timeout = remaining deadline), M3 (Text only on change per group), M4 (baud DragValue 50..=115200 and validate only when serial output used? → 1:1 C# validates always; choose clamp range only), M5 (show clamped rig), M6 (Windows DDE order test), M7 (ddeml.rs comment), M8 (#242 wording + HRD range guard note) — reviewer recommends, all small — cost if wrong: one more review of ~60 lines.
Final: minor (deferred): M9 omnirig.rs and tests/track_service.rs 545 lines (stage 6 split); T7 make_rig Windows assertion when CI first runs.
Final: fix wave dispatched (opus), FIX_BASE 51ef93b
Final: fix wave c767dad, c7cbe59 (995 pass, hamlib + wingnu clean; M2 doc route; M3 dedupe can hide a lasting track error behind a later text). Scoped re-review dispatched (sonnet)
Final: re-review (sonnet) — I1, M1-M8 ADDRESSED; new Important from M3 dedupe: a lasting uncounted OutputError::Failed text is sent once, a later CAT/status write hides it → unplugged rotator shows TRACKING with no error.
Final: Ruling: no second fix wave — revert only the M3 dedupe (back to the reviewed 1:1 text-per-step behaviour) and its 4 test edits; M3 queue growth while minimised returns to deferred minor — a revert restores code three reviews passed, a new resend scheme would ship unreviewed — cost if wrong: a few MB of queued texts after a night minimised with an unplugged rotator.
Final: minor (deferred): M3 one Text per failing step queued while minimised; egui clamps a hand-edited serial_baudrate > 115200 when the Track tab is drawn.
Final: revert b783736 (994 pass, hamlib clean); final review closed
