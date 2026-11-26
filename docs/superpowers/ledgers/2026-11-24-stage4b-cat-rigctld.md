# SDD ledger — plan: docs/superpowers/plans/2026-11-24-stage4b-cat-rigctld.md

Branch stage4b-cat-rigctld, BASE of branch: 1c3540f (main abe9785 + plan commit). Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md. Survey decisions: docs/superpowers/surveys/2026-11-24-integrations-survey.md. Owner: native hamlib Linux/macOS only, Windows via rigctld.
Disk rule: cargo env prefix; cargo clean --profile dev before each dispatch when no agent builds.


## Pre-flight scan

Checked against code at 1c3540f (4a landed): `Services` trait (services_api.rs:109-118, default no-op methods), `RealServices` (take()-based stop), `ServiceEvent` (no `Cat` yet), `TrackStep { values, mode }`, `rx_tx`, `DARK_GRAY`/`CONTROL` (private consts, tracking.rs:143), `Settings.doppler` (`dial_freq_hz`, `strategy`), `OptionsTab` (13 tabs, private `ALL`), `status::show` (`field` closure, no tooltip), `tab()` helper (private in tests/ui_options_4a.rs), last finding #215. All interfaces the plan says exist do exist with the stated signatures.

### Pair rows (shared file or interface)

| Tasks | Produces vs consumes | Found |
|---|---|---|
| 1 -> 2 | `Backend`, `BackendError`, `CatRig`, `DopplerSession`, `RigIdent` -> `Rigctld: Backend`, golden session | Golden line list matches Task 1 enter/leave op orders (checked op by op). Test files reach `rigctld_proto` (needs `pub mod`; lib.rs visibility not stated) - CONFLICT 6. |
| 1 -> 3 | `Rig`, `DopplerSession`, enums -> `CatWorker.pass` | Consistent. `enter` re-calls `get_status`; see CONFLICT 1. |
| 2 -> 3 | `rigctld_rig`, `RigctldConfig` -> `make_rig` | Consistent. |
| 3 -> 4 | `SerialConfig`, `Parity`.. in config.rs -> `hamlib_conf`, `make_rig` Hamlib arm | Consistent. Test literal `port_name: ""` vs `String` (CONFLICT 12). `NativeHamlib` not exported for `tests/hamlib_native.rs` (CONFLICT 6). |
| 4 -> 5 | feature `hamlib` of as-cat -> airscout `[features] hamlib` | Global feature-check command names `airscout/hamlib`, which exists only after Task 5; Task 4 is told to run it (CONFLICT 2). |
| 3 -> 5 | `DopplerTarget/Cell`, `CatEvent`, `run`, `factory`, `CatConfig` -> `CatService`, `apply_step`, `cat_config` | Consistent. `as_cat::Doppler` vs `as_settings::Doppler` name clash needs aliasing in cat_service.rs (implementer detail). `pass`/`finish` must be `pub` for tests/worker.rs (CONFLICT 15). |
| 2 -> 5 | fake_rigctld.rs shared via `#[path]` | Unused pub items in one including test crate warn dead_code under `-D warnings` (CONFLICT 7). |
| 5 -> 6 | `ServiceEvent::Cat`, `cat_colors`, `rig_reports`, `EXIT_WAIT`, `stop_integrations_and_wait`, `pub(crate)` consts | Same `mod.rs` arm touched by both (Task 5 stub, Task 6 wires): fine, linear. Task 5 `RealServices::stop_integrations` uses `take()`, so `cat.wait` has no handle (CONFLICT 4). |
| 5 -> 5/6 (services_api.rs) | `RealServices` fields, start/stop/track_step | services_api.rs is 493 lines, grows ~35-40 (CONFLICT 11). |
| 3 -> 6 | `RigInfo` -> `Info` handler | `RigInfo.rx_hz` can be 0 (getters swallow errors) and would be written to the dial (CONFLICT 10). |
| 6 -> status.rs | CAT field with tooltip | `field` closure returns nothing; no hover support (CONFLICT 9). |
| 5 -> 7 | `Cat`, `CatEngine`, `Parity`.. -> `cat::show`, `validate` | Consistent. |
| 3 -> 7 | `hamlib_available` -> `engines()` | Consistent. |
| 7 -> 4a tests | `tab()` helper, `no_serial_or_dde_controls` | `tab()` is private to ui_options_4a.rs; 4b test file cannot "use" it (CONFLICT 13). Existing `get_by_label_contains("not available yet")` still matches the new Track line (one match). `ui_options.rs:155` tab-name list needs `Cat`: plan already says so. |
| 7 internal | group heading vs checkbox | CONFLICT 3. |
| 5/6/7 -> 8 | behaviours -> README, findings #216..#234 (19 items) | Numbering free (#215 last). Findings 17, 18, 19 match Tasks 6/7/5. Consistent. |
| 6 -> 6 | `on_exit` vs `stop_integrations` | Reset logic duplicated (CONFLICT 8). |

### Task rows (own text agrees with itself)

| Task | Tests vs code vs files | Found |
|---|---|---|
| 1 | 13 tests vs interface list | Op lists agree with enter/leave text (verified). Wording "for `s == A` it calls `get_split()` first" contradicts test order `GetFreq, GetSplit` (CONFLICT 14). |
| 2 | 12 tests vs table | Byte table and fake agree. `parse_never_panics_on_random_bytes` asserts nothing (CONFLICT 5). `silent_server...` needs a `connect()` before the first `get_freq`: implied. |
| 3 | 13 tests vs rules | Rule 4 runs enter when not Online; breaks the "one pass = connect + one read timeout" constraint (CONFLICT 1). `pass`/`finish` visibility (CONFLICT 15). |
| 4 | 3 default tests + 1 ignored | Files agree; `hamlib_conf` consts compile in default build without dead_code (pub). Visibility (CONFLICT 6), literal (CONFLICT 12), feature-check command (CONFLICT 2). `#[link]` check does not link: fine. |
| 5 | 4 + 12 tests vs interfaces | `stop_integrations_and_wait` (CONFLICT 4), size (CONFLICT 11), dead_code (CONFLICT 7). Leave line list of nine agrees with Task 1/2. |
| 6 | 9 tests + 1 kittest | CONFLICTS 8, 9, 10. `mod cat;` line in mod.rs not listed but within "+15". |
| 7 | 3 + 5 kittests | CONFLICTS 3, 13. `OptionsTab::ALL` length 13 -> 14 and show-match arm noted. mod.rs +~4 lines: under limit. |
| 8 | docs only | Consistent with Global Constraints; no gate gap. |

### Conflicts
- CONFLICT 1: Task 3 `pass` rule 4 — enter runs even when `st != Online`, and `enter` calls `get_status` again (a second connect plus read timeout on a silent rig), breaking "one pass = connect timeout + one read timeout" and double-consuming scripted statuses — gate rule 4 on `st == Online`.
- CONFLICT 2: Global Gate feature check (`airscout/hamlib`) "Tasks 4, 7, 8 run it" — the airscout feature appears only in Task 5 — Task 4 runs only the as-cat command already in its Step 4.
- CONFLICT 3: Task 7 kittest `cat_tab_activate_and_engine` — group heading "Activate CAT Interface" and checkbox share one label, so `get_by_label` is ambiguous — drop the heading text or find the checkbox by role.
- CONFLICT 4: Task 5 `RealServices::stop_integrations_and_wait` — `stop_integrations` `take()`s `cat`, so `cat.wait(max)` has no handle — `take` cat first, call `stop()`, run `stop_integrations`, then `wait(max)`.
- CONFLICT 5: Task 2 `parse_never_panics_on_random_bytes` — asserts nothing (reviewer defect) — assert `read_line` Ok implies len <= MAX_LINE and `parse_freq` Ok implies finite-derived value.
- CONFLICT 6: Task 2 / Task 4 lib.rs — tests call `rigctld_proto::*` and `NativeHamlib`, but `hamlib` is private and `rigctld_proto` visibility unstated — `pub mod rigctld_proto;` and `#[cfg(feature = "hamlib")] pub use hamlib::NativeHamlib;`.
- CONFLICT 7: Global Tests, shared `fake_rigctld.rs` — each including test crate leaves some pub items unused, dead_code fails `-D warnings` — `#![allow(dead_code)]` at the top of the file.
- CONFLICT 8: Task 6 `on_exit` — repeats the reset and `integration_gen += 1` of `stop_integrations` (duplicated logic) — one private helper called by both.
- CONFLICT 9: Task 6 status bar — `field` closure in `ui/status.rs` cannot carry `on_hover_text` — make it return `Response` (or add `cat_field`).
- CONFLICT 10: Task 6 `on_cat_event` `Info` — `rx_hz` is 0 when the getter failed, so the dial becomes 0 — apply only when `rx_hz > 0`.
- CONFLICT 11: Task 5 `services_api.rs` (493 lines) gains ~35-40 lines — over the 500 guide — put cat start/stop/wait/step into a small struct in `cat_service.rs`.
- CONFLICT 12: Task 4 `hamlib_conf_defaults` — `port_name: ""` where the field is `String` — write `String::new()`.
- CONFLICT 13: Task 7 tests — "the `tab()` helper of ui_options_4a.rs" is private to that test crate — copy the 4-line helper into ui_options_4b.rs.
- CONFLICT 14: Task 1 `get_status` text — "for `s == A` it calls `get_split()` first" vs test order `GetFreq, GetSplit` — say "after `get_freq` is Ok, for A it calls `get_split()`".
- CONFLICT 15: Task 3 `CatWorker::pass`/`finish` — listed without `pub`, but tests/worker.rs is an integration test — mark both `pub`.

## Pre-flight rulings
- Ruling: C1 Task 3 `pass` rule 4 (enter Doppler) runs only when `st == Online`; implementer checks the C# CATWorker order and logs a finding if upstream enters while not online — avoids a second connect + read timeout per pass on a silent rig — cost if wrong: Doppler enter one pass later.
- Ruling: C2 the `airscout/hamlib` feature check runs from Task 5 on; Task 4 runs only the as-cat feature check — cost: none.
- Ruling: C3 Task 7 kittest finds the "Activate CAT Interface" checkbox by role (checkbox), keeping the C# heading text — cost: none.
- Ruling: C4 `stop_integrations_and_wait`: take the cat handle first, `stop()`, then `stop_integrations`, then `wait(max)` — cost: none.
- Ruling: C5 `parse_never_panics_on_random_bytes` asserts read_line Ok ⇒ len ≤ MAX_LINE and parse_freq Ok ⇒ finite — cost: none.
- Ruling: C6 `pub mod rigctld_proto;` and `#[cfg(feature = "hamlib")] pub use hamlib::NativeHamlib;` — cost: none.
- Ruling: C7 shared tests/fake_rigctld.rs starts with `#![allow(dead_code)]` — cost: none.
- Ruling: C8 one private helper for the integration reset + `integration_gen += 1`, used by `stop_integrations` and `on_exit` — cost: none.
- Ruling: C9 status bar: the field helper returns `Response` (or a `cat_field`) so the CAT field carries `on_hover_text` — cost: none.
- Ruling: C10 `Info` handler applies the dial frequency only when `rx_hz > 0` — never write 0 Hz rule — cost: none.
- Ruling: C11 cat start/stop/wait/step live in a small struct in `cat_service.rs`; services_api.rs only delegates — 500-line guide — cost: none.
- Ruling: C12 `port_name: String::new()` — cost: none.
- Ruling: C13 copy the 4-line `tab()` helper into ui_options_4b.rs (or tests/common) — cost: none.
- Ruling: C14 get_status order: after `get_freq` is Ok, for A call `get_split()` (matches the test order GetFreq, GetSplit) — cost: none.
- Ruling: C15 `CatWorker::pass`/`finish` are `pub` — cost: none.
Pre-4b: Windows CI fix 9db64e4 — calculator_start test area shrunk to 1x1 deg (default area ~1100 squares x ~2 s refused connect on Windows). BASE for Task 1 = this commit.
Task 1: dispatched implementer (sonnet), BASE 9db64e4
Task 1: implementer DONE d81d9ff (RED recorded, 13 tests, gate green); reviewer dispatched (sonnet)
Task 1: review approved (sonnet).
Task 1: minor (deferred): no tests for get_status Lost-on-split and Rejected/Timeout connect branches.
Task 1: minor (deferred): set_simplex_mode/set_split_mode return values on backend failure untested.
Task 1: minor (deferred): retry-keeps-saved-state deviation has no test.
Task 1: minor (deferred): take_error not cleared at start of get_status (stale text possible; deliberate).
Task 1: complete (commits 9db64e4..d81d9ff, review clean)
Task 2: dispatched implementer (sonnet), BASE d81d9ff
Task 2: implementer DONE 11b5b1f (RED recorded, 15 rigctld tests, gate green; stray stash popped, old stash@{0} verified intact); reviewer dispatched (opus: network protocol)
Task 2: review approved (opus). ⚠️ gate log claim accepted from report (controller confirms at final gate).
Task 2: minor (deferred): random-bytes "finite" check is a no-op (`let _ = f`).
Task 2: minor (deferred): random test rarely reaches the oversize path.
Task 2: minor (deferred): unsolicited extra reply lines are read as the next command's reply (no stale-buffer check before write).
Task 2: minor (deferred → Task 3 dispatch): connect tries each resolved address with the full timeout, so localhost (::1 + 127.0.0.1) can take ~2× timeout; Task 3 must document or bound the pass.
Task 2: minor (deferred): `parsed` duplicates `fail_closes`; long_line_is_lost lacks !is_connected assert; "unused workspace dependency as-cat" warning until as-cat is used.
Task 2: complete (commits d81d9ff..11b5b1f, review clean)
Task 3: dispatched implementer (sonnet), BASE 11b5b1f
Task 3: implementer DONE_WITH_CONCERNS c5fd7ec (14 tests, gate 857/0). Upstream enters Doppler regardless of Online and sets olddoppler before enter (CATWorker.cs:277-300) — port deviation to log in Task 8. Connect uses one shared deadline; real pass bound 2 connects + 1 read, documented.
Task 3: reviewer dispatched (opus)
Task 3: review (opus) Needs fixes — Important #1: leave on a closed connection drops the saved state (rig not restored), in pass and finish(); Important #2: stale NoCat error text emitted when Online. Fix round 1 sent.
Task 3: Ruling: Important #1 overrides brief rule 4 wording ("Not active: session.leave(rig)") — leave only when Online, retry otherwise; finish() reconnects via get_status first — restoring the rig is the binding intent of the logged restore deviation — cost if wrong: leave happens one pass later.
Task 3: Ruling: doc bound corrected to 2C+2R in the same round — EXIT_WAIT (2 s) equals 2C+2R at the 500 ms default, Task 5/6 must reconcile — cost if wrong: none.
Task 3: minor (deferred): no test for the shared connect deadline.
Task 3: minor (deferred): rig-lost drops take_error() text (brief rule 2; C# shows ex.ToString()) — finding entry in Task 8.
Task 3: minor (deferred): Status on change vs C# every Online pass (CATWorker.cs:227); one Info vs up to 5 ReportProgress(2) (:232-265) — findings in Task 8.
Task 3: minor (deferred): unreachable let-else at worker.rs:305-307.
Task 3: note for Task 5/6: EXIT_WAIT 2 s vs real pass bound 2C+2R (= 2 s at 500 ms default) before restore setters run.
Task 3: fix round 1/5 (2 addressed + doc, 0 open — leave only when Online + finish reconnects; stale error dropped; commits c5fd7ec..349b4f7)
Task 3: minor (deferred): doppler_leave_retried_until_online does not assert silence after the leave; finish() ignores get_status result (best effort).
Task 3: complete (commits 11b5b1f..349b4f7, review clean)
Side: 9ff22aa docs: CONTRIBUTING.md (feature branches, linear history) on user request — outside the plan.
Task 4: dispatched implementer (sonnet), BASE 9ff22aa
Task 4: implementer DONE_WITH_CONCERNS b3de7f7 (FFI types unverified, no libhamlib; feature clippy clean; Windows feature = not available).
Task 4: review (opus) spec ✅, Needs fixes — Important #1: serial rig_set_conf keys fatal → Dummy/network/USB rigs fail connect (hamlib conf.c returns -RIG_EINVAL on non-serial ports); Important #2: RIG_DEBUG_ERR is 2 not 1.
Task 4: Ruling: Important #1 overrides brief (all set_conf errors fatal) — rig_pathname and timeout stay fatal, serial keys (serial_speed, data_bits, stop_bits, serial_parity, serial_handshake, rts_state, dtr_state) ignore a set_conf error — hamlib rejects them on non-serial ports and the spec wants network/dummy rigs usable — cost if wrong: a real serial misconfig surfaces at rig_open instead of at set_conf.
Task 4: Ruling: Important #2 — debug level constant set to 2 (RIG_DEBUG_ERR per rig.h enum NONE=0,BUG=1,ERR=2) — brief value was a typo — cost if wrong: hamlib log verbosity only.
Task 4: minor (deferred): cfg predicate all(feature="hamlib", linux|macos) repeated 6× — cfg alias.
Task 4: minor (deferred → Task 8 README): #[link(name="hamlib")] needs RUSTFLAGS=-L/opt/homebrew/lib on Apple Silicon.
Task 4: minor (deferred): rig_strrmode "" for unknown mode returns Ok("") instead of Rejected.
Task 4: minor (deferred): StopBits::Two, Parity Odd/Even/Space untested; `0 as c_long` style; "unused workspace dependency as-cat" warning until Task 5.
Task 4: open check for Task 6/7: rig dropped on worker thread (rig_close never on UI thread).
Task 4: fix round 1 sent (resume implementer)
Task 4: fix round 1/5 (2 addressed, 0 open — serial conf keys best effort; RIG_DEBUG_ERR=2; commits b3de7f7..9f4180e)
Task 4: complete (commits 9ff22aa..9f4180e, review clean)
Task 5: Ruling: EXIT_WAIT stays 2 s (reconciles the Task 3 note) — the 2C+2R bound is reached only when the rig does not answer, and then the restore setters fail anyway; a responsive rig finishes pass + finish in milliseconds — cost if wrong: a slow but live rig (slow serial hamlib) may be cut off on exit before all restore lines are sent.
Task 5: dispatched implementer (opus, integration), BASE 9f4180e
Task 5: implementer DONE_WITH_CONCERNS 4e34c49 (settings_4b 5/5, cat_service 15/15, gate 883/0, hamlib clippy clean, unused-dep warning gone). Concerns: services_api.rs 516 lines; stop_integrations_and_wait returns (). Deviations: fake rigctld via #[path] (tests/common/fake_rigctld.rs); fields in CatIntegration struct (C11); apply_step mode None/Error → strategy None (C# else branch).
Task 5: reviewer dispatched (opus)
Task 5: review (opus) spec ✅, quality Approved, 0 Critical/Important. C2 hamlib clippy evidence in task-5-report.md:42-43.
Task 5: minor (deferred → Task 6 dispatch): CatIntegration::start stops the old worker without waiting — Options restart can overlap old restore and new poll on two rigctld connections.
Task 5: minor (deferred): lenient_engine fails on non-string engine values and is case-sensitive.
Task 5: minor (deferred → Task 6): stop_integrations_and_wait discards wait result (unit return per brief).
Task 5: minor (deferred): services_api.rs 516 lines (only delegation).
Task 5: minor (deferred): real_services_start_stop_cat lacks an elapsed-time upper bound.
Task 5: complete (commits 9f4180e..4e34c49, review clean)
Task 6: dispatched implementer (sonnet), BASE 4e34c49
Task 6: implementer DONE_WITH_CONCERNS 3eee1fb (cat_app 12/12, gate green, hamlib clippy clean, app/mod.rs 761 lines). Concern: cat_ident not cleared on stop (brief names only cat_status).
Task 6: reviewer dispatched (sonnet)
Task 6: review (sonnet) spec ✅, Approved. Rig created/dropped only on the cat thread (Task 4 open check closed). cat_ident kept on stop matches C# (ConnectedRig never cleared).
Task 6: minor (deferred): over-long doc lines tracking.rs:3, app/tracking.rs:4.
Task 6: complete (commits 4e34c49..3eee1fb, review clean)
Task 7: dispatched implementer (sonnet), BASE 3eee1fb
Task 7: implementer DONE_WITH_CONCERNS 4d78368 (ui_options_4b 8/8, gate green, hamlib clippy clean, options/mod.rs 510). Concerns: serial group only clippy-checked (feature); combo text read as node value; tab label "CAT"; serial Timeout reuses timeout_ms.
Task 7: reviewer dispatched (sonnet)
Task 7: review (sonnet) spec ❌ — Important: every_tab_renders (tests/ui_options.rs:142-160) lacks "CAT". Concerns accepted (tab text "CAT" = OptionsDlg.Designer.cs:5354).
Task 7: Ruling: reviewer's Minor "num() has no step 100" joins the fix round — the brief names step 100 for CAT Refresh and both timeouts, so it is a spec gap, not polish — cost if wrong: one small helper change.
Task 7: minor (deferred): pick shows empty text for a baudrate/databits value outside its list (hamlib build); pick keys combo ids by label text.
Task 7: fix round 1 sent (resume implementer)
Task 7: fix round 1/5 (2 addressed, 0 open — CAT in every_tab_renders; num_step speed 100 (egui 0.37 DragValue has no step); commits 4d78368..0e68128)
Task 7: minor (deferred): options/mod.rs 521 lines (guide ~520).
Task 7: complete (commits 3eee1fb..0e68128, review clean)
Task 8: dispatched implementer (sonnet), BASE 0e68128
Task 8: implementer DONE_WITH_CONCERNS e96f3fe (macOS + docker gates 903/0/5 ignored, hamlib clippy local + docker). Corrected 3 brief cites (CATUpdater.cs:45-49,:159; CATWorker.cs:217, :226-270; finding 9 strategy+copy share the non-zero condition MapDlg.cs:8436-8447). Findings #216–#234.
Task 8: reviewer dispatched (sonnet)
Task 8: review (sonnet) spec ✅, Approved, 0 Critical/Important (~25 cites spot-checked).
Task 8: minor (deferred → final triage): finding 216 cite should be CATWorker.cs:191-198; finding 218 order S, F, I (rig.rs:179-181); finding 222 guard ScoutBaseRig.cs:671-672; README "once per second" → every CAT refresh; README native hamlib "AirScout opens no socket" → AirScout's own code (libhamlib may); over-long line in "Start rigctld" bullet.
Task 8: complete (commits 0e68128..e96f3fe, review clean)
Final: whole-branch review dispatched (opus)
Final: review (opus) ready after fixes — Important 1: rig lost (rigctld restart/garbage) mid-Doppler drops saved state, re-saves shifted state, restore tunes to stale freq (worker.rs:97,:117); Important 2: rig-reported freq unbounded → Doppler maths overflow (debug panic on UI thread) (rigctld_proto.rs:66-75, app/cat.rs:35-36, tracking.rs:121-128).
Final: Ruling: re-grade Minor 2 (partial save in DopplerSession::enter leaves Saved{rx 0} and returns true → rig left split, nothing restored) to Important — same effect as Important 1 on a real radio — cost if wrong: one small fix.
Final: Ruling: Minors 1 (tick only while Online), 3+4+5 (README SINGLE claim, "once per second", cites #216/#218/#222, hamlib socket wording, line wrap) join the fix wave — wrong public docs and a pass-bound break the exit wait relies on; all cheap — cost if wrong: a few lines.
Final: Ruling: Minor 6 (Options restart overlap) documented in finding #232, not fixed — race only for an instant OK/Cancel; rigctld serializes — cost if wrong: rig briefly polled by two connections.
Final: minor (deferred): lenient_engine case-sensitive, non-string engine fails settings load (hand-edited file only).
Final: minor (deferred): services_api.rs 516, options/mod.rs 521 lines — split in 4c.
Final: minor (deferred): name resolution not bounded by connect timeout (cat thread only, documented).
Final: fix wave dispatched (opus), FIX_BASE e96f3fe
Final: fix wave DONE_WITH_CONCERNS f766378 2b39d0a 2fa0ba1 (gate 908/0/5, hamlib clippy clean). Concerns: overflow guard test passed before fix (guard only); rig-lost deviation folded into #230; finish() does not recreate a missing rig at exit; failed mode/split/RIT save reads store NONE (as upstream).
Final: re-review (opus) 5/5 ADDRESSED, no new Critical/Important. finish() not recreating a missing rig at exit is pre-existing (nothing restored after rig loss before the fix either).
Final: parked — strategy goes None while the rig is gone → restore skipped when the rig returns (old_strategy reset on recreate); kept saved state used only at next enter/Options/exit — Ruling: real, deferred to stage 4c (CAT worker touched again for OmniRig); strictly better than before the fix — cost if wrong: after a rigctld restart mid-tracking the rig can stay split at the shifted frequency until the next enter, Options or exit, and that later restore can overwrite a frequency the user tuned meanwhile.
Final: parked — finish() at exit does not recreate a missing rig — Ruling: pre-existing, deferred to 4c with the item above — cost if wrong: a rig lost just before exit is left in Doppler split.
Final: review clean after one fix wave (commits e96f3fe..2fa0ba1)
Finish: gate on 2fa0ba1 rc=0, 908 passed / 0 failed / 5 ignored
