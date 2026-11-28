# Stage 4b — CAT over rigctld and the Doppler Push Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stage 4b of the spec, "Integrations", as split in the survey:
- A new crate `as-cat`. It has the `Rig` abstraction with the operations `CATWorker` uses, the Doppler enter/leave logic of `ScoutBaseRig`/`OmniRigX`, a rigctld TCP backend and a native hamlib backend behind the cargo feature `hamlib` (off by default).
- The CAT worker in the app. It starts, stops and restarts with the other integrations and reports a CAT status field.
- The Doppler push from the track step to the rig.
- The Options CAT tab, and the Doppler group of the Track tab enabled as upstream.

**Architecture:**
- `as-cat` has three layers, each tested on its own:
  - `Backend` is the hamlib-level primitive set (get/set frequency, mode, split, split frequency, split mode, RIT). `Rigctld` (TCP line protocol) and `NativeHamlib` (FFI, feature `hamlib`) implement it.
  - `CatRig<B: Backend>` implements the C# `IRig` semantics once on top of any backend (status mapping, getters that swallow errors, simplex and split sequences). `DopplerSession` holds the `EnterDoppler`/`LeaveDoppler` save and restore. It is identical in `ScoutBaseRig.cs:650-686` and `OmniRigX.cs:1273-1308`, so 4c's OmniRig reuses it.
  - `CatWorker` is the port of `CATWorker.OnDoWork` as a pure `pass()` plus a cancellable `run()` loop.
- The app gets `CatService`, a std thread `cat` in the style of `TrackService`. Its events go into `ServiceEvent::Cat` tagged with `AppState::integration_gen`. It starts and stops in `start_integrations`/`stop_integrations` (Options, HISTORY, exit), as `bw_CAT` does in `StartAllBackgroundWorkers`/`StopAllBackgroundWorkers` (MapDlg.cs:2437-2453).
- **Doppler push and the 4c move.**
  - The C# UI thread writes `bw_CAT.DopplerStrategy/RxFrequency/TxFrequency`, plain fields the CAT thread reads on its next poll (MapDlg.cs:8380-8401, :8419-8449).
  - The port keeps those semantics with a `DopplerCell = Arc<Mutex<DopplerTarget>>`: the writer stores the latest value, and the worker reads one snapshot per pass. Nothing is queued.
  - The pure function `cat_service::apply_step(&mut DopplerTarget, &TrackStep, Doppler)` holds the C# rules.
  - In 4b, `RealServices::track_step` calls `apply_step`. The track step still runs in the UI frame loop (4a ruling); the call is one mutex store, with no I/O.
  - In 4c, when the stepping moves into `TrackService`, that thread gets a clone of the cell and calls the same `apply_step` after each step. `RealServices::track_step` then no longer touches the cell. The 4c step needs `doppler.dial_freq_hz`; the app pushes it the same way. Nothing in `as-cat` changes.

**Tech Stack:**
- Rust 1.95+, std threads (the workspace has no tokio), `std::net::TcpStream`.
- `thiserror` (workspace). No new external crate.
- Native hamlib: a hand-written minimal `extern "C"` binding to libhamlib ≥ 4.0. It uses an opaque `RIG*` and string configuration tokens, so it mirrors no C structs, needs no bindgen and no `build.rs`. `cargo check`/`clippy` with `--features hamlib` does not link, so it runs on hosts without libhamlib.
  - Existing crates were rejected because the binding needs bindgen headers at build time, so even a check needs a host install.
- eframe/egui 0.37, egui_kittest 0.37 (dev), tempfile (dev).

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`:
- the stage 4 "Integrations" item;
- the `as-cat` crate row: "rigctld TCP (all OS), optional hamlib FFI, OmniRig only `cfg(windows)`";
- "rigctld over TCP as the primary CAT path on all OS; native hamlib FFI and OmniRig optional".

**Inputs:**
- Survey: `docs/superpowers/surveys/2026-11-24-integrations-survey.md`, sections 1, 4, 8, 9 (L14, L15, L21, L22) and "Decisions (project owner, 2026-11-24)". Binding:
  - D5: engine, host and port settings.
  - D6: OmniRig is 4c, not here.
  - D7: no ScoutBase.CAT INI engine and no rig download.
  - D8: hamlib FFI optional, off by default; rigctld primary.
  - D12.
- Ledger `docs/superpowers/ledgers/2026-11-24-stage4a-wintest-track.md`. Carried over:
  - The track step stays in the UI frame loop until 4c (see Architecture).
  - A failed thread spawn must send an event (Task 5).
  - Connect error texts must reach the status line (Tasks 1 and 3).
  - kittests set the tab directly, because clicks do not switch tabs (Task 7).
  - The owed finding "a Disabled event is lost when Options opens" (Task 8).
- Upstream: `target/upstream/AirScout/` @ `91b02cb`. It is read-only: never modify or delete it. C# references are relative to `target/upstream/AirScout/AirScout/` unless they start with `AirScout.CAT/` or `ScoutBase/`.

**Stage split:**
- **4b (this plan):** `as-cat` (rigctld, optional hamlib), `CatService`, the CAT field, the Doppler push, the CAT tab and the Doppler group enablement.
- **4c:** GS-232A, `as-dde`, OmniRig as its own `Rig` implementation (it has its own capability model and reuses `DopplerSession`), and the move of the track stepping into `TrackService` (see Architecture).
- **Not ported (findings, Task 8):**
  - the ScoutBase.CAT engine, `bw_CATUpdater`, the Rig database LED, `Rig_Directory` (D7);
  - `CAT_RigType`, which is replaced by `engine` + `hamlib_model` (D5);
  - `CAT_Poll` (ScoutBase engine only) and `CAT_ResponseTimeout` (never read);
  - `Doppler_DialMode/Split/Rit` (written, never read);
  - `GetVfo`/`SetVfo` and PTT, which the worker never calls.

## Global Constraints

- **Port policy:**
  - Behaviour that affects the rig or the prediction is ported 1:1, even when it is wrong, and logged in `docs/logic-findings.md`. The next finding number is **#216** (check the last row before you write). Task 8 assigns the numbers in the order of its list.
  - Integration-layer and GUI-layer upstream bugs are **not** reproduced. Each still gets a finding marked "Not reproduced", "Port deviation" or "New design".
  - Hard rule: **no frequency ≤ 0 Hz is ever written to a rig** (C# writes 0 Hz in `LeaveDoppler` without a saved state and with a zero dial).
  - D12 rounding and clamping stay 1:1: the `(long)` truncation of each Doppler in `rx_tx` (3c) is unchanged.
- **Scope:**
  - No OmniRig, ScoutBase.CAT, rig download, serial-rotator or DDE code.
  - No stub UI for OmniRig. The CAT engine list has no `[OmniRig]` or `[Serial]` entry.
  - The hamlib engine appears in the UI only when the feature is built.
- Pixel-identical UI is a non-goal.
- **Anti-goal:** the UI must not stutter or block.
  - No socket, FFI or DNS call runs on the UI thread. The UI thread only stores into the `DopplerCell` and reads events.
  - The one exception is exit: `on_exit` waits at most 2 s (`EXIT_WAIT`) so the worker can restore the rig.
- **Secrets:**
  - No secrets in the repo.
  - Credentials come only from user settings.
  - Secrets never appear in logs, `Debug` output, status lines, error texts or the User-Agent.
- **Network:**
  - Every HTTP request uses `as_net::user_agent()`. 4b adds no HTTP.
  - Requests to outside services never carry the user's call sign or position beyond what the API needs.
  - `getkey.php`, session keys, register, upload and plugin download are not ported.
  - Non-public or credential endpoints are off by default and need a disclaimer.
  - Bulk downloads run only on an explicit user action.
  - Self-hosted CI never runs fork PR code.
  - **The one new socket in 4b** is documented in the README (Task 8): outbound TCP to `<cat.rigctld_host>:<cat.rigctld_port>` (default `localhost:4532`). It is open only while `cat.activate` is on and the engine is rigctld.
    - It is plain text without authentication (rigctld has none).
    - It carries frequency, mode, split and RIT commands only, never calls, locators or positions.
  - Native hamlib opens the configured device itself. AirScout opens no socket for it.
  - Every rigctld reply is untrusted input:
    - Parsing never panics.
    - A reply line is at most `MAX_LINE` = 256 bytes.
    - Nothing allocates in proportion to a number in a reply.
    - After a timeout or a protocol error the connection is closed, so a late reply cannot be read as the answer to the next command.
- **Settings:**
  - TOML via `as-settings`.
  - New fields get `#[serde(default)]` defaults from `AirScout.CAT/Properties/Settings.Designer.cs` (lines in Task 5).
  - A stage 4a file (no `cat` table) loads unchanged.
  - An unknown `cat.engine` value (for example `"omnirig"` from a 4c build) loads as `none`, not as a settings error.
- **Unsafe:**
  - Every crate keeps `#![forbid(unsafe_code)]`, except `as-cat` built with `--features hamlib`.
  - There `lib.rs` has `#![cfg_attr(not(feature = "hamlib"), forbid(unsafe_code))]` and `#![cfg_attr(feature = "hamlib", deny(unsafe_code))]`. Only `#[allow(unsafe_code)] mod hamlib;` may contain `unsafe`, and every block has a `// SAFETY:` comment.
- **Threads:**
  - The worker thread is named `cat`.
  - `stop()` sets the cancel flag and returns at once. Sleeps are cut into steps of at most 250 ms.
  - One pass is bounded by the connect timeout plus one read timeout, because the backend closes after the first timeout and later commands fail at once.
  - On cancel the worker leaves Doppler (restores the rig) and reports `RigStatus::None`. `Drop` cancels.
- **Cargo:**
  - Every cargo command carries the prefix `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
  - Total disk use stays under 120 GB. Before each task, check `du -sh target`. Above 40 GB, run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clean --profile dev` (for the docker dir, add `--target-dir target/linux`).
  - **Never** run plain `cargo clean`: it deletes `target/upstream`.
- **Gate:**
  - Command: `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo fmt --all --check && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy --workspace --all-targets --locked -- -D warnings && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace --locked`.
  - Feature check (type-checks the FFI without linking; no libhamlib needed): `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy -p as-cat -p airscout --all-targets --locked --features as-cat/hamlib,airscout/hamlib -- -D warnings`. Tasks 4, 7 and 8 run it.
  - CI and the default gate do **not** enable `hamlib` and need no host install. CI is unchanged.
  - Linux docker gate: `docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target/linux -e CARGO_BUILD_JOBS=2 -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 rust:1.95 bash -c 'apt-get update && apt-get install -y libasound2-dev pkg-config && rustup component add rustfmt clippy && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked && cargo clippy -p as-cat -p airscout --all-targets --locked --features as-cat/hamlib,airscout/hamlib -- -D warnings'`. `target/linux` counts toward the disk budget.
  - CI must stay green on self-hosted Linux, the `x86_64-pc-windows-gnu` cross build, and hosted macOS/Windows.
  - A task that adds a crate or dependency runs one `cargo build` without `--locked` first and commits the updated `Cargo.lock`.
- **Tests:**
  - No display (`egui_kittest` without snapshots).
  - No network beyond 127.0.0.1.
    - The fake rigctld binds `127.0.0.1:0` and the test reads the port back.
    - A "refused" port is a port that was bound to `127.0.0.1:0`, read and released.
  - Waits are bounded generously (10 s) and return early on the awaited line or event. Windows CI takes about 2 s per refused connect.
  - Generic data only: frequencies 144 300 000 Hz and the derived values below, calls `TE1ST`/`TE2ND`, hosts `127.0.0.1`/`localhost`.
  - The fake rigctld lives in `crates/as-cat/tests/common/fake_rigctld.rs`. App tests include it with `#[path = "../../as-cat/tests/common/fake_rigctld.rs"] mod fake_rigctld;`, so no test-only feature is needed.
- **File size:** new files stay under about 500 lines. `app/mod.rs` (751 lines) grows by at most about 15; the app logic goes into `app/cat.rs`.

## Review Focus

1. **rigctld is not running, dies or restarts mid-session** (refused connect, EOF, a half-open socket).
   - Expected: CAT turns red (NOCAT) with one status text, not one per poll. The worker reconnects every poll without a busy loop or UI stall. Doppler re-engages once the rig answers.
   - Tests: Task 2 `refused_connect_is_nocat`, `eof_is_lost`, `silent_server_times_out_and_reconnects`; Task 3 `rig_lost_recreates`, `doppler_enter_retried_until_online`; Task 5 `refused_rigctld_reports_nocat_once`.
2. **rigctld replies unexpectedly** (`RPRT -n`, a slow rig, garbage, a 300-byte line, `\r\n`, a decimal frequency, a mode outside the eight).
   - Expected: no panic and no desync. The rig shows OFFLINE or reconnects; the getters fall back to NONE/0.
   - Tests: Task 2 `rprt_error_is_rejected`, `long_line_is_lost`, `garbage_freq_is_lost`, `decimal_and_crlf_freq_accepted`, `parse_never_panics_on_random_bytes`; Task 1 `mode_mapping`.
3. **Tracking ends, Options opens, HISTORY is entered or the app exits while Doppler is active.**
   - Expected: the rig is restored exactly once (saved mode, split, RIT and frequencies, then simplex 1:1). It is never tuned to 0 Hz or to a stale frequency. Exit waits at most 2 s.
   - Tests: Task 1 `leave_twice_restores_once`, `leave_without_enter_sends_nothing`, `zero_frequency_never_written`; Task 5 `service_pushes_doppler_and_restores_on_stop`, `real_services_start_stop_cat`; Task 6 `exit_waits_for_cat`.
4. **The dial is 0, CAT is offline when tracking starts, or the strategy is None.**
   - Expected: no 0 Hz push. Doppler engages once the rig is online. The dial is not overwritten by the rig while TRACKING (the rig shows the shifted frequency then).
   - Tests: Task 5 `apply_step_zero_freq_keeps_old`; Task 3 `zero_target_not_pushed`, `doppler_enter_retried_until_online`; Task 6 `rig_info_not_applied_while_tracking`.
5. **Settings from another build or edited by hand** (engine `hamlib` in a default build, `omnirig` from 4c, port 0, an empty host, `update_ms = 0`).
   - Expected: the file loads. The worker reports NORIG with the generic text and does not busy-loop. Options shows the hamlib engine as unavailable and blocks OK for an empty host or port 0.
   - Tests: Task 5 `unknown_engine_loads_as_none`, `cat_config_clamps`; Task 3 `hamlib_engine_without_feature_is_norig`; Task 7 `rigctld_port_zero_blocks_ok`, `hamlib_engine_in_default_build_shown_as_unavailable`.

---

### Task 1: Crate `as-cat` — types, `Backend`, `CatRig`, `DopplerSession`

**Files:**
- Create: `crates/as-cat/Cargo.toml` (deps: `thiserror`; `[features] hamlib = []`), `crates/as-cat/src/lib.rs`, `crates/as-cat/src/backend.rs`, `crates/as-cat/src/rig.rs`, `crates/as-cat/src/modes.rs`
- Modify: `Cargo.toml` (`as-cat = { path = "crates/as-cat" }` in `[workspace.dependencies]`), `Cargo.lock`
- Test: `crates/as-cat/tests/rig.rs`, `crates/as-cat/tests/common/fake_backend.rs`

**Interfaces (AirScout.CAT/Enums.cs, IRig.cs, ScoutBaseRig.cs:103-686):**
- Produces (`lib.rs`; all `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]` with `#[default]` on the first variant):
  - `pub enum RigStatus { None, NoCat, NoPort, NoRig, NotSuitable, Offline, Online, Error }`.
    - `fn name(self) -> &'static str`: `"NONE"`, `"NOCAT"`, `"NOPORT"`, `"NORIG"`, `"NOTSUITABLE"`, `"OFFLINE"`, `"ONLINE"`, `"ERROR"`.
    - `fn value(self) -> u8`: 0, 1, 2, 4, 8, 16, 64, 128.
  - `pub enum RigMode { None, Cw, CwR, Lsb, Usb, Dig, DigR, Am, Fm, Other }`. `name()`: `"NONE"`, `"CW"`, `"CW_R"`, `"LSB"`, `"USB"`, `"DIG"`, `"DIG_R"`, `"AM"`, `"FM"`, `"OTHER"`.
  - `pub enum RigSplit { None, SplitOff, SplitOn }`, `pub enum RigRit { None, RitOn, RitOff }`.
  - `pub enum Doppler { None, A, B, C, D }`. `value()`: 0, 1, 2, 4, 8. `fn is_active(self) -> bool` (A..D).
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct RigIdent { pub cat_version: String, pub rig_type: String }`.
- `modes.rs`:
  - `pub fn mode_from_hamlib(s: &str) -> RigMode`: `CW`→Cw, `CWR`→CwR, `LSB`→Lsb, `USB`→Usb, `PKTLSB`→Dig, `PKTUSB`→DigR, `AM`→Am, `FM`→Fm. Anything else gives `None` (1:1: ScoutBaseRig `GetMode` returns NONE for other modes).
  - `pub fn mode_to_hamlib(m: RigMode) -> Option<&'static str>` is the inverse; `None` and `Other` give `None`.
- `backend.rs`:
  - `#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)] pub enum BackendError { #[error("{0}")] NoCat(String), #[error("{0}")] NoPort(String), #[error("the rig rejected the command (RPRT {0})")] Rejected(i32), #[error("no answer from the rig")] Timeout, #[error("{0}")] Lost(String) }`.
  - `pub trait Backend: Send`:
    - `fn ident(&self) -> RigIdent;`
    - `fn connect(&mut self) -> Result<(), BackendError>;` (`Ok` at once when connected)
    - `fn close(&mut self);`
    - `fn get_freq(&mut self) -> Result<i64, BackendError>;` and `fn set_freq(&mut self, hz: i64) -> Result<(), BackendError>;`
    - `fn get_mode(&mut self) -> Result<String, BackendError>;` and `fn set_mode(&mut self, mode: &str) -> Result<(), BackendError>;`
    - `fn get_split(&mut self) -> Result<bool, BackendError>;` and `fn set_split(&mut self, on: bool) -> Result<(), BackendError>;` (TX VFO B when on, A when off)
    - `fn get_split_freq(&mut self) -> Result<i64, BackendError>;`, `fn set_split_freq(&mut self, hz: i64) -> Result<(), BackendError>;`, `fn set_split_mode(&mut self, mode: &str) -> Result<(), BackendError>;`
    - `fn get_rit_on(&mut self) -> Result<bool, BackendError>;` and `fn set_rit_on(&mut self, on: bool) -> Result<(), BackendError>;`
    - Every operation except `connect` returns `Lost("not connected")` while closed.
- `rig.rs`:
  - `pub trait Rig: Send`, the `IRig` subset `CATWorker` and `EnterDoppler`/`LeaveDoppler` use:
    - `fn ident(&self) -> RigIdent; fn get_status(&mut self, strategy: Doppler) -> RigStatus; fn take_error(&mut self) -> Option<String>;`
    - `fn get_mode(&mut self) -> RigMode; fn get_split(&mut self) -> RigSplit; fn get_rit(&mut self) -> RigRit; fn get_rx_frequency(&mut self) -> i64; fn get_tx_frequency(&mut self) -> i64;`
    - `fn set_mode(&mut self, m: RigMode) -> bool; fn set_split(&mut self, s: RigSplit) -> bool; fn set_rit(&mut self, r: RigRit) -> bool; fn set_rx_frequency(&mut self, hz: i64) -> bool; fn set_tx_frequency(&mut self, hz: i64) -> bool;`
    - `fn set_simplex_mode(&mut self, hz: i64, mode: RigMode) -> bool; fn set_split_mode(&mut self, rx: i64, tx: i64, mode: RigMode) -> bool;`
  - `pub struct CatRig<B: Backend>`. `CatRig::new(b: B) -> Self`, `fn backend(&self) -> &B`, `impl<B: Backend> Rig for CatRig<B>`:
    - `get_status(s)` (ScoutBaseRig.cs:103-141):
      - `connect()`: `NoCat(t)` gives NoCat and `NoPort(t)` gives NoPort; both store `t` for `take_error`. Any other error gives None.
      - Then `get_freq()`: `Ok` gives Online, but for `s == A` it calls `get_split()` first: `Lost` gives None, any other error gives NotSuitable (only A is checked, 1:1). `Rejected`/`Timeout` give Offline. `Lost(t)` gives None and stores `t`.
    - Getters swallow errors (C# `catch {}`): mode → `mode_from_hamlib` or None; split → SplitOn/SplitOff/None; RIT → RitOn/RitOff/None; RX → `get_freq` or 0; TX → `get_split() == Ok(true)` ? `get_split_freq` : `get_freq`, or 0.
    - `set_mode(m)` with no hamlib name sends nothing and returns false. `set_split(None)` and `set_rit(None)` likewise.
    - `set_rx_frequency(hz)`: `hz <= 0` gives false with no call; otherwise `set_freq`.
    - `set_tx_frequency(hz)`: `hz <= 0` gives false; otherwise `set_split_freq` when `get_split() == Ok(true)`, else `set_freq`.
    - `set_simplex_mode(hz, m)`: `hz <= 0` gives false with no call. Otherwise `set_split(false)`, `set_freq(hz)`, then `set_mode` if `m` has a name. True when the first two succeed.
    - `set_split_mode(rx, tx, m)`: `rx <= 0 || tx <= 0` gives false with no call. Otherwise `set_split(true)`, `set_freq(rx)`, `set_split_freq(tx)`, then, if `m` has a name, `set_mode(m)` and `set_split_mode(m)`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct Saved { pub mode: RigMode, pub split: RigSplit, pub rit: RigRit, pub rx_hz: i64, pub tx_hz: i64 }`.
  - `#[derive(Default)] pub struct DopplerSession`, with `fn saved(&self) -> Option<Saved>`:
    - `fn enter(&mut self, rig: &mut dyn Rig, s: Doppler) -> bool` (`EnterDoppler`, :650-666):
      - `rig.get_status(s) != Online` gives false.
      - Otherwise, if nothing is saved yet, save mode, split, RIT, RX and TX in that order. A saved state is kept, never overwritten.
      - Then `rig.set_split_mode(rx, rx, mode)` and true.
    - `fn leave(&mut self, rig: &mut dyn Rig)` (`LeaveDoppler`, :668-686):
      - Take the saved state. Without one, send nothing (Port deviation; C# calls `SetSimplexMode(0, NONE)`).
      - With one: `set_mode` if not None, `set_split` if not None, `set_rit` if not None, `set_rx_frequency(rx)` if > 0, `set_tx_frequency(tx)` if > 0, then `set_simplex_mode(rx, mode)` (1:1: this switches split off again).
      - The state is cleared, so a second `leave` sends nothing (Port deviation).

- [ ] **Step 1: Write the failing tests** (`rig.rs`). The `FakeBackend` keeps a rig state (default `freq 144_300_000`, `split_freq 144_300_000`, mode `"USB"`, split off, RIT off), a script of errors per operation, and a log of `Op` (`Connect, GetFreq, SetFreq(i64), GetMode, SetMode(String), GetSplit, SetSplit(bool), GetSplitFreq, SetSplitFreq(i64), SetSplitMode(String), GetRit, SetRit(bool)`); every call is logged.
  - `enter_saves_state_and_sets_split_rx_rx`: `enter(&mut rig, Doppler::A) == true`. The ops are `[Connect, GetFreq, GetSplit, GetMode, GetSplit, GetRit, GetFreq, GetSplit, GetFreq, SetSplit(true), SetFreq(144300000), SetSplitFreq(144300000), SetMode("USB"), SetSplitMode("USB")]`, and `saved() == Some(Saved { mode: Usb, split: SplitOff, rit: RitOff, rx_hz: 144300000, tx_hz: 144300000 })`.
  - `leave_restores_then_simplex`: after `enter`, set the fake to freq 144300312, split on, split freq 144300000, then clear the log. `leave` gives `[SetMode("USB"), SetSplit(false), SetRit(false), SetFreq(144300000), GetSplit, SetFreq(144300000), SetSplit(false), SetFreq(144300000), SetMode("USB")]`.
  - `leave_twice_restores_once`: the second `leave` logs nothing.
  - `leave_without_enter_sends_nothing`.
  - `enter_refused_when_offline`: `GetFreq` scripted to `Timeout`. `enter` is false, the ops are `[Connect, GetFreq]`, and `saved() == None`.
  - `status_mapping`: connect `NoCat("x")` → NoCat and `take_error() == Some("x")`, then `None` on the second call; connect `NoPort` → NoPort; `GetFreq` `Rejected(-5)` → Offline; `Timeout` → Offline; `Lost` → None; `Ok` → Online.
  - `strategy_a_needs_split`: `GetSplit` scripted to `Rejected(-11)`. A gives NotSuitable; B gives Online with no `GetSplit` op.
  - `getters_swallow_errors`: every get scripted to `Rejected(-1)`. The getters give None, None, None, 0, 0.
  - `tx_frequency_uses_split_freq_when_split_on`: split on, split freq 144300500 gives `get_tx_frequency() == 144300500`.
  - `zero_frequency_never_written`: `set_rx_frequency(0)`, `set_tx_frequency(-1)`, `set_simplex_mode(0, Usb)`, `set_split_mode(0, 144300000, None)` and `set_split_mode(144300000, 0, None)` are all false, and the log is empty.
  - `set_mode_none_or_other_sends_nothing`.
  - `mode_mapping`: the eight pairs both ways; `"RTTY"`, `"WFM"`, `""` and `"usb"` give `RigMode::None`.
  - `enums_match_csharp`: the eight `RigStatus` `(name, value)` pairs, and `Doppler` values 0, 1, 2, 4, 8.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat`. Expected: FAIL (crate items missing).
- [ ] **Step 3: Implement** the items above. Run one `cargo build -p as-cat` without `--locked` to add the crate to `Cargo.lock`. `lib.rs` carries the two `cfg_attr` unsafe lines from Global Constraints.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-cat): rig types, CatRig semantics and Doppler session`.

### Task 2: rigctld TCP backend and the fake rigctld

**Files:**
- Create: `crates/as-cat/src/rigctld.rs` (backend), `crates/as-cat/src/rigctld_proto.rs` (pure encode and parse), `crates/as-cat/tests/common/fake_rigctld.rs`
- Modify: `crates/as-cat/src/lib.rs`
- Test: `crates/as-cat/tests/rigctld.rs`

**Interfaces (hamlib net protocol; new design, survey 4.8):**
- Consumes: `Backend`, `BackendError`, `CatRig`, `DopplerSession`, `RigIdent` (Task 1).
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct RigctldConfig { pub host: String, pub port: u16, pub timeout: Duration }`.
  - `pub struct Rigctld`, built by `Rigctld::new(c: RigctldConfig)`, with `fn is_connected(&self) -> bool`, and `impl Backend for Rigctld`.
  - `pub fn rigctld_rig(c: RigctldConfig) -> CatRig<Rigctld>`.
  - `ident()`: `cat_version "Hamlib rigctld (net protocol)"`, `rig_type format!("[rigctld] {host}:{port}")`.
  - `connect`:
    - Resolve `(host, port)` with `ToSocketAddrs`; this runs on the worker thread.
    - Try each address with `TcpStream::connect_timeout(timeout)`. Set `set_nodelay(true)` and read and write timeouts = `timeout`.
    - A resolve or connect failure gives `NoCat(format!("rigctld {host}:{port}: {e}"))`.
  - Commands, exact bytes, one per call:

    | op | sent | reply |
    |---|---|---|
    | `get_freq` | `f\n` | 1 line |
    | `set_freq(h)` | `F {h}\n` | `RPRT n` |
    | `get_mode` | `m\n` | 2 lines (mode, passband; the passband is read and ignored) |
    | `set_mode(m)` | `M {m} -1\n` (passband -1 = no change, hamlib ≥ 4.0) | `RPRT n` |
    | `get_split` | `s\n` | 2 lines (`0`/`1`, TX VFO ignored) |
    | `set_split(true)` / `(false)` | `S 1 VFOB\n` / `S 0 VFOA\n` | `RPRT n` |
    | `get_split_freq` | `i\n` | 1 line |
    | `set_split_freq(h)` | `I {h}\n` | `RPRT n` |
    | `set_split_mode(m)` | `X {m} -1\n` | `RPRT n` |
    | `get_rit_on` | `u RIT\n` | 1 line (`0`/`1`) |
    | `set_rit_on(b)` | `U RIT {0\|1}\n` | `RPRT n` |
  - `rigctld_proto.rs`:
    - `pub const MAX_LINE: usize = 256;`
    - `pub fn read_line(r: &mut impl BufRead) -> Result<String, BackendError>`:
      - Reads up to `\n`; a trailing `\r` is stripped.
      - More than `MAX_LINE` bytes, non-UTF-8 or EOF give `Lost` (EOF: `"rigctld closed the connection"`).
      - `WouldBlock`/`TimedOut` give `Timeout`.
    - `pub fn parse_rprt(line: &str) -> Option<i32>`: `"RPRT -11"` gives `Some(-11)`.
    - `pub fn parse_freq(line: &str) -> Result<i64, BackendError>`: an integer or a decimal, finite, rounded half away from zero (`f64::round`). Otherwise `Lost`.
    - `pub fn parse_flag(line: &str) -> Result<bool, BackendError>`: `"0"`/`"1"`, otherwise `Lost`.
  - Reply rules:
    - A first line `RPRT n` with `n != 0` gives `Rejected(n)` for get and set commands; the connection stays open.
    - A set command gets `Ok` only from `RPRT 0`; any other line gives `Lost`.
    - A get command whose first line is `RPRT 0` gives `Lost`.
    - `Timeout` and `Lost` close the socket. The next `connect` opens a new one.
  - `fake_rigctld.rs` (test support):
    - `pub struct FakeRigctld { pub addr: SocketAddr, .. }` and `FakeRigctld::start(state: RigState) -> Self`, bound to `127.0.0.1:0`. Each connection runs on its own thread. `Drop` stops it.
    - `#[derive(Clone, Debug)] pub struct RigState { pub freq: i64, pub split_freq: i64, pub mode: String, pub split: bool, pub rit: bool }`. The default is `144_300_000`, `144_300_000`, `"USB"`, false, false.
    - `pub enum Behaviour { Normal, Silent, CloseAfterRead, Fixed(Vec<u8>) }`.
    - `Normal` answers the table above from the state:
      - `m` gives `"{mode}\n2400\n"`; `s` gives `"{0|1}\n{VFOA|VFOB}\n"`.
      - A set updates the state and answers `RPRT 0\n`.
      - Any other line answers `RPRT -11\n`.
    - Methods: `set_behaviour(&self, b)`, `lines(&self) -> Vec<String>` (received lines without `\n`), `raw(&self) -> Vec<u8>`, `clear(&self)`, `accepts(&self) -> usize`, `state(&self) -> RigState`, `wait_for_line(&self, l: &str, max: Duration) -> bool` (returns early).

- [ ] **Step 1: Write the failing tests** (`rigctld.rs`; `timeout` 500 ms unless named):
  - `commands_are_byte_exact`: one call per row, then `raw()`. Examples: `set_freq(144300000)` → `b"F 144300000\n"`, `set_mode("USB")` → `b"M USB -1\n"`, `set_split(true)` → `b"S 1 VFOB\n"`, `set_split(false)` → `b"S 0 VFOA\n"`, `set_split_freq(144300500)` → `b"I 144300500\n"`, `set_split_mode("PKTUSB")` → `b"X PKTUSB -1\n"`, `set_rit_on(true)` → `b"U RIT 1\n"`, and the gets `b"f\n"`, `b"m\n"`, `b"s\n"`, `b"i\n"`, `b"u RIT\n"`.
  - `replies_parsed`: with split on and split freq 144300500, the gets give 144300000, `"USB"`, true, 144300500 and false.
  - `decimal_and_crlf_freq_accepted`: `Fixed(b"144300000.000000\r\n")` gives 144300000; `Fixed(b"144300000.5\n")` gives 144300001.
  - `rprt_error_is_rejected`: `Fixed(b"RPRT -11\n")` gives `Rejected(-11)` for `get_freq` and for `set_freq(1)`, and `is_connected()` stays true.
  - `silent_server_times_out_and_reconnects`: timeout 300 ms and `Silent`. `get_freq` returns `Timeout` in under 3 s and `is_connected()` is false. Then `Normal`, `connect()`, and `get_freq() == Ok(144300000)`; `accepts() == 2`.
  - `eof_is_lost`: `CloseAfterRead` gives `Lost`.
  - `long_line_is_lost`: `Fixed` of 300 bytes `x` plus `\n` gives `Lost`.
  - `garbage_freq_is_lost`: `Fixed(b"abc\n")` gives `Lost`.
  - `refused_connect_is_nocat`: on a refused port, `connect()` returns `NoCat(t)` with `t.starts_with(&format!("rigctld 127.0.0.1:{p}: "))`, within 10 s.
  - `ops_without_connection_are_lost`: `get_freq` on a new `Rigctld` gives `Lost("not connected")`, and `accepts() == 0`.
  - `parse_never_panics_on_random_bytes`: 10 000 byte strings of length 0..400 from a fixed-seed LCG, each through `read_line` on a `Cursor` and then `parse_freq`, `parse_flag` and `parse_rprt`. No panic.
  - `golden_doppler_session_over_rigctld`: `rigctld_rig` on the default state. `enter(A)`, then `set_split_mode(144300312, 144300000, RigMode::None)`, then `leave`. `lines()` is:
    `["f","s","m","s","u RIT","f","s","f","S 1 VFOB","F 144300000","I 144300000","M USB -1","X USB -1","S 1 VFOB","F 144300312","I 144300000","M USB -1","S 0 VFOA","U RIT 0","F 144300000","s","F 144300000","S 0 VFOA","F 144300000","M USB -1"]`.
    (`enter` with A checks the split: `"f","s"`. The tick derives from dial 144 300 000 with My 523.7 Hz and DX -211.9 Hz under strategy A.)
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat --test rigctld`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-cat): rigctld TCP backend with byte-exact commands`.

### Task 3: The CAT worker (`CATWorker.OnDoWork`)

**Files:**
- Create: `crates/as-cat/src/worker.rs`, `crates/as-cat/src/config.rs`
- Modify: `crates/as-cat/src/lib.rs`
- Test: `crates/as-cat/tests/worker.rs` (a `FakeRig` implementing `Rig` with scripted statuses and values and a call log)

**Interfaces (AirScout.CAT/CATWorker.cs:87-323):**
- Consumes: `Rig`, `DopplerSession`, `RigIdent`, `RigStatus`, `RigMode`, `RigSplit`, `RigRit`, `Doppler` (Task 1); `rigctld_rig`, `RigctldConfig` (Task 2).
- Produces (`worker.rs`):
  - `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub struct DopplerTarget { pub strategy: Doppler, pub rx_hz: i64, pub tx_hz: i64 }`, and `pub type DopplerCell = Arc<Mutex<DopplerTarget>>`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct RigInfo { pub rx_hz: i64, pub tx_hz: i64, pub mode: RigMode, pub split: RigSplit, pub rit: RigRit }`.
  - `#[derive(Clone, Debug, PartialEq)] pub enum CatEvent { Text(String), Status(RigStatus), Ident(Option<RigIdent>), Info(RigInfo) }`.
  - `pub const NO_RIG_WAIT: Duration = Duration::from_secs(10);` (:196).
  - `pub type RigFactory = Box<dyn FnMut() -> Option<Box<dyn Rig>> + Send>;`
  - `pub struct CatWorker`:
    - `CatWorker::new(factory: RigFactory, update: Duration) -> Self`.
    - `fn pass(&mut self, t: DopplerTarget) -> (Vec<CatEvent>, Duration)`.
    - `fn finish(&mut self) -> Vec<CatEvent>`.
  - `pass` rules:
    1. No rig: call the factory.
       - `None` gives `[Ident(None), Text("No rig selected or rig is not supported by any available CAT!"), Status(NoRig)]` and the wait `NO_RIG_WAIT` (:190-198). The C# "Opening CAT: Success!" for an unmatched rig (L14) is not reproduced.
       - `Some` gives `[Text("Opening CAT: Success!"), Ident(Some(rig.ident()))]` and resets the old status, the old info, the old strategy and the session (:171-182).
    2. `st = rig.get_status(t.strategy)`.
       - `None` gives `[Text("Error while polling CAT: Rig lost during polling!"), Status(NoRig)]`. The rig and its session are dropped (1:1, the saved state is lost), and the wait is `update` (:203-205, :301-306).
       - Otherwise, if `st` changed: first `Text(e)` if `take_error()` gives one, then `Status(st)`.
    3. Only when Online: read mode, split, RIT, RX and TX in that order. Emit one `Info` when the `RigInfo` differs from the last one (:209-266).
    4. Doppler (:277-300):
       - If `t.strategy != old`:
         - Active: `session.enter(rig, s)`. On `true`, `old = s`; on `false`, it is retried next pass (L14, Port deviation).
         - Not active: `session.leave(rig)` and `old = None`.
       - Then, if `s.is_active() && old == s && t.rx_hz > 0 && t.tx_hz > 0`: `rig.set_split_mode(t.rx_hz, t.tx_hz, RigMode::None)` on every pass (L15, 1:1).
    5. The wait is `update`.
  - `finish()`: `session.leave(rig)` if there is a rig, then `[Status(RigStatus::None)]` (:311-322).
  - `pub fn run(w: CatWorker, cell: DopplerCell, cancel: &AtomicBool, emit: &mut dyn FnMut(CatEvent))`:
    1. Emit `Text("CAT Interface started.")` (:93, `StartOptions.Name` is `"CAT Interface"`, MapDlg.cs:2440).
    2. While not cancelled: copy `*cell.lock()` (a poisoned lock gives its inner value), call `pass`, emit each event, then sleep the wait in steps of at most 250 ms, checking `cancel`.
    3. After the loop, emit the events of `finish()`.
- Produces (`config.rs`):
  - `pub enum Parity { None, Odd, Even, Mark, Space }`, `pub enum StopBits { One, One5, Two }`, `pub enum FlowControl { Low, High, Handshake }` (Enums.cs; `Clone, Copy, Debug, Default, PartialEq, Eq`, the first variant is the default).
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct SerialConfig { pub port_name: String, pub baudrate: u32, pub data_bits: u8, pub parity: Parity, pub stop_bits: StopBits, pub rts: FlowControl, pub dtr: FlowControl }`.
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum Engine { None, Rigctld { host: String, port: u16 }, Hamlib { model: u32, serial: SerialConfig } }`.
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct CatConfig { pub engine: Engine, pub update: Duration, pub timeout: Duration }`.
  - `pub fn make_rig(c: &CatConfig) -> Option<Box<dyn Rig>>`: `None` gives `None`; `Rigctld` gives `rigctld_rig(..)`; `Hamlib` gives `None` until Task 4 (and without the feature).
  - `pub fn factory(c: CatConfig) -> RigFactory`, and `pub fn hamlib_available() -> bool { cfg!(feature = "hamlib") }`.

- [ ] **Step 1: Write the failing tests** (`update` 500 ms; the `FakeRig` ident is `("Fake", "[Fake] test")`, its values are 144300000/144300000/Usb/SplitOff/RitOff, and each Rig call is logged as a string such as `"set_split_mode(144300312,144300000,NONE)"`):
  - `no_rig_reports_ident_none_text_norig_and_waits_10s`: events as in rule 1, wait `Duration::from_secs(10)`.
  - `opening_reports_success_ident_status_info`: pass 1 gives `[Text("Opening CAT: Success!"), Ident(Some(..)), Status(Online), Info(RigInfo { rx_hz: 144300000, tx_hz: 144300000, mode: Usb, split: SplitOff, rit: RitOff })]` and wait 500 ms. Pass 2 gives `[]`.
  - `status_change_reported_with_error_text`: NoCat with `take_error() == Some("rigctld 127.0.0.1:1: refused")` gives `[Text("Opening CAT: Success!"), Ident(Some(..)), Text("rigctld 127.0.0.1:1: refused"), Status(NoCat)]`. NoCat again gives `[]`. Online gives `[Status(Online), Info(..)]`.
  - `values_read_only_online`: Offline logs no `get_mode`.
  - `info_on_any_change`: a RIT change after a quiet pass gives exactly one `Info`.
  - `rig_lost_recreates`: status None gives the rule-2 events. The factory call count goes 1, then 2 with `"Opening CAT: Success!"` on the next pass.
  - `doppler_enter_once_then_split_every_pass`: target `{A, 144300312, 144300000}`, 3 passes. The log has `"set_split_mode(144300000,144300000,USB)"` exactly once (the enter) and `"set_split_mode(144300312,144300000,NONE)"` exactly 3 times.
  - `doppler_enter_retried_until_online`: Offline for 2 passes, then Online. Passes 1 and 2 log no `set_split_mode`; pass 3 logs the enter and one tick.
  - `doppler_none_leaves_once`: after an enter, target `{None, ..}` logs `"set_simplex_mode(144300000,USB)"` once; the next pass logs no setter.
  - `zero_target_not_pushed`: target `{A, 0, 0}`. The enter happens, and no `set_split_mode` with a 0 argument is logged.
  - `finish_leaves_and_reports_none`: after an enter, `finish()` logs the leave and returns `[Status(RigStatus::None)]`.
  - `run_starts_and_stops_within_bounds`: `run` on a thread with an Online `FakeRig`. The first event is `Text("CAT Interface started.")`. After `cancel`, the thread ends within 2 s, and the last event is `Status(RigStatus::None)`.
  - `make_rig_engines`: `Engine::None` gives `None`; `Rigctld { host: "127.0.0.1", port: 4532 }` gives a rig whose `ident().rig_type == "[rigctld] 127.0.0.1:4532"`.
  - `hamlib_engine_without_feature_is_norig` (`#[cfg(not(feature = "hamlib"))]`): `make_rig` of `Engine::Hamlib { model: 1, .. }` is `None`, and `hamlib_available() == false`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat --test worker`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-cat): CAT worker loop with Doppler enter, tick and leave`.

### Task 4: Native hamlib backend behind the feature `hamlib`

**Files:**
- Create: `crates/as-cat/src/hamlib_conf.rs` (always compiled, pure), `crates/as-cat/src/hamlib.rs` (only `#[cfg(feature = "hamlib")]`)
- Modify: `crates/as-cat/src/lib.rs`, `crates/as-cat/src/config.rs` (`make_rig` Hamlib arm)
- Test: `crates/as-cat/tests/hamlib_conf.rs`, `crates/as-cat/tests/hamlib_native.rs` (`#![cfg(feature = "hamlib")]`, one `#[ignore]` test)

**Interfaces (new design; HamLibSharp is unreferenced upstream, survey 4.6):**
- Consumes: `Backend`, `BackendError`, `CatRig`, `SerialConfig`, `Parity`, `StopBits`, `FlowControl`, `Engine` (Tasks 1–3).
- Produces (`hamlib_conf.rs`):
  - `pub fn hamlib_conf(c: &SerialConfig, timeout: Duration) -> Vec<(&'static str, String)>`, in this order:
    - `rig_pathname` = port name; `serial_speed`; `data_bits`.
    - `stop_bits`: One `"1"`, One5 `"2"`, Two `"2"`.
    - `serial_parity`: `"None"`, `"Odd"`, `"Even"`, `"Mark"`, `"Space"`.
    - `serial_handshake`: `"Hardware"` when `rts == Handshake`, else `"None"`.
    - `rts_state` and `dtr_state`: Low `"OFF"`, High `"ON"`, Handshake `"Unset"`.
    - `timeout` in ms.
  - `pub fn hamlib_error(code: i32) -> BackendError`: -5 (`RIG_ETIMEOUT`) gives `Timeout`; -6 (`RIG_EIO`) gives `Lost("hamlib: I/O error (-6)")`; everything else gives `Rejected(code)`.
  - `pub const RIG_VFO_CURR: u32 = 0x2000_0000; pub const RIG_VFO_A: u32 = 1; pub const RIG_VFO_B: u32 = 2; pub const RIG_PASSBAND_NOCHANGE: std::ffi::c_long = -1; pub const RIG_DEBUG_ERR: i32 = 1; pub const RIG_CONF_END: std::ffi::c_long = 0;`
- `hamlib.rs` (feature only):
  - A private `#[link(name = "hamlib")] extern "C"` block (hamlib ≥ 4.0 ABI, `c_*` from `std::ffi`, `RIG*` as `*mut c_void`):
    - `rig_set_debug(level: c_int)`; `rig_init(model: c_uint) -> *mut c_void`; `rig_token_lookup(rig, name: *const c_char) -> c_long`; `rig_set_conf(rig, token: c_long, val: *const c_char) -> c_int`.
    - `rig_open`, `rig_close`, `rig_cleanup` `(rig) -> c_int`.
    - `rig_get_freq(rig, vfo: c_uint, *mut f64) -> c_int`; `rig_set_freq(rig, vfo, f64) -> c_int`.
    - `rig_get_mode(rig, vfo, *mut u64, *mut c_long) -> c_int`; `rig_set_mode(rig, vfo, u64, c_long) -> c_int`.
    - `rig_get_split_vfo(rig, vfo, *mut c_int, *mut c_uint) -> c_int`; `rig_set_split_vfo(rig, vfo, c_int, c_uint) -> c_int`.
    - `rig_get_split_freq(rig, vfo, *mut f64) -> c_int`; `rig_set_split_freq(rig, vfo, f64) -> c_int`; `rig_set_split_mode(rig, vfo, u64, c_long) -> c_int`.
    - `rig_get_func(rig, vfo, u64, *mut c_int) -> c_int`; `rig_set_func(rig, vfo, u64, c_int) -> c_int`.
    - `rig_parse_mode(*const c_char) -> u64`; `rig_strrmode(u64) -> *const c_char`; `rig_parse_func(*const c_char) -> u64`.
  - `pub struct NativeHamlib`, built by `NativeHamlib::new(model: u32, serial: SerialConfig, timeout: Duration)`; `unsafe impl Send` with a `SAFETY` note (one owner thread, never shared); `impl Backend`; `Drop` closes.
    - `connect`:
      1. `rig_set_debug(RIG_DEBUG_ERR)`.
      2. `rig_init(model)`; null gives `NoPort(format!("hamlib: unknown rig model {model}"))`.
      3. For each `hamlib_conf` pair: a token equal to `RIG_CONF_END` is skipped; an error from `rig_set_conf` gives `NoPort(format!("hamlib: {name}: error {code}"))`.
      4. `rig_open` < 0 gives `NoPort(format!("hamlib: cannot open the rig ({code})"))`.
      5. Every failure calls `rig_cleanup`.
    - Every operation uses `RIG_VFO_CURR`. `set_split(on)` is `rig_set_split_vfo(CURR, on, if on { B } else { A })`. RIT is `rig_get_func`/`rig_set_func` with `rig_parse_func("RIT")`. Modes go through `rig_parse_mode`/`rig_strrmode`; an unknown name (0) gives `Rejected(-1)` without a call. Return codes < 0 map through `hamlib_error`.
    - `ident()`: `("Hamlib (native)", format!("[Hamlib] model {model}"))`.
  - `make_rig`: `Engine::Hamlib` gives `Some(Box::new(CatRig::new(NativeHamlib::new(..))))` under `cfg(feature = "hamlib")`.

- [ ] **Step 1: Write the failing tests** (`hamlib_conf.rs`, default build):
  - `hamlib_conf_defaults`: `SerialConfig { port_name: "", baudrate: 9600, data_bits: 8, parity: None, stop_bits: One, rts: Low, dtr: Low }`, 500 ms, gives exactly `[("rig_pathname",""),("serial_speed","9600"),("data_bits","8"),("stop_bits","1"),("serial_parity","None"),("serial_handshake","None"),("rts_state","OFF"),("dtr_state","OFF"),("timeout","500")]`.
  - `hamlib_conf_variants`: One5 → `"2"`, Mark → `"Mark"`; `rts = Handshake` → `("serial_handshake","Hardware")` and `("rts_state","Unset")`; `dtr = High` → `"ON"`.
  - `hamlib_error_mapping`: -5 → `Timeout`; -6 → `Lost("hamlib: I/O error (-6)")`; -11 → `Rejected(-11)`; -1 → `Rejected(-1)`.
  - `hamlib_native.rs`: `#[ignore] native_dummy_rig_roundtrip` with model 1 (Dummy): connect, `set_freq(144300000)`, then `get_freq() == Ok(144300000)`. It runs only by hand: `cargo test -p as-cat --features hamlib -- --ignored`, with libhamlib installed.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat --test hamlib_conf`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cat`. Expected: PASS.
  - Then `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy -p as-cat --all-targets --locked --features hamlib -- -D warnings`. Expected: `Finished` with no warning; no linker runs.
  - Then `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo tree -p as-cat -e normal`. Expected: only `as-cat` and `thiserror` (and its deps).
- [ ] **Step 5: Commit** `feat(as-cat): optional native hamlib backend (feature hamlib, off by default)`.

### Task 5: CAT settings, `CatService` and the Doppler push

**Files:**
- Create: `crates/as-settings/src/cat.rs`, `crates/airscout/src/cat_service.rs`
- Modify:
  - `crates/as-settings/src/lib.rs` (`pub mod cat`, re-exports, `Settings.cat`)
  - `crates/airscout/Cargo.toml` (`as-cat.workspace = true`; `[features] hamlib = ["as-cat/hamlib"]`), `Cargo.lock`
  - `crates/airscout/src/lib.rs`
  - `crates/airscout/src/services.rs` (`ServiceEvent::Cat`)
  - `crates/airscout/src/app/services_api.rs`
  - `crates/airscout/src/app/mod.rs` (the arm `ServiceEvent::Cat { .. } => {}` with the comment "wired in Task 6")
  - `crates/airscout/src/tracking.rs` (`DARK_GRAY`, `CONTROL` become `pub(crate)`)
- Test: `crates/as-settings/tests/settings_4b.rs`, `crates/airscout/tests/cat_service.rs`

**Interfaces:**
- Consumes: `CatConfig`, `Engine`, `SerialConfig`, `DopplerTarget`, `DopplerCell`, `Doppler`, `CatEvent`, `CatWorker`, `run`, `factory`, `RigStatus` (Tasks 1–4); `TrackStep`, `TrackValues` (4a); `as_settings::DopplerStrategy` (3c).
- Produces (`as-settings/src/cat.rs`, `#[serde(default)]`, defaults from `AirScout.CAT/Properties/Settings.Designer.cs`):
  - `pub struct Cat`:
    - `pub activate: bool = false` (:28); `pub update_ms: u64 = 500` (:40).
    - `pub engine: CatEngine = None`.
    - `pub rigctld_host: String = "localhost"`; `pub rigctld_port: u16 = 4532`.
    - `pub hamlib_model: u32 = 1` (Hamlib Dummy).
    - `pub port_name: String = ""` (:76); `pub baudrate: u32 = 9600` (:88); `pub data_bits: u8 = 8` (:100).
    - `pub parity: Parity = None` (:112); `pub stop_bits: StopBits = One` (:124); `pub rts: FlowControl = Low` (:136); `pub dtr: FlowControl = Low` (:148).
    - `pub timeout_ms: u64 = 500` (:172).
  - `pub enum CatEngine { #[default] None, Rigctld, Hamlib }`, serialized `"none"`, `"rigctld"`, `"hamlib"`. It is deserialized through `deserialize_with` a lenient function: any other string gives `None`. 4c adds `OmniRig`.
  - `Parity`, `StopBits`, `FlowControl` with `rename_all = "lowercase"` (`"none"…"space"`, `"one"`, `"one5"`, `"two"`, `"low"`, `"high"`, `"handshake"`).
  - `Settings.cat: Cat`.
- Produces (`cat_service.rs`):
  - `pub fn cat_config(s: &Settings) -> as_cat::CatConfig`:
    - The engine maps from `s.cat` (Hamlib carries the serial fields).
    - `update` and `timeout` are clamped to 100..=10000 ms, the Options range (`OptionsDlg.Designer.cs` `ud_Options_CAT_Update`/`ud_Options_CAT_Timeout` Minimum/Maximum).
  - `pub fn doppler_of(d: DopplerStrategy) -> Doppler`.
  - `pub fn apply_step(t: &mut DopplerTarget, step: &TrackStep, strategy: Doppler)` (MapDlg.cs:8380-8401, :8419-8449):
    - `values == Some(v)` with `mode == Track`: `t.strategy = strategy`.
    - In TRACK and SINGLE, if `strategy.is_active() && v.rx_hz != 0 && v.tx_hz != 0`: `t.rx_hz = v.rx_hz` and `t.tx_hz = v.tx_hz`.
    - `mode == Single`: afterwards `t.strategy = None` (SINGLE ends with STOPPED in the same step, :8293-8297).
    - `values == None`: `t.strategy = None`.
  - `pub fn cat_colors(s: RigStatus) -> ([u8; 3], [u8; 3])` (`SayCAT`, MapDlg.cs:9262-9282), as `(text, background)`:
    - Online: `([255,255,255],[0,100,0])`.
    - Error, NoCat, NoPort, NoRig, NotSuitable: `([255,255,0],[255,0,0])`.
    - Offline: `([255,255,255],[255,140,0])`.
    - None: `(DARK_GRAY, CONTROL)`.
  - `pub fn rig_reports(i: &RigInfo) -> String`: `format!("Rig reports RX: {}Hz, TX: {}Hz, Mode: {}, RIT: {}, Split: {}", rx, tx, mode.name(), on_off(rit == RitOn), on_off(split == SplitOn))`, with `"ON"`/`"OFF"` (:9305-9308; `long.ToString(info)` prints plain digits).
  - `pub const EXIT_WAIT: Duration = Duration::from_secs(2);`
  - `pub struct CatService`:
    - `CatService::start(cfg: CatConfig, cell: DopplerCell, tx: Sender<ServiceEvent>, generation: u64) -> Self` spawns the thread `cat` running `as_cat::run`. Each event goes out as `ServiceEvent::Cat { generation, event }`. A spawn failure sends `ServiceEvent::Error(format!("CAT: cannot start the worker thread: {e}"))` (4a deferred minor).
    - `fn stop(&mut self)` (idempotent, returns at once).
    - `fn wait(&self, max: Duration) -> bool`: true once the thread has ended (a done channel with `recv_timeout`).
    - `Drop` stops.
  - `ServiceEvent::Cat { generation: u64, event: as_cat::CatEvent }`.
  - `Services` gains `fn stop_integrations_and_wait(&mut self, _max: Duration) { self.stop_integrations() }`.
  - `RealServices`:
    - New fields `cat: Option<CatService>`, `doppler: DopplerCell`, `doppler_strategy: Doppler`.
    - `start_integrations` also: a new cell `DopplerTarget::default()`, `doppler_strategy = doppler_of(s.doppler.strategy)` (a snapshot; only Options changes it, and Options restarts), and `CatService::start` if `s.cat.activate`.
    - `stop_integrations` stops `cat` too.
    - `track_step` also runs `apply_step(&mut cell.lock(), &step, doppler_strategy)`, with a comment naming the 4c move (Architecture).
    - `stop_integrations_and_wait` stops all, then `cat.wait(max)`.
    - `Drop` stops.

- [ ] **Step 1: Write the failing tests:**
  - `settings_4b.rs`:
    - `cat_defaults_match_csharp`: every default above.
    - `stage4a_file_loads_unchanged`: serialize the default, remove the `cat` table, set `track.offset_s = 5`. The load gives the defaults and `offset_s == 5`.
    - `cat_engine_roundtrip`: `"none"`, `"rigctld"`, `"hamlib"`.
    - `unknown_engine_loads_as_none`: `[cat]` with `engine = "omnirig"` and `rigctld_port = 4533` loads with engine `None` and port 4533.
  - `cat_service.rs` (the fake rigctld from Global Constraints):
    - `apply_step_track_sets_strategy_and_freqs`: a Track step with `rx_hz 144300312`, `tx_hz 144300000` and A gives `{A, 144300312, 144300000}`.
    - `apply_step_strategy_none_keeps_freqs`: from `{A, 1, 2}`, a Track step with strategy None gives `{None, 1, 2}`.
    - `apply_step_zero_freq_keeps_old`: from `{A, 144300100, 144300000}`, a Track step with rx 312 and tx 0 (dial 0) gives `{A, 144300100, 144300000}`.
    - `apply_step_single_ends_none_with_freqs`: a Single step with rx = tx = 144300000 and A gives `{None, 144300000, 144300000}`.
    - `apply_step_invalid_is_none`: `values: None` gives strategy None.
    - `cat_config_clamps`: `update_ms` 0 gives 100 ms; `timeout_ms` 60000 gives 10000 ms; rigctld `127.0.0.1:4533` gives `Engine::Rigctld { host: "127.0.0.1", port: 4533 }`.
    - `cat_colors_match_csharp`: all eight statuses.
    - `rig_reports_text`: `RigInfo { 144300312, 144300000, CwR, SplitOn, RitOn }` gives `"Rig reports RX: 144300312Hz, TX: 144300000Hz, Mode: CW_R, RIT: ON, Split: ON"`.
    - `service_reports_online_against_fake_rigctld`: update 100 ms, generation 7. Within 10 s the events with generation 7 contain, in order, `Text("CAT Interface started.")`, `Text("Opening CAT: Success!")`, `Status(Online)` and `Info(RigInfo { rx_hz: 144300000, .. })`.
    - `service_pushes_doppler_and_restores_on_stop`: cell `{A, 144300312, 144300000}`. Within 10 s the fake sees `"F 144300312"` and `"I 144300000"`. Then `stop()`; `wait(10 s)` is true. The fake's last nine lines are `["M USB -1","S 0 VFOA","U RIT 0","F 144300000","s","F 144300000","S 0 VFOA","F 144300000","M USB -1"]`, and the last event is `Status(RigStatus::None)`.
    - `refused_rigctld_reports_nocat_once`: on a refused port with update 100 ms, wait for the first `Status(NoCat)` (≤ 10 s), then collect 3 s more. There is no second `Status(NoCat)` and exactly one `Text` starting with `"rigctld 127.0.0.1:"`.
    - `real_services_start_stop_cat`: `RealServices` with `cat.activate`, rigctld on the fake, `track.activate`, strategy A, update 100 ms. `start_integrations(.., 1, ..)` gives `Cat { generation: 1, Status(Online) }` within 10 s. `track_step(TrackStep { values: Some(v with rx_hz 144300312, tx_hz 144300000), mode: Track })` gives `"F 144300312"` at the fake. `stop_integrations_and_wait(Duration::from_secs(5))` returns, and the fake already holds the nine leave lines.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings --test settings_4b` and `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test cat_service`. Expected: FAIL.
- [ ] **Step 3: Implement.** Run one `cargo build -p airscout` without `--locked` and commit `Cargo.lock`.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): CAT settings, CAT service and the Doppler push`.

### Task 6: App wiring — CAT events, dial frequency, CAT field, exit

**Files:**
- Create: `crates/airscout/src/app/cat.rs`
- Modify:
  - `crates/airscout/src/app/mod.rs` (fields, the `ServiceEvent::Cat` arm, `on_exit` calls `stop_integrations_and_wait(EXIT_WAIT)`)
  - `crates/airscout/src/app/integrations.rs` (`cat_status` reset in start and stop; module doc)
  - `crates/airscout/src/ui/status.rs` (the CAT field after ROT, with a tooltip)
  - `crates/airscout/src/tracking.rs` and `crates/airscout/src/app/tracking.rs` (module docs: the rig output now exists)
- Test: `crates/airscout/tests/cat_app.rs` (a fake `Services` that records calls, as in `tests/integrations.rs`)

**Interfaces (MapDlg.cs:9252-9331):**
- Consumes: `CatEvent`, `RigInfo`, `RigIdent`, `RigStatus` (Tasks 1, 3); `cat_colors`, `rig_reports`, `EXIT_WAIT`, `ServiceEvent::Cat`, `Services::stop_integrations_and_wait` (Task 5); `integration_gen`, `track_status` (4a).
- Produces:
  - `AppState`: `pub cat_status: RigStatus`, `pub cat_ident: Option<RigIdent>`.
  - `pub fn cat_tooltip(&self) -> String` (:9315-9325): `format!("{}\n{}\n\n{}", ident.cat_version, ident.rig_type, status.name())`, or `format!("CAT error!\n\n{}", status.name())` without an ident.
  - `pub(super) fn on_cat_event(&mut self, generation: u64, e: CatEvent)`. Other generations are dropped.
    - `Text(t)` sets `status = t`. `Status(s)` sets `cat_status = s`. `Ident(i)` sets `cat_ident = i`.
    - `Info(i)` sets `status = rig_reports(&i)`. If `track_status != TrackStatus::Tracking`, also `settings.doppler.dial_freq_hz = i.rx_hz` (:9293-9301; SINGLE and STOPPED do update).
  - `start_integrations` and `stop_integrations` set `cat_status = RigStatus::None` (the C# worker's last report, CATWorker.cs:322).
  - `on_exit` calls `services.stop_integrations_and_wait(EXIT_WAIT)` instead of `stop_integrations` (then `integration_gen += 1` and the resets, as in `stop_integrations`).
  - Status bar: `"CAT"` coloured by `cat_colors(state.cat_status)`, after ROT (`ss_Main` order TRK, ROT, CAT, MapDlg.Designer.cs:1100-1102), with `on_hover_text(state.cat_tooltip())`.

- [ ] **Step 1: Write the failing tests:**
  - `rig_info_sets_dial_and_status_text`: `Info(RigInfo { 144300000, 144300000, Usb, SplitOff, RitOff })` gives `dial_freq_hz == 144300000` and `status == "Rig reports RX: 144300000Hz, TX: 144300000Hz, Mode: USB, RIT: OFF, Split: OFF"`.
  - `rig_info_not_applied_while_tracking`: with `track_status = Tracking` and dial 144300000, `Info` with rx 144300312 leaves the dial at 144300000 and still sets the status text.
  - `rig_info_applied_while_single_or_stopped`.
  - `stale_cat_events_dropped`: `Status(Online)` with the old generation leaves `cat_status == RigStatus::None`.
  - `cat_tooltip_texts`: ident `("Hamlib rigctld (net protocol)", "[rigctld] localhost:4532")` with Online gives `"Hamlib rigctld (net protocol)\n[rigctld] localhost:4532\n\nONLINE"`; no ident with NoRig gives `"CAT error!\n\nNORIG"`.
  - `stop_integrations_resets_cat_status`.
  - `exit_waits_for_cat`: `on_exit()` records one `stop_integrations_and_wait(Duration::from_secs(2))` and no plain `stop_integrations`.
  - `history_on_stops_cat`: entering HISTORY records the stop (4a lifecycle) and sets `cat_status == None`.
  - kittest `status_bar_shows_cat_after_rot`: the label `"CAT"` comes after `"ROT"`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test cat_app`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS. Report `wc -l crates/airscout/src/app/mod.rs` (at most about 766).
- [ ] **Step 5: Commit** `feat(airscout): CAT status field, rig info and dial frequency`.

### Task 7: Options — CAT tab and the Track tab Doppler group

**Files:**
- Create: `crates/airscout/src/ui/options/cat.rs`
- Modify:
  - `crates/airscout/src/ui/options/mod.rs` (`OptionsTab::Cat` after `Track`, the `ALL` array, the show arm, `validate` appends `cat::validate`)
  - `crates/airscout/src/ui/options/track.rs` (the Doppler group and texts; module doc)
- Test: `crates/airscout/tests/ui_options_4b.rs`. Set the tab with the `tab()` helper of `ui_options_4a.rs`, not by click (4a minor). Add `Cat` wherever the existing tests list every tab.

**Interfaces (OptionsDlg.cs:2617-2660, :2744-3080; OptionsDlg.Designer.cs CAT tab texts; OptionsDlg.resx `richTextBox1.Text`, `richTextBox4.Text`):**
- Consumes: `Cat`, `CatEngine`, `Parity`, `StopBits`, `FlowControl` (Task 5); `as_cat::hamlib_available` (Task 3).
- Produces:
  - `cat::validate(s: &Settings) -> Vec<String>`, only when `s.cat.activate && s.cat.engine == CatEngine::Rigctld`:
    - An empty `rigctld_host.trim()` gives `"CAT: the rigctld host must not be empty."`.
    - `rigctld_port == 0` gives `"CAT: the rigctld port must be between 1 and 65535."`.
  - `cat::engines() -> Vec<(CatEngine, &'static str)>`: `(None, "[None]")`, `(Rigctld, "[rigctld] Hamlib rigctld (TCP)")`, plus `(Hamlib, "[Hamlib] native library")` only if `hamlib_available()`.
  - `cat::show(ui, s)`, in this order:
    - Group **"Activate CAT Interface"**: checkbox **"Activate CAT Interface"**, and **"CAT Refresh [ms]:"** (`update_ms`, `100..=10000`, step 100), enabled while activated.
    - Group **"Rig Type"**, enabled while activated:
      - **"Select CAT-Engine & Rig-Type:"** with a combo over `engines()`. An engine not in the list shows `"[Hamlib] not available in this build"` as the selected text.
      - The line **"AirScout supports the following CAT-Engines:"**, then `"1. Hamlib rigctld via TCP (all systems)\n(rigctld must be running; several programs can share the rig through it)"`, and with the feature `"2. Hamlib native library (needs libhamlib, exclusive access to the rig)"`.
      - Then verbatim: `"Rig must support a minimum of CAT control capabilities:\n- set frequency of VFO A and VFO B\n- set/reset Split Mode"`.
    - Group **"rigctld Settings"**, only for Rigctld: **"Host:"** (text), **"Port:"** (`0..=65535`), **"Timeout [ms]:"** (`timeout_ms`, `100..=10000`, step 100), and the line "rigctld is reached over plain TCP without authentication; keep it on this computer or a trusted network."
    - Group **"Serial Port Settings"**, only `#[cfg(feature = "hamlib")]` and Hamlib:
      - **"Hamlib Rig Model:"** (`u32`), **"PortName:"** (free text: a device path on Linux/macOS).
      - **"Baudrate:"** from `[110, 300, 600, 1200, 2400, 4800, 9600, 14400, 19200, 38400, 56000, 57600, 115200, 128000, 256000]` (:2793), **"Databits:"** `[5, 6, 7, 8]`.
      - **"Parity:"** None/Odd/Even/Mark/Space, **"Stopbits:"** `"1"`/`"1,5"`/`"2"`, **"RTS:"** and **"DTR:"** Low/High/Handshake (Enums.cs descriptions).
      - **"Timeout [ms]:"**.
    - Group **"Operating Instructions"**, enabled while activated, verbatim: `"CAT control is introduced for Doppler shift compensation (see \"Track\" tab for details). \nAirScout is calculating tracking and Doppler shifts once per second by default. Each communication with rig must be completed within this period."` The OmniRig recommendations follow with OmniRig in 4c.
  - `track::show`:
    - The 3c dial frequency and strategy controls sit in a group headed **"Doppler Compensation (requires activated CAT and Rig connected)"**. It is enabled only when `s.cat.activate && s.track.activate` (`tab_Options_Track_Enter` :2621-2628 and `cb_Options_CAT_Activate_CheckedChanged` :2896-2899).
    - The group holds the label "You can see the dial frequency from rig here and choose how to compensate Doppler shift on your site and on DX site."
    - The last line becomes "Serial (GS-232A) and DDE (Ham Radio Deluxe) rotator outputs are not available yet."

- [ ] **Step 1: Write the failing tests:**
  - `rigctld_empty_host_blocks_ok`: `validate` contains exactly `"CAT: the rigctld host must not be empty."` for host `"  "`.
  - `rigctld_port_zero_blocks_ok`: contains exactly `"CAT: the rigctld port must be between 1 and 65535."`.
  - `cat_off_skips_rigctld_checks`: `activate = false` with port 0 gives no CAT entry.
  - kittest `cat_tab_activate_and_engine`: `tab(Cat)`, click "Activate CAT Interface", choose "[rigctld] Hamlib rigctld (TCP)", OK. Then `settings.cat.activate` is true and `engine == Rigctld`.
  - kittest `rigctld_fields_only_for_rigctld`: with engine None there is no "Host:" label; with Rigctld there are "Host:" and "Port:".
  - kittest `no_omnirig_or_serial_entries`: no label contains `"[OmniRig]"` or `"[Serial]"`. With `#[cfg(not(feature = "hamlib"))]`, also no `"PortName:"` and no `"[Hamlib] native library"`.
  - kittest `hamlib_engine_in_default_build_shown_as_unavailable` (`#[cfg(not(feature = "hamlib"))]`): with engine Hamlib, the text "[Hamlib] not available in this build" is shown.
  - kittest `doppler_group_needs_cat_and_track`: on the Track tab, the combo labelled "Doppler strategy" is disabled with only `track.activate` and enabled with `cat.activate` as well.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test ui_options_4b`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS.
  - Then run the feature check from Global Constraints. Expected: `Finished`, no warning.
  - Report `wc -l` of `ui/options/mod.rs` (505; keep it under about 520).
- [ ] **Step 5: Commit** `feat(airscout): Options CAT tab and Doppler group enablement`.

### Task 8: Docs, findings, gates

**Files:**
- Modify: `README.md`, `docs/logic-findings.md`

**Steps:**
- [ ] **Step 1: README:**
  - Status row `4b | CAT over rigctld (optional native hamlib), Doppler push, CAT field and tab | done`.
  - Desktop app, stage 4b:
    - The engines.
    - How to run rigctld (`rigctld -m <model> -r <device>`; default port 4532; no `--vfo` mode).
    - The CAT field colours and tooltip.
    - The Doppler strategies push RX/TX once per second while TRACKING. The rig is restored when tracking ends, when Options opens, in HISTORY and on exit (exit waits up to 2 s).
    - The dial frequency follows the rig except while TRACKING.
    - Replace "the radio Doppler control (CAT, stage 4b) are not available yet" accordingly. The minimised-window limitation now covers the Doppler push too.
  - Build: `cargo build -p airscout --release --features hamlib` needs libhamlib ≥ 4.0 with its development files (for example Debian `libhamlib-dev`, Homebrew `hamlib`; add `RUSTFLAGS="-L <libdir>"` if the linker does not find it). Linux and macOS only; on Windows use rigctld. Not built by CI.
  - "Network sockets": add the row `CAT rigctld | <rigctld host>:<rigctld port> (default localhost:4532), TCP | outbound | Options, CAT, "Activate CAT Interface" is on with the rigctld engine`. Note that it is plain text, unauthenticated, and carries only rig commands. Fix the intro sentence (UDP sockets plus one TCP connection).
  - Privacy: CAT sends no personal data. A remote rigctld host receives the frequency and mode commands in plain text.
  - Findings range "Stage 4b's are #216 to #N".
- [ ] **Step 2:** Add findings from **#216**, in this order. Each cites C# file:line and says Ported 1:1 / Not reproduced / Port deviation / New design.
  1. L14a: "Opening CAT: Success!" and rig info are reported though no rig matched (CATWorker.cs:149-173). Not reproduced: the port reports only the "No rig selected…" text, NORIG and a "CAT error!" tooltip.
  2. L14b: `EnterDoppler` returns false when the rig is not online, but `olddoppler` is already set, so it is never retried. Split mode is then pushed with nothing saved (CATWorker.cs:277-300, ScoutBaseRig.cs:650-655). Port deviation: retried every pass; ticks start only after a successful enter.
  3. L15: `SetSplitMode` runs on every poll even when unchanged (CATWorker.cs:296-300). Ported 1:1; with rigctld that is three commands per poll. The lock-free cross-thread fields become a mutex cell with the same "latest value" semantics.
  4. `LeaveDoppler` without a saved state calls `SetSimplexMode(0, NONE)`, which writes 0 Hz and split off, on every worker stop (ScoutBaseRig.cs:668-686, OmniRigX.cs:1291-1308, CATWorker.cs:311-318). Port deviation: nothing is sent without a saved state, and no frequency ≤ 0 is ever written, also not from a zero dial.
  5. `LeaveDoppler` keeps the saved state, so a second leave (a stop after tracking ended) retunes the rig to the frequency of before the last tracking (CATWorker.cs:314). Port deviation: restored once.
  6. `LeaveDoppler` restores split and then `SetSimplexMode` switches split off again, so a saved split ON is lost (ScoutBaseRig.cs:668-686). Ported 1:1.
  7. Modes outside CW, CW-R, LSB, USB, DIG, DIG-R, AM and FM read as NONE and are not restored after Doppler (ScoutBaseRig.cs:143-163). Ported 1:1. Give the hamlib name mapping (`PKTLSB`/`PKTUSB` as DIG/DIG_R).
  8. NOTSUITABLE is checked only for strategy A (ScoutBaseRig.cs:103-130). Ported 1:1; for rigctld and hamlib "suitable" means that the split query answers.
  9. Doppler push:
     - TRACKING/SINGLE sets the strategy with the first true flag (MapDlg.cs:8380-8401); the values set it with the last true flag (:8419-8449, L21).
     - SINGLE ends with STOPPED, so the strategy is NONE again in the same step.
     - RX/TX are copied only when both are non-zero, but the strategy is set anyway, so the rig gets stale or zero frequencies.
     - Ported 1:1, except zero (item 4). L21 does not apply (one enum).
  10. Rig info updates `Doppler_DialFreq/Mode/Split/Rit` only when `TrackStatus != TRACKING` (MapDlg.cs:9293-9301). Mode, Split and Rit are written and never read.
      - Ported 1:1 for the frequency; the other three are not ported.
      - The C# report at open reads an unpolled rig and sets the dial to 0. Not reproduced (GUI).
  11. The UI thread reads the live `IRig` object (MapDlg.cs:9290-9310) and gets up to five reports per poll (CATWorker.cs:215-266). Port deviation: one `RigInfo` snapshot per changed poll, with the same status text.
  12. The status is re-reported on every online poll (CATWorker.cs:213), and the error text is `ex.ToString()` with a stack trace (:303). Port: changes only, message only.
  13. `CAT_Update` is read live and `Thread.Sleep(0)` busy-loops for 0 (:308). `CAT_Timeout` defaults to 500 while the designer shows 5000. `CAT_ResponseTimeout` is never read; `CAT_Poll` only drives the ScoutBase engine. Port: a start snapshot (Options restarts the worker, which is equivalent); update and timeout clamped to 100..=10000 ms; the other two not ported.
  14. D7: the ScoutBase.CAT INI engine, the plain-HTTP rig-definition download (ScoutBase/ScoutBase.CAT/CATUpdater.cs:118-127), the Rig database LED and `Rig_Directory`. Not ported (owner decision).
  15. D5/D8: rigctld and native hamlib are new engines (HamLibSharp is unreferenced, survey 4.6). `CAT_RigType` is replaced by `engine` + `hamlib_model`. New design: give the command table of Task 2 and the hamlib ≥ 4.0 requirement.
  16. L22: `(int)` frequency casts overflow above 2.147 GHz (ScoutBaseRig.cs:374-418). Not reproduced: i64 throughout.
  17. Stop is asynchronous: Options open returns at once and the worker restores the rig afterwards, while upstream waits without a limit (MapDlg.cs:2469-2472). Exit waits up to 2 s. Port deviation.
  18. The Doppler group is enabled only with CAT and Track active (OptionsDlg.cs:2621-2628, :2896-2899). 1:1 restored; 3c showed it always.
  19. Owed from 4a: a Track worker `Disabled` event queued when Options opens is dropped by the generation change, so the switch-off is not persisted. Upstream writes the setting on the worker (MapDlg.cs:8216-8230). Port deviation.
- [ ] **Step 3:** Run the full gate locally on macOS, the feature check, then the Linux docker gate (all commands in Global Constraints). Expected: green. Report the test counts.
- [ ] **Step 4: Commit** `docs: stage 4b README, CAT socket and logic findings`.

## Self-review against the spec and the brief

- **`as-cat` with the operations `CATWorker` uses:** Task 1 (`Rig` = status, mode, split, RIT, RX/TX, set variants, simplex, split mode, enter/leave). PTT and VFO are unused upstream and not ported (Stage split).
- **rigctld client, exact commands, parsing, timeouts, reconnect, fake rigctld on 127.0.0.1:0:** Task 2 (byte table, 12 tests, golden session sequence).
- **hamlib FFI behind `hamlib`, off by default, the crate choice stated, CI does not build it, compile-gated:** Tech Stack, Task 4, Global Constraints (feature check without linking).
- **The CAT worker, start/stop/restart, generations, no socket I/O on the UI thread:** Tasks 3, 5, 6.
- **CAT status field:** Task 6.
- **Doppler push, 1:1 maths, rounding and clamping logged, and how it fits 4c:** Architecture, Task 5 `apply_step` (the 3c `rx_tx` truncation is unchanged), findings 2–5 and 9.
- **Options CAT tab with the C# texts; no OmniRig stub:** Task 7 (`no_omnirig_or_serial_entries`).
- **README, findings, macOS and docker gates:** Task 8.
- **Type consistency:** `DopplerTarget`, `DopplerCell`, `CatEvent`, `RigInfo`, `RigIdent`, `CatConfig` and `Engine` are defined once (Tasks 1–3) and used with the same names in Tasks 5–7.
- **4a carry-overs:** spawn-failure event (Task 5), connect error text (Tasks 1 and 3), kittest tab selection (Task 7), the owed finding (Task 8 item 19).

## Open questions (owner) and controller rulings

Owner decision (2026-11-24):
1. Approved: native hamlib targets Linux and macOS only in 4b; Windows uses rigctld. Original question: native hamlib targets Linux and macOS only in 4b (link name `hamlib`; Windows ships `libhamlib-4.dll` with another import name). Is "use rigctld on Windows" acceptable, or should the Windows link name be supported too?

Controller rulings (binding for the implementers; each with its cost if wrong):

- Ruling: a hand-written minimal binding over an opaque `RIG*` with string conf tokens, not a bindgen crate — a check without libhamlib and no struct mirrors that break between hamlib versions — cost if wrong: a hamlib ABI change in a function signature shows only at link or run time on the user's machine.
- Ruling: `unsafe` is allowed only in `as-cat/src/hamlib.rs` and only with the feature; the default build keeps `forbid(unsafe_code)` — D8 requires FFI — cost if wrong: none for default builds.
- Ruling: no frequency ≤ 0 Hz is ever written, and `LeaveDoppler` restores once and only with a saved state (findings 4, 5) — these C# paths tune a real radio to 0 Hz or to a stale frequency — cost if wrong: a user relying on the old re-tune after Options sees the rig stay on its current frequency.
- Ruling: a failed `EnterDoppler` is retried (L14b, finding 2) — integration-layer bug, not prediction — cost if wrong: Doppler engages later than upstream would have pushed a shifted split, which only happens with nothing saved.
- Ruling: `LeaveDoppler` ends in simplex even if the rig was in split before (finding 6, 1:1) — rig-affecting behaviour stays 1:1 — cost if wrong: split users re-enable split by hand after tracking, as upstream.
- Ruling: rigctld uses passband `-1` (no change) and needs hamlib ≥ 4.0; `S 1 VFOB` comes before `F`/`I` so the split frequency never lands on the RX VFO — new engine, no C# order to keep — cost if wrong: a hamlib 3.x rigctld rejects `M`/`X` (RPRT error), which shows as the mode not being restored.
- Ruling: the default rigctld host is `localhost` (all resolved addresses tried) and the port 4532 (the rigctld default) — cost if wrong: one setting.
- Ruling: rigctld connect failure maps to NOCAT and an `RPRT`/timeout to OFFLINE; EOF and protocol errors are "rig lost" (NORIG, then a new rig) — this matches the C# meaning of "CAT not available" and "rig not responding" — cost if wrong: one colour.
- Ruling: the Doppler group is disabled without CAT, 1:1 (finding 18) — cost if wrong: a 3c user loses the offline strategy choice in Options until CAT is on (the setting is kept).
- Ruling: exit waits up to 2 s for the rig restore; Options open does not wait — the UI must not block, except at exit, where the rig state matters more — cost if wrong: a rig behind a slow network rigctld may be left in split on exit.
- Ruling: the feature check runs locally and in the docker gate, not in CI — the brief says CI does not build the feature — cost if wrong: a hamlib-only compile error can reach `main` between stages.
