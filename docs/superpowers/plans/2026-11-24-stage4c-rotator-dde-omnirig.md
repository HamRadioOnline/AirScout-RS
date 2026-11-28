# Stage 4c — GS-232A Rotator, HRD DDE, OmniRig and Tracking off the UI Thread Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stage 4c of the spec, "Integrations", as split in the survey:
- A GS-232A serial rotator output (Az only, Az/El) in the Track worker, 1:1 with the C# commands, timing and error counting.
- A DDE rotator output to Ham Radio Deluxe (`HRDRotator`), Windows only, in a new crate `as-dde`.
- An OmniRig CAT engine (COM, Windows only) in `as-cat` with working V2 support (D6), plus the OmniRig entries and texts of the Options CAT tab that 4b deferred.
- The tracking step moves from the UI frame loop into the Track worker, so tracking, the rotator outputs and the Doppler push keep running while the window is minimised.
- The two CAT restore cases parked at the end of 4b.

**Architecture:**
- **Tracking off the UI thread.**
  - A pure `Tracker` (new `tracker.rs`) holds the step logic that `app/tracking.rs` runs today (`track_step_values`), with `oldvalues`. Its input is a `TrackInput` snapshot (life mode, track mode, selected hex, station heights, a small `TrackParams` derived from the settings).
  - The app writes `TrackInput` into a shared cell when it changes (one compare per frame, no I/O). The Track worker thread owns the 1 s clock: each step it reads the cell, runs `Tracker::step`, fans the step out to the outputs, and calls 4b's `apply_step` on a clone of the CAT `DopplerCell`.
  - The worker publishes the latest `TrackShown` (values and TRK status) in a second cell, tagged with the input revision. The app reads it each frame and applies it only when the revision matches its last input, so a stale step cannot undo a Pause.
  - No per-step event: a minimised window collects no events except state changes and errors.
- **GS-232A** (`rotator_gs232.rs`): pure command formatting and reply checking over any `Read + Write`. The port is opened through a `SerialOpen` function (real: `serial2`; tests: an in-memory fake). It is a `TrackOutput` in the group `Serial`.
- **DDE** (`as-dde`): a `DdeLink` trait plus `Ddeml`, a `cfg(windows)` DDEML client. The app's `DdeOutput` (all OS, tested with a fake link) formats the HRD pokes. The Windows build adds it as the first output.
- **OmniRig** (`as-cat/src/omnirig.rs`): `OmniRig` implements the C# `OmniRigX` semantics as a `Rig` over a thin `OmniDispatch` trait (property get/put, `IsParamReadable`/`IsParamWriteable`). It reuses `DopplerSession`. `omnirig_com.rs` (`cfg(windows)`) implements `OmniDispatch` with `IDispatch` late binding. The pure part is compiled and tested on every OS.
- `unsafe` lives only in `as-cat/src/hamlib.rs` (4b), `as-cat/src/omnirig_com.rs` and `as-dde/src/ddeml.rs`.

**Tech Stack:**
- Rust 1.95+, std threads, eframe/egui 0.37, egui_kittest 0.37 (dev), tempfile (dev).
- New, pending the owner (Open questions): `serial2` 0.2 (serial port; BSD-2-Clause OR Apache-2.0); `windows` 0.62 as a `cfg(windows)` dependency of `as-cat` and `as-dde` (MIT OR Apache-2.0; 0.62.2 is already in `Cargo.lock` through the GUI stack). Neither needs a system library, so CI and the Linux docker gate keep their packages.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`:
- stage 4 "Integrations" (`as-dde`/OmniRig on Windows);
- the `as-cat` row ("OmniRig only `cfg(windows)`") and the `as-dde` row ("`cfg(windows)` only");
- non-goal: "Platform-specific features (DDE, OmniRig COM) leaking into the shared code; they live behind `cfg(windows)` and the app works fully without them."

**Inputs:**
- Survey `docs/superpowers/surveys/2026-11-24-integrations-survey.md`, sections 4.4, 5.1-5.3, 5.6, 6, 8.3, 9 (L9, L10, L16, L17, L18) and the Decisions (D6, D12).
- Ledger `docs/superpowers/ledgers/2026-11-24-stage4b-cat-rigctld.md`:
  - "Final: parked" (two restore cases, Task 1).
  - Deferred minors folded here: `services_api.rs` 516 lines (Task 3), `ui/options/mod.rs` 521 lines (Task 8), `lenient_engine` case and non-string values (Task 7).
  - Not folded: the other 4b minors (unbounded DNS resolution, Options restart overlap #232); 4c does not touch that code.
- Ledger `docs/superpowers/ledgers/2026-11-24-stage4a-wintest-track.md`: the fanout and reconnect structure (D12), kittests set tabs directly, the declined "minimised-window stepping" item (now Task 3).
- Upstream `target/upstream/AirScout/` @ `91b02cb`, read-only. C# references are relative to `target/upstream/AirScout/AirScout/` unless they start with `AirScout.CAT/`.

**Stage split:**
- **4c (this plan):** the Track worker clock, GS-232A, `as-dde` + HRD output, OmniRig, the two restore fixes, Options Track and CAT additions.
- **4d:** `as-web`.
- **Not ported (findings, Task 9):** reading rig models from OmniRig for the Options list (C# does COM on the UI thread); `ExtOmniRig.cs` (dead code); `GetVfo`/`SetVfo` (never called by the worker).

## Global Constraints

Everything in the 4b plan's Global Constraints still applies (port policy, secrets, network, settings, threads, cargo, gate, tests, file size). Changes and additions:

- **Port policy:**
  - Behaviour that affects the rig, the rotator or the prediction is ported 1:1, even when wrong, and logged in `docs/logic-findings.md`. Logic fixes are stage 6. The next finding number is **#235** (check the last row first); Task 9 assigns numbers in its list order.
  - Integration-layer and GUI bugs are not reproduced. Each gets a finding: "Not reproduced", "Port deviation" or "New design".
  - Hard rule from 4b: no frequency ≤ 0 Hz and none above `as_cat::MAX_FREQ_HZ` is ever written to a rig. OmniRig adds the C# 32-bit bound (`> i32::MAX` gives false).
  - D12: rounding (`"000"`, `F1`) and the error counters stay 1:1.
  - Every C# `file:line` in code comments and findings is checked against `target/upstream`.
- **Platform:**
  - DDE and OmniRig code is `#[cfg(windows)]` only. On macOS and Linux the app builds, runs and passes all tests without it. The Options dialog shows no DDE or OmniRig choice there.
  - The pure logic (HRD poke bytes, OmniRig sequences, GS-232A protocol) compiles and is tested on every OS. COM and DDE calls sit behind `OmniDispatch` and `DdeLink`.
  - Windows-only code must pass `clippy -D warnings` for `x86_64-pc-windows-gnu` (CI job `windows-cross`) and the hosted Windows tests.
- **No I/O on the UI thread:** opening a serial port, COM creation and calls, and DDE calls run on the `track` or `cat` thread only. The UI thread only writes `TrackInput` and reads `TrackShown` and events.
- **Untrusted input:** serial replies, COM return values and DDE results never panic and never allocate in proportion to a received value.
  - A GS-232A reply line is at most `MAX_REPLY` = 256 bytes. Bytes above 0x7F decode as `?` (C# `Encoding.ASCII`).
  - An OmniRig integer outside the known constants maps to NONE (1:1 switch fall-through), except `Status` (Task 6).
- **Unsafe:**
  - `airscout`, `as-settings` and every other crate keep `#![forbid(unsafe_code)]`.
  - `as-cat/src/lib.rs`: `#![cfg_attr(not(any(feature = "hamlib", windows)), forbid(unsafe_code))]` and `#![cfg_attr(any(feature = "hamlib", windows), deny(unsafe_code))]`. Only `hamlib.rs` and `#[cfg(windows)] #[allow(unsafe_code)] mod omnirig_com;` contain `unsafe`.
  - `as-dde/src/lib.rs`: the same pair with `windows` only; only `#[cfg(windows)] #[allow(unsafe_code)] mod ddeml;`.
  - Every `unsafe` block has a `// SAFETY:` comment. Each `unsafe impl Send` states that the handle is created and used on one thread only.
- **Threads:**
  - Track worker `track`: steps at once after start, then every `TRACK_STEP` (1 s, `MapDlg.cs:8313`), on a deadline (no drift; a late step does not cause a burst). Cancel is seen within 250 ms. A serial exchange can delay one step by about 4 s at most: 1 s for the query write, about 2 s for the reply (the 1 s reply deadline is checked between reads, and one read can block for the 1 s read timeout) and 1 s for the set write.
  - CAT worker `cat`: unchanged; OmniRig COM is initialised on it (multithreaded apartment) and released on it.
- **Cargo:**
  - Every cargo command carries `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
  - Disk: total under 120 GB; check `du -sh target` before each task; above 40 GB run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clean --profile dev` (add `--target-dir target/linux` or `--target-dir target/wingnu` for the docker dirs). **Never** plain `cargo clean`: it deletes `target/upstream`.
  - A task that adds a dependency runs one `cargo build` without `--locked` and commits `Cargo.lock`. New workspace entries: `serial2 = "0.2"`, `windows = "0.62"`, `as-dde = { path = "crates/as-dde" }`.
- **Gate:**
  - `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo fmt --all --check && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy --workspace --all-targets --locked -- -D warnings && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace --locked`.
  - hamlib feature check (4b, unchanged): `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy -p as-cat -p airscout --all-targets --locked --features as-cat/hamlib,airscout/hamlib -- -D warnings`.
  - Linux docker gate (4b, unchanged): `docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target/linux -e CARGO_BUILD_JOBS=2 -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 rust:1.95 bash -c 'apt-get update && apt-get install -y libasound2-dev pkg-config && rustup component add rustfmt clippy && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked && cargo clippy -p as-cat -p airscout --all-targets --locked --features as-cat/hamlib,airscout/hamlib -- -D warnings'`.
  - **Windows check (new, the CI `windows-cross` job locally):** `docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target/wingnu -e CARGO_BUILD_JOBS=2 -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 rust:1.95 bash -c 'apt-get update && apt-get install -y gcc-mingw-w64-x86-64 && rustup target add x86_64-pc-windows-gnu && rustup component add clippy && cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings && cargo build --workspace --all-targets --locked --target x86_64-pc-windows-gnu'`. Tasks 5, 7, 8 and 9 run it. `target/wingnu` counts toward the disk budget.
  - `#[cfg(windows)]` tests run only in the hosted Windows CI job. The stage is not done until that job is green on the branch.
- **Tests:**
  - No display, no network beyond 127.0.0.1, no real serial port, no COM server, no DDE server (the Windows smoke tests in Tasks 5 and 7 only prove that a missing HRD/OmniRig is handled).
  - Generic data only: 144 300 000 Hz, `TE1ST`/`TE2ND`, port names `COM13` and `/dev/ttyTEST0`.
  - Waits bounded at 10 s, returning early.
- **File size:** new files stay under about 500 lines. `app/mod.rs` (761) must not grow; `app/services_api.rs` and `ui/options/mod.rs` end below 500 (Tasks 3 and 8).

## Review Focus

1. **The window is minimised or hidden for a long time while tracking** (eframe stops calling `update`).
   - Expected: UDP, file, serial and DDE outputs and the Doppler push continue once a second. Memory does not grow per step. On restore, the TRK field and readout show the current step.
   - Tests: Task 3 `worker_steps_without_ui`, `steady_tracking_sends_no_event_per_step`, `stale_shown_revision_ignored`.
2. **The rotator interface is missing, busy, unplugged mid-session, silent, or sends garbage** (wrong port name, no reply, `?0123`, a 300-byte line, bytes ≥ 0x80).
   - Expected: no panic and no UI stall. One status text per step. The port is reopened on the next step. Only `[Serial]:` format errors count, and the output switches itself off at the 6th one (L18, 1:1).
   - Tests: Task 4 `open_failure_is_shown_not_counted`, `timeout_closes_and_reopens`, `long_reply_fails`, `non_ascii_reply_is_format_error`, `serial_disabled_at_sixth_format_error`.
3. **rigctld or the rig disappears during Doppler, then tracking ends or the app exits before the rig is back.**
   - Expected: the rig is restored once when it returns, and at exit the worker makes one attempt to reach it.
   - Tests: Task 1 `strategy_none_while_rig_gone_restores_on_return`, `finish_recreates_missing_rig_to_restore`, `finish_without_saved_state_opens_nothing`.
4. **OmniRig not installed, OmniRig V1 with Rig 3 selected, or a COM call fails** (version or status values outside the constants, `IsParamWriteable` throws).
   - Expected: NOCAT or NORIG with one text, no busy loop, no panic. A failed call reads as NONE/0, as in C#.
   - Tests: Task 6 `open_error_is_nocat_with_text`, `rig3_on_v1_engine_is_norig`, `com_error_on_status_is_none`, `unknown_status_value_is_online`; Task 7 `com_open_without_omnirig_reports_nocat` (Windows).
5. **Settings from another OS or build** (`dde = "hrd"` on Linux, `engine = "OmniRig"` or `engine = 42`, `serial_baudrate = 1234`, `omnirig_rig = 9`).
   - Expected: the file loads. Linux builds no DDE output. OmniRig on macOS/Linux reports NORIG and Options shows it as unavailable. Options blocks OK for an invalid baud rate with the C# text.
   - Tests: Task 4 `track_4b_file_loads_unchanged`, `dde_ignored_off_windows`; Task 7 `engine_lenient_case_and_type`, `omnirig_rig_clamped`; Task 8 `invalid_baudrate_blocks_ok`, `omnirig_engine_off_windows_shown_as_unavailable`.

---

### Task 1: CAT worker — the two parked restore cases

**Files:**
- Modify: `crates/as-cat/src/worker.rs` (`pass` recreate branch, `finish`)
- Test: `crates/as-cat/tests/worker.rs`

**Interfaces (AirScout.CAT/CATWorker.cs:171-176, :301-306, :311-318):**
- Consumes: `CatWorker`, `DopplerSession::saved`, `RigFactory`, the test `FakeRig` (4b).
- Produces (no new names):
  - On recreating a rig, `old_strategy` is reset to `Doppler::None` only when `session.saved()` is `None`. With a saved state it is kept, so a target `None` causes the leave once the new rig is Online, and an unchanged active target goes on ticking without a second enter.
  - `finish()`: when there is no rig and `session.saved().is_some()`, call the factory once. If it gives a rig, run `get_status(Doppler::None)` and then `session.leave(rig)`. Without a saved state the factory is not called. It always returns `[Status(RigStatus::None)]`.

- [ ] **Step 1: Write the failing tests** (update 500 ms, factory call counter, 4b `FakeRig` log strings):
  - `strategy_none_while_rig_gone_restores_on_return`: enter with target `{A, 144300312, 144300000}`. Then rig 1 reports status None (rig lost). The target becomes `{None, ..}`. Pass 3 creates rig 2 (Online). Rig 2's log has `"set_simplex_mode(144300000,USB)"` exactly once in pass 3, and no `set_split_mode` call.
  - `active_target_survives_recreate_without_second_enter`: as above but the target stays `{A, ..}`. Rig 2 logs no enter (`"set_split_mode(144300000,144300000,USB)"` absent) and one `"set_split_mode(144300312,144300000,NONE)"` per Online pass.
  - `finish_recreates_missing_rig_to_restore`: enter, rig lost, then `finish()`. The factory count goes 1 → 2, rig 2 logs the leave, and the result is `[Status(RigStatus::None)]`.
  - `finish_without_saved_state_opens_nothing`: rig lost without an enter, then `finish()`. The factory count stays 1.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat --test worker`. Expected: the first three FAIL.
- [ ] **Step 3: Implement.** Update the comment at the recreate branch and the `finish` doc (one extra connect at exit, bounded by the backend timeout; `EXIT_WAIT` still caps the app's wait).
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat`. Expected: PASS.
- [ ] **Step 5: Commit** `fix(as-cat): restore the rig after it returns and at exit when it was lost`.

### Task 2: Pure `Tracker` — the step logic leaves `AppState`

Behaviour is unchanged in this task: the app still calls the step from the frame loop, now through `Tracker`. Task 3 moves the call.

**Files:**
- Create: `crates/airscout/src/tracker.rs`
- Modify: `crates/airscout/src/tracking.rs` (`track_values` takes `&TrackParams`), `crates/airscout/src/app/tracking.rs` (`TrackRt` keeps a `Tracker`; `track_step_values` delegates), `crates/airscout/src/lib.rs`
- Test: `crates/airscout/tests/tracker.rs` (new), `crates/airscout/tests/tracking.rs` (the `track_values` tests build `TrackParams::from_settings(&settings())`)

**Interfaces (MapDlg.cs:8054-8155, :8251-8281):**
- Consumes: `track_values`, `valid`, `rx_tx`, `TrackValues`, `TrackStatus`, `TrackStep` (3c/4a); `PlaneInfoCache::get(hex, at, ttl_min)`.
- Produces (`tracker.rs`):
  - `#[derive(Clone, Debug, PartialEq)] pub struct TrackParams { pub my_lat: f64, pub my_lon: f64, pub dx_lat: f64, pub dx_lon: f64, pub radius_km: f64, pub band_hz: f64, pub set_az: f64, pub set_el: f64, pub offset_s: i64, pub ttl_min: i64, pub dial_freq_hz: i64, pub strategy: DopplerStrategy }`.
    - `TrackParams::from_settings(s: &Settings) -> Self` (`radius_km()`, `band.to_hz()`, `track.set_az/set_el/offset_s`, `planes_position_ttl_min`, `doppler.*`).
  - `#[derive(Clone, Debug, Default, PartialEq)] pub struct TrackInput { pub rev: u64, pub life: bool, pub mode: TrackMode, pub hex: Option<String>, pub heights: Option<(f64, f64)>, pub params: Option<TrackParams> }`. `heights` are above sea level including the antenna height. `params == None` means "no step".
  - `#[derive(Clone, Debug, Default, PartialEq)] pub struct TrackShown { pub rev: u64, pub values: Option<TrackValues>, pub status: TrackStatus }`.
  - `#[derive(Default)] pub struct Tracker { old: Option<TrackValues> }`:
    - `fn step(&mut self, input: &TrackInput, now: DateTime<Utc>, cache: &PlaneInfoCache) -> Option<(TrackStep, TrackShown)>`.
    - It gives `None` when `!input.life` or `params` is `None`.
    - Otherwise it applies today's `track_step_values` rules:
      - `old` is dropped when TRACK runs for another hex.
      - TRACK without `heights` gives values `None` and status Stopped.
      - Invalid values give Stopped.
      - Valid values get `rx_tx`; the status is Tracking for TRACK and Stopped otherwise.
      - `old` is set to valid values.
    - `TrackShown.rev = input.rev`.
  - `track_values(at, plane, mode, p: &TrackParams, my_h, dx_h, old)`: the body is unchanged except the parameter.
- `app/tracking.rs`: `TrackRt` gets `tracker: Tracker` in place of `old`. `track_tick` builds a `TrackInput` (via `fn track_input(&self) -> TrackInput`, `pub(super)`, also used by Task 3), calls `tracker.step` and applies the `TrackShown` to `track` (only when values are `Some`) and `track_status`. The heights request moves into `track_input`: it requests once per Play when the mode is TRACK.

- [ ] **Step 1: Write the failing tests** (`tracker.rs`; the 3c/4a test helpers `settings()`, `plane()`, `t0()` copied or moved to `tests/common`):
  - `no_step_outside_life_or_without_params`.
  - `track_without_heights_is_stopped`: values `None`, status Stopped.
  - `track_with_plane_tracks_and_sets_rx_tx`: heights `(0.0, 0.0)`, strategy A, dial 144300000, plane approaching over two steps 1 s apart. Step 2 has status Tracking, `rx_hz != 144300000`, `tx_hz == 144300000`, and the step's mode TRACK.
  - `single_uses_set_az_and_is_stopped`.
  - `old_dropped_for_another_plane` (moved from `tracking.rs::old_values_dropped_by_an_invalid_step_of_another_plane`).
  - `shown_carries_input_rev`.
  - Move `track_tick_values_once_a_second`, `track_tick_waits_for_the_heights_from_the_service` and `track_tick_needs_activate_and_plane` unchanged in meaning (they still drive `AppState::track_tick` in this task).
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test tracker`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS, including every existing tracking, integrations and CAT test.
- [ ] **Step 5: Commit** `refactor(airscout): tracking step as a pure Tracker over a TrackInput snapshot`.

### Task 3: The Track worker owns the clock; the UI only syncs

**Files:**
- Create: `crates/airscout/src/app/services_trait.rs` (the `Services` trait moves here unchanged, then gets the changes below; `app/mod.rs` re-exports it as before)
- Modify:
  - `crates/airscout/src/track_service.rs` (clock, `Tracker`, Doppler cell, shown cell)
  - `crates/airscout/src/cat_service.rs` (`CatIntegration` drops `strategy` and `step`, gains `cell()`)
  - `crates/airscout/src/app/services_api.rs` (`RealServices` only)
  - `crates/airscout/src/app/tracking.rs` (`track_tick` becomes `track_sync`), `crates/airscout/src/app/shell.rs` (calls `track_sync`; the repaint comment)
  - `README.md` (lift the minimised-window limitation, the bullet at lines 141-144)
- Test: `crates/airscout/tests/track_service.rs`, `crates/airscout/tests/integrations.rs`, `crates/airscout/tests/tracking.rs`, `crates/airscout/tests/cat_service.rs` (rewrite the uses of `track_step`/`track_tick`)

**Interfaces:**
- Consumes: `Tracker`, `TrackInput`, `TrackShown`, `TrackParams` (Task 2); `apply_step`, `doppler_of`, `DopplerCell` (4b); `TrackFanout` (4a).
- Produces:
  - `pub type TrackInputCell = Arc<Mutex<TrackInput>>;` and `pub type TrackShownCell = Arc<Mutex<TrackShown>>;` (`tracker.rs`).
  - `pub const TRACK_STEP: Duration = Duration::from_secs(1);` (`track_service.rs`; removed from `app/tracking.rs`).
  - `pub struct TrackCtx { pub input: TrackInputCell, pub cache: Arc<PlaneInfoCache>, pub doppler: DopplerCell, pub tx: Sender<ServiceEvent>, pub generation: u64 }`.
  - `TrackService::start(cfg: TrackConfig, ctx: TrackCtx) -> Self`. The thread, per step:
    1. copies `*input.lock()` (poison: inner value);
    2. runs `tracker.step(&input, Utc::now(), &cache)`; `None` means nothing else happens this step;
    3. stores the `TrackShown`;
    4. sends the `fan.step(&step)` events;
    5. calls `apply_step(&mut doppler.lock(), &step, doppler_of(params.strategy))`.
    - The spawn-failure event and `Rot(Stopped)` first stay as in 4a/4b.
  - `TrackService::shown(&self) -> TrackShown`. `TrackService::step` is removed.
  - `Services`: `fn track_step` is replaced by `fn track_input(&mut self, _i: TrackInput) {}` (stores; never blocks) and `fn track_shown(&self) -> Option<TrackShown> { None }`.
  - `RealServices`:
    - One `TrackInputCell` lives for the whole `RealServices`, so a restarted worker sees the current input at its first step.
    - `start_integrations` starts CAT first, then Track with `doppler: self.cat.cell()` and the `cache` argument.
    - `track_shown` reads the running worker's cell.
  - `AppState::track_sync(&mut self)` (replaces `track_tick`, called every frame):
    - Builds `track_input()`, with `params: None` unless `settings.track.activate`.
    - If it differs from the last pushed input, ignoring `rev`, it does `track_rev += 1`, pushes the input with that `rev` and remembers it.
    - Then, when `services.track_shown()` is `Some(sh)` with `sh.rev == track_rev`, it applies `sh` as Task 2 did.
  - `shell.rs` keeps `request_repaint_after(1 s)` while tracking (display only); the comment says so.

- [ ] **Step 1: Write the failing tests:**
  - `track_service.rs`:
    - `worker_steps_without_ui`: UDP AirScout output to a loopback receiver (4a helper), input `{life, TRACK, hex, heights (0,0), params}` with the plane in the cache and strategy A, dial 144300000. Nothing calls the app.
      - Within 10 s the receiver gets ≥ 3 SETAZIMUTH datagrams.
      - The Doppler cell holds strategy A with `tx_hz == 144300000`.
      - `shown().status == Tracking`.
    - `steps_once_a_second`: over 3.5 s the receiver gets between 3 and 5 SETAZIMUTH datagrams (the first step is immediate).
    - `steady_tracking_sends_no_event_per_step`: after the first `Rot(Tracking)`, 4 more steps send no `ServiceEvent::Track` event.
    - `input_change_seen_next_step`: switching the input mode to `None` gives `shown().status == Stopped` and Doppler strategy None within 2.5 s.
    - `history_input_sends_nothing`: `life: false` gives no datagram in 2.5 s.
  - `integrations.rs`:
    - `track_sync_pushes_on_change_only`: the fake records `track_input` calls. Two frames without changes give one push; selecting a plane gives a second push with `rev` + 1.
    - `stale_shown_revision_ignored`: the fake answers `track_shown` with `rev` one behind and status Tracking after a Pause. `track_status` stays Stopped.
    - `shown_applied_when_revision_matches`.
  - `cat_service.rs`:
    - `real_services_start_stop_cat` is rewritten: `track_input` with a TRACK input on a cached plane and strategy A gives `"F 144300312"`-style traffic at the fake rigctld. Assert a line starting with `"F 1443"` and an `"I 144300000"` line. Stop and wait as in 4b.
  - `tracking.rs`: the tests that drove `track_tick` with a clock now assert on the pushed `TrackInput` (heights request once per Play; no params while `track.activate` is off; `life: false` in HISTORY).
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test track_service --test integrations`. Expected: FAIL.
- [ ] **Step 3: Implement.** Move the trait first (a pure move, its own commit `refactor(airscout): Services trait in its own file`), then the changes. README: replace the "Stage 4a limitation" bullet with "Tracking runs on the Track worker: outputs and the Doppler push continue while the window is minimised."
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS. Report `wc -l` of `app/services_api.rs` and `app/services_trait.rs` (each below 500) and `app/mod.rs` (≤ 761).
- [ ] **Step 5: Commit** `feat(airscout): tracking steps on the Track worker, so a minimised window keeps tracking`.

### Task 4: Track settings for serial and DDE, and the GS-232A output

**Files:**
- Create: `crates/airscout/src/rotator_gs232.rs`
- Modify:
  - `crates/as-settings/src/track.rs`, `crates/as-settings/src/lib.rs` (re-exports)
  - `crates/airscout/src/track_outputs.rs` (`OutputGroup::{Dde, Serial}`, `TrackConfig`, `build_outputs`)
  - `crates/airscout/src/track_service.rs` (`group_name`, the Serial double count)
  - `crates/airscout/src/app/integrations.rs` (`Disabled(Serial | Dde)`)
  - `Cargo.toml`, `crates/airscout/Cargo.toml` (`serial2.workspace = true`), `Cargo.lock`
- Test: `crates/as-settings/tests/settings_4c.rs`, `crates/airscout/tests/rotator_gs232.rs`, `crates/airscout/tests/track_service.rs`

**Interfaces (MapDlg.cs:7897-7962, :8036-8047, :8198-8250; Settings.settings:275-307):**
- Produces (`as-settings/src/track.rs`, `#[serde(default)]`):
  - `pub serial: TrackSerial = None` (`Track_Serial_None` :299). `pub enum TrackSerial { #[default] None, Gs232Az, Gs232AzEl }`, serialized `"none"`, `"gs232az"`, `"gs232azel"`.
  - `pub serial_port: String = "COM13"` (:278); `pub serial_baudrate: u32 = 4800` (:281).
  - `pub dde: TrackDde = None` (`Track_DDE_None` :305). `pub enum TrackDde { #[default] None, Hrd }`, serialized `"none"`, `"hrd"`.
- Produces (`rotator_gs232.rs`):
  - `pub const MAX_REPLY: usize = 256; pub const IO_TIMEOUT: Duration = Duration::from_millis(1000);` (`ReadTimeout`/`WriteTimeout`, :8044-8045).
  - `pub trait SerialIo: Read + Write + Send {}` with a blanket impl, and `pub type SerialOpen = Arc<dyn Fn(&str, u32) -> std::io::Result<Box<dyn SerialIo>> + Send + Sync>;`.
  - `pub fn serial2_open() -> SerialOpen`:
    - `serial2::SerialPort::open(name, baud)`: 8 data bits, no parity, 1 stop bit, no flow control (:8036-8041).
    - Then `set_read_timeout(IO_TIMEOUT)`, `set_write_timeout(IO_TIMEOUT)`, `set_dtr(false)`, `set_rts(false)` (the .NET defaults).
    - Check that `COM13` (above COM9) opens on Windows; prefix `\\.\` if serial2 does not.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Gs232 { Az, AzEl }`.
  - `pub fn query(k: Gs232) -> &'static str`: `"C"` / `"C2"`.
  - `pub fn set_command(k: Gs232, az: f64, el: f64) -> String`: `format!("M{}", zero3(az))` / `format!("W{} {}", zero3(az), zero3(el))` (`as_wintest::dotnet::zero3`, L9 1:1).
  - `pub fn reply_ok(s: &str) -> bool`: `s.starts_with("+0")`, then the characters 2..5 exist and parse with `parse_dotnet_float`.
  - `pub fn parse_dotnet_float(s: &str) -> Option<f64>`: `Convert.ToDouble(s, Invariant)` = `NumberStyles.Float | AllowThousands`. That is leading and trailing white space, an optional sign, digits with `,` group separators in the integral part, an optional `.` fraction, and an optional exponent. At least one digit.
  - `pub fn read_reply(r: &mut dyn Read, deadline: Instant) -> std::io::Result<String>`:
    - Reads bytes up to `\r`; `\r` is not returned; `\n` is removed; bytes > 0x7F become `?`.
    - More than `MAX_REPLY` bytes give an `InvalidData` error "reply too long".
    - `TimedOut`/`WouldBlock` past the deadline give `TimedOut`.
  - `pub struct Gs232Output`, `Gs232Output::new(k: Gs232, port: String, baud: u32, open: SerialOpen)`, `impl TrackOutput` with group `Serial`:
    - `connect`: open; on error remember `e.to_string()` and return `Err`.
    - `send(az, el)`:
      - Not open: `Failed(format!("Track error: {e}"))` with the remembered error (C# outer catch, :8318-8321; not counted).
      - Open: write `query + "\r"`, then `read_reply` with deadline `now + IO_TIMEOUT`.
      - An I/O error (timeout, too long, write failure) gives `Failed(e.to_string())`, and the fanout closes the port.
      - `!reply_ok` gives `NotConnected("[Serial]: Wrong serial data format.")` (counted; the port stays open).
      - Then write `set_command + "\r"` with no reply read.
    - `close` drops the port.
  - `OutputGroup` gains `Dde` and `Serial` (order `Dde, Udp, Serial, File`). `group_name`: `"DDE"`, `"UDP"`, `"Serial"`, `"file"`.
  - `TrackFanout`: for `Serial` a counted error does `n += 1`, then the `> MAX_ERRORS` check, then `n += 1` again (L18, :8232-8247). So the switch-off comes with the 6th error.
  - `TrackConfig` gains `serial: TrackSerial, serial_port: String, serial_baudrate: u32, serial_open: SerialOpen, dde: TrackDde`. `from_settings` sets `serial_open: serial2_open()`. `build_outputs` inserts `Gs232Output` between UDP and File (the 4a marker comment).
  - `on_track_event`: `Disabled(Serial)` sets `track.serial = None`; `Disabled(Dde)` sets `track.dde = None`.

- [ ] **Step 1: Write the failing tests** (a `FakeSerial` with a scripted input queue, an output `Vec<u8>` and scripted errors; the opener counts calls):
  - `settings_4c.rs`: `track_serial_dde_defaults_match_csharp`; `track_4b_file_loads_unchanged` (a default file without the new keys); `serial_enums_roundtrip`.
  - `rotator_gs232.rs`:
    - `commands_match_csharp`:
      - `set_command(Az, 123.4, 0.0) == "M123"`, `(Az, 359.6, _) == "M360"`, `(AzEl, 5.0, 45.2) == "W005 045"`, `(AzEl, 10.0, -5.0) == "W010 -005"`.
      - `query(Az) == "C"`, `query(AzEl) == "C2"`.
    - `reply_check`: `"+0123"`, `"+0123+0045"`, `"+0 12"`, `"+01.5"`, `"+01,2"`, `"+0-12"` are ok. `"?0123"`, `"+0"`, `"+012"`, `"+0abc"`, `"+0   "`, `""` are not.
    - `az_exchange_bytes`: reply `"+0123\r"`. The output bytes are exactly `b"C\rM123\r"`.
    - `azel_exchange_bytes`: reply `"+0123+0045\r\n"`. The output is `b"C2\rW123 045\r"`.
    - `non_ascii_reply_is_format_error`: reply `b"\xAB0123\r"` gives `NotConnected("[Serial]: Wrong serial data format.")`, and no `M` is written.
    - `long_reply_fails`: 300 × `b'1'` without `\r` gives `Failed(_)`.
    - `timeout_closes_and_reopens`: the reply read times out. That gives `Failed(_)`, then `is_connected() == false` after the fanout's close. The next step opens again (opener count 2).
    - `open_failure_is_shown_not_counted`: the opener returns `NotFound`. 20 steps through `TrackFanout` give 20 texts starting with `"Track error: "`, no `Disabled`, and `Rot(Tracking)` stays.
  - `track_service.rs`:
    - `serial_disabled_at_sixth_format_error`: replies `"?\r"` every step. Step 6 gives `Text("Tracking via Serial disabled.")`, `Rot(Error)`, `Disabled(OutputGroup::Serial)`. Steps 1-5 do not.
    - `outputs_in_csharp_order`: with UDP, Serial and File configured, `build_outputs` gives groups `[Udp, Serial, File]`.
    - `dde_ignored_off_windows` (`#[cfg(not(windows))]`): `dde = Hrd` adds no output.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings --test settings_4c` and `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test rotator_gs232 --test track_service`. Expected: FAIL.
- [ ] **Step 3: Implement.** Run one `cargo build -p airscout` without `--locked`. Check `cargo tree -p airscout -e normal -i serial2`: only `libc`/`cfg-if` (unix) and `windows-sys` (windows) are new below it.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): GS-232A serial rotator output and serial/DDE track settings`.

### Task 5: `as-dde` and the HRD rotator output (Windows)

**Files:**
- Create: `crates/as-dde/Cargo.toml` (no deps; `[target.'cfg(windows)'.dependencies] windows = { workspace = true, features = ["Win32_Foundation", "Win32_System_DataExchange"] }`), `crates/as-dde/src/lib.rs`, `crates/as-dde/src/ddeml.rs`
- Modify: `Cargo.toml` (`as-dde`, `windows = "0.62"`), `Cargo.lock`, `crates/airscout/Cargo.toml` (`as-dde.workspace = true`), `crates/airscout/src/track_outputs.rs` (`DdeOutput`, `hrd_pokes`, the `cfg(windows)` arm of `build_outputs`)
- Test: `crates/airscout/tests/track_dde.rs` (all OS, fake link), `crates/as-dde/tests/ddeml.rs` (`#![cfg(windows)]`)

**Interfaces (MapDlg.cs:7838-7861, :8018-8022, :8199-8215, :8332-8336; survey 6):**
- Produces (`as-dde`):
  - `pub const SERVICE: &str = "HRDRotator"; pub const TOPIC: &str = "Position"; pub const ITEM: &str = "PositionData"; pub const CF_TEXT: u32 = 1; pub const POKE_TIMEOUT: Duration = Duration::from_millis(10_000);`
  - `pub trait DdeLink: Send { fn connect(&mut self, service: &str, topic: &str) -> Result<(), String>; fn is_connected(&self) -> bool; fn poke(&mut self, item: &str, data: &[u8], format: u32, timeout: Duration) -> bool; fn disconnect(&mut self); }`
  - `#[cfg(windows)] pub struct Ddeml` (`Ddeml::new()`), `impl DdeLink` in `ddeml.rs`:
    - `connect`: `DdeInitializeW` (client only, a no-op callback) once; `DdeCreateStringHandleW` (`CP_WINUNICODE`) for service and topic; `DdeConnect`; free the string handles. Failure gives `Err(format!("DDE: {service}/{topic}: error {DdeGetLastError}"))`.
    - `poke`: `DdeClientTransaction(data, len, hconv, hsz(item), format, XTYP_POKE, timeout ms, null)`; true when it returns non-zero. The data is sent as given, without a NUL (1:1 `TryPoke`).
    - `disconnect`: `DdeDisconnect`. `Drop`: disconnect, then `DdeUninitialize`.
    - `unsafe impl Send` with the one-thread SAFETY note: the Track worker builds its outputs on its own thread, `track_service.rs`.
- Produces (`track_outputs.rs`):
  - `pub fn hrd_pokes(az: f64, el: f64) -> Vec<Vec<u8>>`: `b"SET-AZ:" + fixed(az, 1)` when `0 <= az <= 360`, then `b"SET-EL:" + fixed(el, 1)` when `0 <= el <= 90` (invariant `.`; C# current culture, finding).
  - `pub struct DdeOutput`, `DdeOutput::new(link: Box<dyn DdeLink>)`, `impl TrackOutput` with group `Dde`:
    - `connect` is `link.connect(SERVICE, TOPIC)`.
    - `send`: `!is_connected` gives `NotConnected("[DDE]: Client not connected.")` (counted, 1:1). Otherwise each poke goes out with `CF_TEXT` and `POKE_TIMEOUT`, its result is ignored (1:1), and the result is `Ok`.
    - `close` disconnects.
  - `build_outputs`: `#[cfg(windows)]`, if `c.dde == TrackDde::Hrd`, a `DdeOutput::new(Box::new(as_dde::Ddeml::new()))` comes first.

- [ ] **Step 1: Write the failing tests:**
  - `track_dde.rs` (`FakeLink` with scripted connect results and a log of `(item, data, format, timeout)`):
    - `hrd_poke_bytes`: `(123.45, 5.0)` gives `[b"SET-AZ:123.5", b"SET-EL:5.0"]` (rounding 1:1 with `fixed`). `(359.0, 95.0)` gives only the AZ poke. `(10.0, -1.0)` gives only `b"SET-AZ:10.0"`.
    - `pokes_use_cf_text_and_10s`: each log entry has `("PositionData", .., 1, 10 s)`.
    - `not_connected_is_counted_and_reconnected`: connect fails. Through `TrackFanout`, every step gives `Text("[DDE]: Client not connected.")`, and step 11 disables DDE (`Disabled(OutputGroup::Dde)`, `"Tracking via DDE disabled."`). The connect count equals the step count.
    - `late_server_is_picked_up`: connect fails twice, then works. Step 3 pokes.
    - `poke_failure_is_silent`: poke returns false; no text.
  - `ddeml.rs` (Windows): `connect_without_hrd_fails_cleanly`: `Ddeml::new().connect(SERVICE, TOPIC)` returns `Err(t)` with `t.starts_with("DDE: HRDRotator/Position: ")`; `is_connected() == false`; drop does not panic.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test track_dde`. Expected: FAIL.
- [ ] **Step 3: Implement.** One `cargo build` without `--locked` and commit `Cargo.lock`. Check `cargo tree -p as-dde -e normal` on macOS: no dependencies.
- [ ] **Step 4: Run** the gate, then the Windows check from Global Constraints. Expected: PASS; clippy clean for `x86_64-pc-windows-gnu`.
- [ ] **Step 5: Commit** `feat(as-dde): DDEML client and the Ham Radio Deluxe rotator output (Windows)`.

### Task 6: OmniRig `Rig` semantics over `OmniDispatch` (all OS)

**Files:**
- Create: `crates/as-cat/src/omnirig.rs`
- Modify: `crates/as-cat/src/lib.rs`
- Test: `crates/as-cat/tests/omnirig.rs`, `crates/as-cat/tests/common/fake_omni.rs`

**Interfaces (AirScout.CAT/OmniRigX.cs):**
- Consumes: `Rig`, `RigIdent`, `RigStatus`, `RigMode`, `RigSplit`, `RigRit`, `Doppler`, `DopplerSession` (4b).
- Produces (`omnirig.rs`):
  - The `OMNIRIGSTATUS` and `OMNIRIGPARAM` values as `pub const` (`:14-58`), for example `PM_FREQ = 0x2`, `PM_VFOEQUAL = 0x2000`, `PM_SPLITON = 0x8000`, `PM_SSB_U = 0x0200_0000`. Also `pub fn param_name(v: i32) -> Option<&'static str>` (`"PM_SSB_U"` …).
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum OmniProp { Status, Mode, Freq, FreqA, FreqB, Vfo, Split, Rit }`, with `fn name(self) -> &'static str` = the COM property name (`"Status"`, `"FreqA"` …).
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct OmniError(pub String);`
  - `pub trait OmniDispatch`:
    - `fn interface_version(&mut self) -> Result<i32, OmniError>; fn has_rig(&mut self, n: u8) -> bool;`
    - `fn get(&mut self, n: u8, p: OmniProp) -> Result<i32, OmniError>; fn set(&mut self, n: u8, p: OmniProp, v: i32) -> Result<(), OmniError>;`
    - `fn is_readable(&mut self, n: u8, param: i32) -> Result<bool, OmniError>; fn is_writeable(&mut self, n: u8, param: i32) -> Result<bool, OmniError>;`
    - `fn call_missing(&mut self, n: u8, name: &str, param: i32) -> Result<bool, OmniError>;` calls a method by name. It exists only so the 1:1 `IsWritable` call (a name OmniRig does not have) fails the same way the C# late binding does.
  - `pub type OmniOpen = Box<dyn FnMut() -> Result<Box<dyn OmniDispatch>, String> + Send>;`
  - `pub struct OmniRig`, `OmniRig::new(open: OmniOpen, rig: u8) -> Self`, `impl Rig`:
    - **Engine:** opened lazily at the first `get_status`. An open error gives `NoCat`, and `take_error` gives `format!("Error while initializing OmniRig: {e}")` (Port deviation of :87-152, :274-277). The open is retried on the next `get_status`.
    - **Rigs:** 4 when `has_rig(3) && has_rig(4)`, else 2 (D6; replaces the impossible tests of L16, :130-141, :229-237).
    - **`ident()`:** `cat_version` is `"OmniRig V2.x"` with 4 rigs and `"OmniRig V1.19"` with 2 (the C# strings, :109, :115); `rig_type` is `format!("[OmniRig] Rig {n}")` (:68-71). Before the open it is `"OmniRig"`.
    - **`get_status(s)` (:243-312):**
      - `rig > count` gives `NoRig`, and `take_error` gives `"OmniRig V1.19 supports Rig 1 and Rig 2 only."` (Port deviation).
      - Any `OmniError` gives `None` (:308-312, rig lost).
      - `Status` 2 gives NoPort; 0 or 1 give NoRig; 3 gives Offline.
      - Any other value goes on as online (switch fall-through, 1:1). For `s == A`: readable SPLITOFF and SPLITON, and writeable SPLITOFF, SPLITON and VFOEQUAL, else NotSuitable. Then Online.
    - **Getters** (any error gives NONE/0, 1:1 `catch {}`):
      - Mode: `PM_CW_L`→Cw, `PM_CW_U`→CwR, `PM_SSB_L`→Lsb, `PM_SSB_U`→Usb, `PM_DIG_L`→Dig, `PM_DIG_U`→DigR, `PM_AM`→Am, `PM_FM`→Fm (:315-330).
      - Split ON/OFF, RIT ON/OFF (:369-378, :418-427).
      - RX (:518-536): split OFF gives `Freq > 0 ? Freq : FreqA > 0 ? FreqA : 0`; otherwise `FreqA > 0 ? FreqA : 0`.
      - TX (:576-594): split OFF as RX; otherwise `FreqB > 0 ? FreqB : 0`.
    - **`set_mode`, `set_split`, `set_rit`:** a property write of the `PM_*` value; `None`/`Other` give false with no call (:636-651, :690-699, :739-748).
    - **`set_rx_frequency(hz)` / `set_tx_frequency(hz)` (:837-965):**
      - `hz <= 0` or `hz > i32::MAX` gives false with no call.
      - Otherwise `get(Split)`, then `call_missing(n, "IsWritable", PM_FREQ…)`. Its error gives false, and nothing is written (1:1 with the misspelled `rig.IsWritable`, finding).
    - **`set_simplex_mode(f, m)` (:979-1061):** false with no call for `f <= 0 || f > i32::MAX`. Otherwise the first matching branch, in C# order:
      1. FREQA, VFOAA: `Vfo=VFOAA, FreqA=f`.
      2. FREQA, VFOA, SPLITOFF: `Vfo=VFOA, FreqA=f`.
      3. FREQ, VFOA, VFOB: `Vfo=VFOB, Freq=f, Vfo=VFOA, Freq=f`.
      4. FREQ, VFOEQUAL: `Freq=f, Vfo=VFOEQUAL`.
      5. FREQ, VFOSWAP: `Vfo=VFOSWAP, Freq=f, Vfo=VFOSWAP, Freq=f`.
      6. FREQ, not VFOA, FREQB: `Freq=f, FreqB=f`.
      7. FREQ: `Freq=f`.
      - Then `Split=SPLITOFF` if SPLITOFF is writeable.
      - Then `Mode=<PM value of m>` if all 8 modes are writeable (`CanWriteMode`, :1090-1111) and `m` has one (L17 not reproduced: C# writes the `RIGMODE` number).
      - Any error gives false, else true.
    - **`set_split_mode(rx, tx, m)` (:1113-1257):** false with no call when either frequency is `<= 0` or `> i32::MAX`. Otherwise:
      - If VFOEQUAL is writeable and the mode can be written: `Mode=m, Vfo=VFOEQUAL`.
      - Then the first branch:
        1. FREQA, FREQB, VFOAB (TS-570): `Vfo=VFOAB, FreqA=rx, FreqB=tx`, then `Mode=m`.
        2. FREQ, VFOEQUAL (IC-746): `Freq=tx`, `Mode=m`, `Vfo=VFOEQUAL`, `Freq=rx`.
        3. VFOB, FREQ, VFOA (FT-100D): `Vfo=VFOB, Freq=tx, Mode=m, Vfo=VFOA, Freq=rx, Mode=m`.
        4. FREQ, VFOSWAP (FT-817): `Vfo=VFOSWAP, Freq=tx, Mode=m, Vfo=VFOSWAP, Freq=rx, Mode=m`.
        5. FREQA, FREQB, VFOA (FT-1000MP): `Vfo=VFOA, FreqA=rx, FreqB=tx, Mode=m`.
        6. FREQ, not FREQA, FREQB: `Freq=rx, FreqB=tx`.
      - Each `Mode=m` happens only when the mode can be written and `m != None`.
      - Then `Split=SPLITON` if writeable. Errors give false, else true.
    - **`take_error`:** the open or Rig 3/4 text, once.
  - `unsafe` is not needed here; `OmniRig: Send` follows from `OmniOpen` and `Box<dyn OmniDispatch + Send>`, so `OmniDispatch: Send`.
- `fake_omni.rs`:
  - `FakeOmni { version, rigs: u8, status, props: BTreeMap<OmniProp, i32>, readable: BTreeSet<i32>, writeable: BTreeSet<i32>, fail: BTreeSet<&'static str>, log: Arc<Mutex<Vec<String>>> }`.
  - Every `set` is logged as `"{Prop}={v}"`, where `v` is printed through `param_name` when it is a known param and as plain digits for frequencies (for example `"Vfo=PM_VFOEQUAL"`, `"Freq=144300000"`).
  - `call_missing` always errors.

- [ ] **Step 1: Write the failing tests** (`rig` 1; state Freq 144300000, Mode `PM_SSB_U`, Split `PM_SPLITOFF`, Rit `PM_RITOFF`, Status 4):
  - `status_mapping`: 0, 1 → NoRig; 2 → NoPort; 3 → Offline; 4 and 7 → Online (`unknown_status_value_is_online`).
  - `strategy_a_needs_split_and_vfoequal`: writeable lacks VFOEQUAL. A gives NotSuitable; B gives Online.
  - `open_error_is_nocat_with_text`: the opener errors with `"no class"`. NoCat and `"Error while initializing OmniRig: no class"`; the second open is attempted on the next `get_status`.
  - `rig3_on_v1_engine_is_norig`: `rigs: 2`, `OmniRig::new(.., 3)` gives NoRig with the V1 text; `ident().cat_version == "OmniRig V1.19"`.
  - `v2_engine_has_four_rigs`: `rigs: 4`, rig 4 gives Online and `"OmniRig V2.x"`, `"[OmniRig] Rig 4"`.
  - `com_error_on_status_is_none`.
  - `getters_follow_csharp`: split ON with FreqA 144300312 and FreqB 144300000 gives RX 144300312 and TX 144300000; split OFF with Freq 0 and FreqA 144300000 gives RX 144300000; an unknown mode value gives `RigMode::None`.
  - `set_frequency_never_writes`: `set_rx_frequency(144300000) == false`, and the log has no `Freq`.
  - `split_mode_ts570`: writeable `{FREQA, FREQB, VFOAB, SPLITON}`, `set_split_mode(144300312, 144300000, None)` logs `["Vfo=PM_VFOAB", "FreqA=144300312", "FreqB=144300000", "Split=PM_SPLITON"]`.
  - `simplex_freq_only`: writeable `{FREQ}` logs `["Freq=144300000"]`.
  - `limits`: `set_split_mode(3_000_000_000, 144300000, None)`, `set_simplex_mode(0, Usb)` and `set_mode(RigMode::Other)` are false with an empty log.
  - `golden_doppler_session_ic746`: writeable `{FREQ, VFOEQUAL, SPLITON, SPLITOFF}` plus all 8 mode params, readable `{SPLITON, SPLITOFF}`.
    - `DopplerSession::enter(A)` logs `["Mode=PM_SSB_U", "Vfo=PM_VFOEQUAL", "Freq=144300000", "Mode=PM_SSB_U", "Vfo=PM_VFOEQUAL", "Freq=144300000", "Split=PM_SPLITON"]`.
    - After clearing the log, `leave` logs `["Mode=PM_SSB_U", "Split=PM_SPLITOFF", "Rit=PM_RITOFF", "Freq=144300000", "Vfo=PM_VFOEQUAL", "Split=PM_SPLITOFF", "Mode=PM_SSB_U"]`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat --test omnirig`. Expected: FAIL.
- [ ] **Step 3: Implement.** Branch tables as data (a slice of `(needs, writes)`) are fine; the order above is binding.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-cat): OmniRig rig semantics over a dispatch seam`.

### Task 7: OmniRig COM backend (Windows), engine setting and `make_rig`

**Files:**
- Create: `crates/as-cat/src/omnirig_com.rs` (`#[cfg(windows)]`)
- Modify:
  - `crates/as-cat/Cargo.toml` (`[target.'cfg(windows)'.dependencies] windows = { workspace = true, features = ["Win32_Foundation", "Win32_System_Com", "Win32_System_Ole", "Win32_System_Variant"] }`), `crates/as-cat/src/lib.rs` (the unsafe attributes), `crates/as-cat/src/config.rs`
  - `crates/as-settings/src/cat.rs` (`CatEngine::OmniRig`, `omnirig_rig`, `lenient_engine`)
  - `crates/airscout/src/cat_service.rs` (`cat_config`)
  - `Cargo.lock`
- Test: `crates/as-cat/tests/omnirig_com.rs` (`#![cfg(windows)]`), `crates/as-settings/tests/settings_4c.rs`, `crates/as-cat/tests/worker.rs`, `crates/airscout/tests/cat_service.rs`

**Interfaces (OmniRigX.cs:87-148):**
- Produces:
  - `Engine::OmniRig { rig: u8 }`. `pub fn omnirig_available() -> bool { cfg!(windows) }`.
  - `make_rig`: `#[cfg(windows)]` gives `Some(Box::new(OmniRig::new(omnirig_com::opener(), rig)))`; elsewhere `None` (NORIG, generic text).
  - `omnirig_com.rs`:
    - `pub fn opener() -> OmniOpen`. Each call:
      1. `CoInitializeEx(None, COINIT_MULTITHREADED)` (S_FALSE is fine; a guard calls `CoUninitialize` on drop, on the same thread).
      2. `CLSIDFromProgID("OmniRig.OmniRigX")`, falling back to `{4FE359C5-A58F-459D-BE95-CA559FB4F270}` (:108-115).
      3. `CoCreateInstance(.., CLSCTX_LOCAL_SERVER | CLSCTX_INPROC_SERVER)` as `IDispatch`.
      4. Errors give `Err(format!("{hresult:?} {message}"))`.
    - `struct ComOmni`:
      - Holds the engine `IDispatch`, the `RigN` dispatch cache (filled by `has_rig`) and the guard.
      - `impl OmniDispatch`:
        - `interface_version`: `DISPATCH_PROPERTYGET "InterfaceVersion"`.
        - `has_rig(n)`: `GetIDsOfNames("Rig{n}")` succeeds and the property get returns `VT_DISPATCH`.
        - `get`/`set`: property get (`VT_I4`, coerced with `VariantChangeType`) and put (`DISPID_PROPERTYPUT`, `VT_I4`).
        - `is_readable`/`is_writeable`: `DISPATCH_METHOD "IsParamReadable"`/`"IsParamWriteable"` with one `VT_I4`, result `VT_BOOL`.
        - `call_missing`: the same call by name, so a missing name gives `OmniError`.
      - `unsafe impl Send` with the SAFETY note: created by the factory inside `CatWorker::pass` on the `cat` thread and dropped there.
    - Every `unsafe` block is commented. No `unsafe` outside this file and `hamlib.rs`.
  - `as-settings`:
    - `CatEngine::OmniRig` serialized `"omnirig"`; `pub omnirig_rig: u8 = 1`.
    - `lenient_engine` accepts any TOML value: a string is matched case-insensitively (`"rigctld"`, `"hamlib"`, `"omnirig"`), and any other string or a non-string gives `None`. Use an untagged helper `enum Raw { S(String), Other(serde::de::IgnoredAny) }`.
  - `cat_config`: `CatEngine::OmniRig` gives `Engine::OmniRig { rig: omnirig_rig.clamp(1, 4) }`.

- [ ] **Step 1: Write the failing tests:**
  - `settings_4c.rs`:
    - `engine_lenient_case_and_type`: `"OmniRig"` and `"RIGCTLD"` load as OmniRig and Rigctld; `engine = 42` and `engine = ["x"]` load as None, and the other `[cat]` keys still load.
    - `omnirig_rig_default_is_1`.
  - `cat_service.rs`: `omnirig_rig_clamped` (0 → 1, 9 → 4).
  - `worker.rs`: `omnirig_engine_off_windows_is_norig` (`#[cfg(not(windows))]`): `make_rig(Engine::OmniRig { rig: 1 })` is `None`, and `omnirig_available() == false`.
  - `omnirig_com.rs` (Windows CI, no OmniRig installed):
    - `com_open_without_omnirig_reports_nocat`: `OmniRig::new(opener(), 1).get_status(Doppler::None) == NoCat`, and `take_error()` starts with `"Error while initializing OmniRig: "`.
    - `#[ignore] omnirig_installed_roundtrip`: by hand on a machine with OmniRig. `get_status` is Online or Offline, and `ident().cat_version` is one of the two labels.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings -p as-cat -p airscout --test settings_4c --test worker --test cat_service`. Expected: FAIL.
- [ ] **Step 3: Implement.** One `cargo build` without `--locked`; commit `Cargo.lock` (on macOS it must not change the resolved `windows` version).
- [ ] **Step 4: Run** the gate, the hamlib feature check and the Windows check. Expected: PASS and clippy clean for `x86_64-pc-windows-gnu`.
- [ ] **Step 5: Commit** `feat(as-cat): OmniRig COM engine on Windows with V1 and V2 rigs`.

### Task 8: Options — Track serial/DDE groups, CAT OmniRig entries, `options/mod.rs` split

**Files:**
- Create: `crates/airscout/src/ui/options/fields.rs` (move `validate`, `validate_area`, `positive`, `non_negative`, `num`, `num_step`, `text` from `mod.rs` unchanged; `mod.rs` re-exports them under the same paths)
- Modify: `crates/airscout/src/ui/options/mod.rs`, `crates/airscout/src/ui/options/track.rs`, `crates/airscout/src/ui/options/cat.rs`
- Test: `crates/airscout/tests/ui_options_4c.rs` (tabs set with the `tab()` helper, not by click)

**Interfaces (OptionsDlg.cs:2617-2742, :2870-2930; OptionsDlg.Designer.cs:5112-5138, :5222-5281, :5671-5672; OptionsDlg.resx `richTextBox1.Text`):**
- `track.rs`:
  - `pub const BAUDRATES: [u32; 14] = [50, 100, 110, 150, 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200];` (:2670).
  - `validate` adds, always (1:1 `tc_Track_Validating`): `format!("Track: This is not a valid baudrate: {b}\nValid baudrates are:\n\n50, 100, 110, 150, 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200")` when `serial_baudrate` is not in `BAUDRATES` (:2671-2678).
  - Inside the existing `add_enabled_ui(t.activate, ..)`:
    - Group **"Rotator Control Serial Output"**: radios **"None"**, **"GS-232A Az only"**, **"GS-232A Az/El"**; **"Port:"** (free text; a device path on Linux/macOS); **"Baudrate:"** (number).
    - `#[cfg(windows)]` group **"Rotator Control DDE Output"**: radios **"None"**, **"Ham Radio Deluxe (HRDRotator.exe)"**.
  - The line "Serial (GS-232A) and DDE … not available yet." and the module doc sentence go.
- `cat.rs`:
  - `engines()` adds `(OmniRig, "[OmniRig] OmniRig V1.19 or V2.x (ActiveX)")` only when `as_cat::omnirig_available()`. An engine missing from the list shows `"[OmniRig] not available on this system"` for OmniRig, and the 4b hamlib text for Hamlib.
  - Group **"OmniRig Settings"** (only OmniRig): **"Rig:"** combo `"[OmniRig] Rig 1"` … `"[OmniRig] Rig 4"` (`omnirig_rig`), and the line "Rig 3 and Rig 4 need OmniRig V2.x." No port, baud or timeout fields (C# disables them for OmniRig, :2878-2896).
  - The engines list text gains, first, on Windows: `"1. OmniRig V1.19 or V2.x via ActiveX (Windows only)\n(multiple access to rig from different applications possible)"` (:5671-5672). The rigctld and hamlib lines renumber accordingly.
  - "Operating Instructions" appends on Windows, verbatim from `richTextBox1.Text`: `"  The  recommended settings for OmniRig are:\n\n- Polling interval:               100ms ...500ms\n- Timeout:                           500ms .. 1sec (essential for OmniRig)"`.
  - The module doc line "OmniRig and serial-rotator parts … arrive with stage 4c" goes.

- [ ] **Step 1: Write the failing tests:**
  - `invalid_baudrate_blocks_ok`: `serial_baudrate = 1234` makes `validate` contain exactly the text above with `1234`; 4800 gives no Track entry.
  - kittest `serial_group_sets_settings`: Track tab with `track.activate`; click "GS-232A Az/El", OK. Then `track.serial == TrackSerial::Gs232AzEl`.
  - kittest `serial_group_disabled_without_tracking`.
  - kittest `dde_group_only_on_windows`: the label "Ham Radio Deluxe (HRDRotator.exe)" exists iff `cfg!(windows)`.
  - kittest `no_not_available_yet_line`.
  - kittest `omnirig_engine_off_windows_shown_as_unavailable` (`#[cfg(not(windows))]`): engine OmniRig shows "[OmniRig] not available on this system", and no "[OmniRig] Rig 1" label.
  - kittest `omnirig_rig_choice` (`#[cfg(windows)]`): choose "[OmniRig] Rig 3", OK; `cat.omnirig_rig == 3`.
  - Update the 4b test `no_omnirig_or_serial_entries` to `#[cfg(not(windows))]` and drop its `[Serial]` assertion only if it conflicts with the new Track group (the CAT tab still has no `[Serial]` engine).
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test ui_options_4c`. Expected: FAIL.
- [ ] **Step 3: Implement.** Do the `fields.rs` move first as its own commit `refactor(airscout): Options field helpers in fields.rs`.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`, then the Windows check. Expected: PASS. Report `wc -l` of `ui/options/mod.rs` (below 500).
- [ ] **Step 5: Commit** `feat(airscout): Options serial and DDE rotator groups and OmniRig CAT entries`.

### Task 9: Docs, findings, gates

**Files:**
- Modify: `README.md`, `docs/logic-findings.md`, `.github/workflows/ci.yml` (no change expected; only if the Windows check showed a gap)

**Steps:**
- [ ] **Step 1: README:**
  - Status row `4c | GS-232A rotator, HRD DDE (Windows), OmniRig CAT (Windows), tracking on the Track worker | done`.
  - Desktop app, stage 4c:
    - GS-232A Az / Az/El: the port name (`COM13` on Windows, `/dev/ttyUSB0`-style on Linux/macOS), 4800 baud default, 8N1, once a second, `C`/`M123` and `C2`/`W123 045`.
    - The switch-off after the 6th format error (1:1, finding).
    - HRD DDE on Windows only.
    - OmniRig V1.19 (Rig 1-2) and V2.x (Rig 1-4) on Windows; OmniRig owns the serial port and its own timeouts.
    - "Not tested against a real OmniRig, HRD or GS-232 controller by CI" (until the owner confirms, Open question 3).
  - "Network sockets" gains the devices: `GS-232A | <track.serial_port>, serial | open | Track tab serial output not None`; `HRD | DDE service HRDRotator/Position (local) | connect | Windows, Track tab DDE output`; `OmniRig | COM server OmniRig.OmniRigX (local) | Windows, CAT engine OmniRig`.
  - Privacy: none of them carries calls, locators or positions; the rotator receives azimuth and elevation only.
  - Findings range "Stage 4c's are #235 to #N".
- [ ] **Step 2:** Add findings from **#235**, in this order. Each cites C# file:line and says Ported 1:1 / Not reproduced / Port deviation / New design.
  1. GS-232A exchange: `C`/`C2` query with a reply that must start with `+0` and parse in characters 2-4, then `M<000>`/`W<000> <000>` without a reply (MapDlg.cs:7897-7962). Ported 1:1; the queried position is never used.
  2. L9 for serial: `"000"` rounds (359.6 → `M360`) and a negative elevation writes `-005` (MapDlg.cs:7936, :7960). Ported 1:1 (D12).
  3. L18 serial: `serialerr` is incremented twice per error (MapDlg.cs:8234, :8246), so serial switches off with the 6th format error. Ported 1:1.
  4. Serial open failure: C# reports "Track error: …" and TRK ERROR from the outer catch and retries after `Track_Update`, re-opening a port that may still be open (L10, MapDlg.cs:8036-8046, :8315-8321). Port deviation: the text is shown, nothing is counted, TRK is not set to ERROR, and the port is closed before the next open (each step).
  5. Serial timeouts and I/O errors carry no `[Serial]:` prefix and are not counted (MapDlg.cs:7897-7913, :8232). Ported 1:1; Port deviation: the port is closed and reopened next step, so a late reply cannot be read as the next answer. A reply over 256 bytes fails (New design).
  6. Serial reply bytes ≥ 0x80 read as `?` (`Encoding.ASCII`, MapDlg.cs:8043). Ported 1:1. DTR and RTS are held low, as the .NET defaults (New design note).
  7. DDE `F1` uses the current culture (MapDlg.cs:7852, :7858), so a German system sends `123,4`. Not reproduced: invariant `.`, as the 4a outputs.
  8. DDE: the client connects once per pass and the `TryConnect` result is ignored, so HRD started later is never reached (MapDlg.cs:8020-8022). Port deviation: the output reconnects every step (D12). "[DDE]: Client not connected." is counted 1:1 (switch-off at the 11th), and `TryPoke` failures stay silent 1:1 (MapDlg.cs:7849-7860).
  9. Tracking steps on the Track worker. 3c-4b ran them in the UI frame loop, which a minimised window can stop. New design; `oldvalues` now starts empty after an Options restart, which is 1:1 again (a local of `bw_Track_DoWork`, MapDlg.cs:7997). The 4b Doppler snapshot rule (finding #224) is unchanged.
  10. L16: the OmniRig version tests are impossible (`< 0x210 && > 0x299`, OmniRigX.cs:134, :233, :1357), so V2 lists no rigs and Rig 3/4 are unreachable, and `InterfaceVersion == 0x110` is rejected (:130-141). Port deviation (D6): 4 rigs when the engine has `Rig3`/`Rig4`, else 2; no version is rejected.
  11. L17: `IntSetSimplexMode`/`IntSetSplitMode` write the `RIGMODE` number (USB = 4) into `rig.Mode` instead of `PM_SSB_U` (OmniRigX.cs:1032, :1123-1197). Not reproduced (per the owner's answer to Open question 4).
  12. `IntSetRxFrequency`/`IntSetTxFrequency` call `rig.IsWritable`, which OmniRig does not have, so they always fail and write nothing (OmniRigX.cs:842-857, :913-928). `LeaveDoppler` restores the frequency only through `SetSimplexMode`. Ported 1:1.
  13. An OmniRig start failure throws out of `GetStatus` (OmniRigX.cs:144-148, :276-277). The worker then reports NORIG and a stack trace and rebuilds the rig every pass (CATWorker.cs:301-306). Port deviation: NOCAT with "Error while initializing OmniRig: …" once.
  14. Rig 3/4 on a 2-rig engine return NONE (OmniRigX.cs:312): "rig lost" every pass. Port deviation: NORIG with a text.
  15. Unknown `Status` values fall through to the online path (OmniRigX.cs:245-266). Ported 1:1.
  16. The Options rig list reads the rig models from OmniRig on the UI thread (`CATWorker.SupportedRigs`, OptionsDlg.cs:2754, OmniRigX.cs:1312-1385). Port deviation: a fixed "[OmniRig] Rig 1-4" list; no model names.
  17. OmniRig frequencies are limited to 32 bit (OmniRigX.cs:873, :944, :1053, :1230). Ported 1:1, plus the ≤ 0 Hz rule.
  18. The two 4b parked restore cases: the C# loses the saved state with the rig (CATWorker.cs:175, :305) and calls `LeaveDoppler` on a null rig at exit (:311-318). Port deviation: the saved state and the pending leave survive a rig re-creation, and the exit re-opens a lost rig once to restore it.
- [ ] **Step 3:** Run the full gate on macOS, the hamlib feature check, the Linux docker gate and the Windows check (Global Constraints). Expected: green. Report the test counts. After the controller pushes the branch, the hosted Windows job must be green (it runs the `#[cfg(windows)]` tests).
- [ ] **Step 4: Commit** `docs: stage 4c README, devices and logic findings`.

## Self-review against the spec

- **GS-232A 1:1 (commands, timing, error handling):** Task 4: byte-exact exchanges, the 1 s timeouts, the doubled counter, the open-failure path; findings 1-6.
- **DDE to HRD, Windows only, absent elsewhere:** Task 5 (`as-dde` with `cfg(windows)` DDEML, a pure poke builder tested on all OS, a Windows smoke test); Task 4 `dde_ignored_off_windows`; Task 8 hides the group.
- **OmniRig CAT engine, V1 and V2, `CatEngine::OmniRig`, Options texts:** Tasks 6-8. The C# support for both versions is attempted but broken (L16); the port supports both (finding 10).
- **Tracking off the UI thread; the minimised-window limitation lifted:** Tasks 2-3, README in Task 3, finding 9.
- **Parked 4b restore cases, test-first in `worker.rs`:** Task 1.
- **Deferred minors folded:** `services_api.rs` (Task 3), `options/mod.rs` (Task 8), `lenient_engine` (Task 7).
- **Platform rule (spec non-goal):** every Windows module is `cfg(windows)`. COM and DDE sit behind `OmniDispatch` and `DdeLink`. The Windows check plus the hosted Windows job cover compilation and the smoke tests.
- **Unsafe isolated with SAFETY comments:** Global Constraints; only `omnirig_com.rs`, `ddeml.rs` (and 4b's `hamlib.rs`).
- **Type consistency:**
  - `TrackInput`, `TrackShown`, `TrackParams`, `Tracker`, `TrackInputCell`, `TrackShownCell` are defined in Tasks 2-3 and used with the same names.
  - `OutputGroup::{Dde, Serial}` (Task 4) is used by `DdeOutput` (Task 5).
  - `OmniDispatch`, `OmniOpen`, `OmniRig` (Task 6) are used by `omnirig_com::opener` (Task 7).
  - `Engine::OmniRig { rig: u8 }` and `CatEngine::OmniRig` are used in Tasks 7-8.
- **Proportion:** 9 tasks; code blocks are signatures, byte strings and branch orders only.

## Open questions (owner) and controller rulings

Owner decisions (2026-11-24): 1. `serial2` 0.2.39. 2. `windows` 0.62 (cfg(windows) only). 3. Merge on CI plus fakes; README marks OmniRig, HRD and GS-232 as untested on real devices. 4. L17 not reproduced (correct `PM_*` value, finding).

Original owner questions (recommendation first):

1. **Serial port crate.**
   - Recommended: `serial2` 0.2.39 (2026-10-26, BSD-2-Clause OR Apache-2.0, both GPL-3.0-compatible). Its dependencies are `libc` and `cfg-if` (unix) and `windows-sys` 0.61 (already in `Cargo.lock`). It needs no system library, so CI, the Linux docker gate and `ci-prepare.sh` stay unchanged.
   - Alternative: `serialport` 4.10.1 (MPL-2.0, GPL-3.0-compatible through its secondary-licence clause). It must use `default-features = false`; otherwise it needs `libudev-dev` on Linux, which changes CI and the docker gate. It pulls `nix` 0.26 and the macOS IOKit crates.
   - Alternative: a hand-written termios/Win32 port, which means more `unsafe`.
2. **Windows API crate for COM and DDE.**
   - Recommended: `windows` 0.62 (MIT OR Apache-2.0). 0.62.2 is already in `Cargo.lock` through the GUI stack. It is a `cfg(windows)` dependency only, with the features `Win32_Foundation`, `Win32_System_Com`, `Win32_System_Ole`, `Win32_System_Variant` and `Win32_System_DataExchange`. It needs no system library; the windows-gnu cross build already links it.
   - Alternative: `windows-sys` 0.61 (raw bindings without the `IDispatch`/`VARIANT` wrappers, so more `unsafe` code).
   - Alternative: a COM helper crate (not needed).
3. **Real-device verification.** CI can only prove the "not installed" paths of OmniRig and HRD, and no CI has a GS-232 controller.
   - Recommended: merge 4c on CI plus the fakes. The README marks the three as untested on real hardware. The owner runs the `#[ignore]` OmniRig test and a short manual HRD/GS-232 session on Windows when convenient. A result becomes a stage 6 item if needed.
   - Alternative: the stage stays open until the owner has tested it.
4. **OmniRig L17** (the `RIGMODE` number written as the rig mode, OmniRigX.cs:1032, :1123-1197). This affects the rig, so the port policy says 1:1.
   - Recommended: do not reproduce it; write the correct `PM_*` value. The C# effect is an undefined parameter write to a real radio during every Doppler enter and leave, which is the same class as 4b's 0 Hz rule.
   - Alternative: 1:1, writing `Mode = 1..8`.
   - The misspelled `IsWritable` (finding 12) stays 1:1 either way, because its C# effect is well defined: nothing is written.

Controller rulings (binding for the implementers; each with its cost if wrong):

- Ruling: the step clock moves to the Track worker; the UI pushes a revisioned `TrackInput` and applies only a matching `TrackShown` — the brief's goal (minimised window) needs the clock off the UI, and the revision stops a stale step from overriding Pause — cost if wrong: one more cell and one compare per frame.
- Ruling: the station heights stay a UI request answered by an event (3c/4a flow, finding #214), now issued from `track_input` when the mode becomes TRACK — it keeps `request_ground_elevations` and its tests — cost if wrong: a window minimised in the same second as the plane click waits with "no values" until it is shown again.
- Ruling: a GS-232A open failure is a shown, uncounted `Failed` ("Track error: …"), not a counted error — this is the C# outer-catch path; counting would switch a temporarily unplugged rotator off for good — cost if wrong: an absent port keeps printing one text per second instead of switching itself off.
- Ruling: after a serial timeout or I/O error the fanout closes and reopens the port (4a structure), while C# keeps it open — stale replies cannot pair with the next query — cost if wrong: some USB adapters toggle DTR on open; the port holds DTR low.
- Ruling: OmniRig V2 is detected by the presence of `Rig3`/`Rig4` on the engine, not by `InterfaceVersion` — the V2 interface number could not be verified from the sources, and the C# tests are impossible — cost if wrong: a V1 engine that exposes `Rig3` would offer an unusable rig, which then reports Offline or NORIG.
- Ruling: COM runs in a multithreaded apartment on the `cat` thread and the rig object never leaves it — the out-of-process OmniRig server is apartment-neutral — cost if wrong: a hung OmniRig blocks the `cat` thread; exit still waits at most `EXIT_WAIT` (2 s).
- Ruling: the Options OmniRig list is static ("[OmniRig] Rig 1-4", finding 16) — no COM on the UI thread — cost if wrong: users do not see the rig model name in Options; the CAT tooltip still names the rig.
- Ruling: `services_api.rs` is split by moving the `Services` trait to `services_trait.rs`, and `options/mod.rs` by moving the field helpers to `fields.rs` — pure moves, as the 4b ledger asked — cost if wrong: none.
