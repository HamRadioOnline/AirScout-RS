# Stage 4a — Win-Test UDP and Track Outputs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stage 4a of the spec, "Integrations", as split in the survey:
- A new crate `as-wintest` for the Win-Test UDP protocol. It parses and formats byte-exact.
- The Win-Test UDP receiver in the app. It answers ASSETPATH with ASNEAREST and passes ASSHOWPATH, ASWATCHLIST, ASADDWATCH and ASREMOVEWATCH to the app.
- The Track output worker with the outputs UDP Win-Test, UDP AirScout, native azel.dat and WSJT azel.dat.
- The Options Network tab (UDP part), the full Track tab for these outputs, and the KST watchlist sync checkbox.

**Architecture:**
- `as-wintest` is pure: it has an enum, a message struct, `encode`/`decode`, .NET Framework number formatting and the builders for the track messages. It does no I/O.
- The app gets three std-thread services in the style of `SpecLabService`. Each has a cancel flag, a `stop()` that returns at once, `Drop` that cancels, and events into the existing `ServiceEvent` channel. The events are tagged with `AppState::integration_gen`, so the app drops events of replaced services.
  - `TrackService` receives one `TrackStep` per second from the app over an mpsc channel. It fans the step out to a list of `Box<dyn TrackOutput>`, each with its own connect, send and close.
  - `WinTestService` owns the UDP listener. On its own thread it does all database work (locations, QRV, path, nearest planes) and the ASNEAREST reply. It hands the app only pure commands.
  - The ASSETPATH answer reads "live" GUI values (band, models, plane filters) from an `Arc<RwLock<NearestSettings>>`. The app pushes that struct when it changes (the `sync_my` pattern).
- The tracking maths stays where stage 3c put it (`tracking.rs`, `app/tracking.rs`). The app step now also runs while paused, as upstream does (Task 6). It hands each step to `Services::track_step`.
- Lifecycle as upstream (`ShowOptionsDlg`, MapDlg.cs:3893, :3976): the integrations start after startup or after the wizard, stop when Options opens, and start again when Options closes (OK or Cancel), also in HISTORY. They stop on exit.

**Tech Stack:**
- Rust 1.95+, std threads (the workspace has no tokio), `std::net::UdpSocket`.
- `socket2` 0.6 for `SO_REUSEADDR`/`SO_REUSEPORT`. It is already in `Cargo.lock` as a transitive dependency (0.6.5). The owner approved it as a direct dependency (2026-11-24).
- eframe/egui 0.37, egui_kittest 0.37 (dev), tempfile (dev).
- Existing crates: as-geo, as-settings, as-stations, as-aircraft, as-elevation, as-propagation.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md` (stage 4 "Integrations", the `as-wintest` crate row, "Win-Test UDP is byte-compatible by nature").

**Inputs:**
- Survey: `docs/superpowers/surveys/2026-11-24-integrations-survey.md`, sections 1, 2, 5 and 8, and "Decisions (project owner, 2026-11-24)". The decisions are binding: D9 (checksum ignored), D10 (ports 1:1 plus a warning), D12 (correct connect/reconnect; rounding, clamping and counters 1:1).
- Ledger: `docs/superpowers/ledgers/2026-11-23-stage3c-history-maintenance.md`. Its Task 15 ruling says no elevation or DB I/O on the UI thread; tracking heights come from a worker.
- Upstream: `target/upstream/AirScout/` @ `91b02cb`. It is read-only: never modify or delete it. C# references are relative to `target/upstream/AirScout/AirScout/` unless they start with `WinTest/`, `ScoutBase/` or `AirScout.`. Port from `WinTest/WinTest.cs`, not from the stale `AirScout/WinTest.cs` (survey 2.7).

**Stage split:**
- **4a (this plan):** `as-wintest`; the receiver and its five handled messages; the Track worker core with UDP and file outputs; the ROT status field; the Network tab (UDP part); the Track tab (activation, offset, UDP and file groups, the 3c Doppler display fields); the Watchlist sync checkbox.
- **4b:** `as-cat` (rigctld, optional hamlib), the CAT tab and the Doppler push. `TrackValues.rx_hz/tx_hz` are already computed (3c).
- **4c:** the GS-232A serial outputs, `as-dde` (HRD rotator) and OmniRig. They plug in as new `TrackOutput` implementations and new `OutputGroup` variants (`Dde`, `Serial`). Their settings and their Options groups are added then.
- **4d:** `as-web`, the HTTP part of the Network tab, and `Webserver_Port` with its "different port numbers" check (OptionsDlg.cs:2591-2598).
- **Not ported:** the `WinTest_Port` setting (Settings.settings:113, never read); the stale `AirScout/WinTest.cs`; the unused `WinTest/SupportFunctions.cs` helper.

## Global Constraints

- **Port policy:**
  - Prediction- and wire-affecting behaviour is ported 1:1, even when it is wrong, and logged in `docs/logic-findings.md`. The next finding number is **#194** (check the last row before you write). Task 9 assigns the numbers in the order of its list.
  - D9: the checksum is computed on receive and never checked (1:1).
  - D12: the connect/reconnect structure is done correctly. Rounding (`"000"`, `F1`, `F8`), clamping (`0 <= az < 360` on the unrounded value) and the error counters are ported 1:1.
  - D10: the defaults stay 1:1. `Server_Port` and `Track_UDP_AirScout_Port` are both 9872. Options warns when two enabled sockets share a port.
  - GUI-layer and integration-layer upstream bugs are **not** reproduced. Each still gets a finding marked "Not reproduced (GUI)" or "Port deviation".
- **Scope:** no rotator serial, DDE, CAT or web code in 4a. There is no stub UI that pretends to work. The Track tab states in one line that serial/DDE rotators and the radio Doppler control are not available yet.
  - Extension points: the `TrackOutput` trait, the `OutputGroup` enum, and the fixed output order in `build_outputs` (a comment marks where DDE (first) and Serial (between UDP and File) go).
- Pixel-identical UI is a non-goal. The layout follows the original's information architecture.
- **Anti-goal:** the UI must not stutter or block.
  - No socket, file, DB or elevation I/O runs on the UI thread. The UI thread only sends over channels and computes the 3c tracking maths.
  - A slow ASSETPATH answer blocks only the receiver thread, as upstream does (MapDlg.cs:7578-7583).
- **Secrets:**
  - No secrets in the repo.
  - Credentials (feed keys, the Thunderforest key) come only from user settings.
  - They never appear in logs, `Debug` output, status lines, error texts or the User-Agent.
- **Network:**
  - Every HTTP request uses the honest User-Agent `as_net::user_agent()`. No spoofing. 4a adds no HTTP.
  - Requests to outside services never carry the user's call sign or position beyond what the feed's documented API needs.
  - `getkey.php`, session keys, register, upload and plugin download are **not** ported.
  - Non-public or credential endpoints are off by default and need a disclaimer.
  - Bulk downloads run only on an explicit user action.
  - Self-hosted CI never runs fork PR code.
  - **Sockets in 4a, as upstream (survey 8.1). Each one is documented in the README (Task 9):**
    - Win-Test receiver: UDP `0.0.0.0:<server.port>` (default 9872), only while `server.activate`.
    - ASNEAREST reply: UDP broadcast `255.255.255.255:<server.port>` from an ephemeral port, only as the answer to an ASSETPATH.
    - Track output: UDP broadcast `255.255.255.255:<udp_wintest_port | udp_airscout_port>` (9871 / 9872), only while tracking with that output selected.
  - There is no other outbound traffic. Track messages carry the server name and az/el. ASNEAREST carries the calls and locators of the request, plus plane calls, categories, distances, potentials and minutes.
  - Every received datagram is untrusted input:
    - `decode` never panics.
    - The receive buffer is 65 535 bytes.
    - Watchlist messages add at most `watchlist_max_count` entries.
    - Nothing allocates in proportion to a number found in a datagram.
- **Settings:**
  - TOML via `as-settings`.
  - New fields get `#[serde(default)]` defaults from `AirScout/Properties/Settings.settings` (lines named in Task 2).
  - A settings file from stage 3c (new fields missing) loads unchanged.
- Every crate has `#![forbid(unsafe_code)]`.
- **SQL:** all SQL uses bound parameters.
- **Files:** `azel.dat` is `<data dir>/Tmp/azel.dat`. `Tmp` is created when missing. The file is rewritten in place every step (1:1).
- **Threads:** each service thread is named (`wintest`, `track`). `stop()` sets the cancel flag and returns at once. A thread notices the flag within 250 ms (socket read timeout or `recv_timeout`) and closes its sockets. `Drop` cancels.
- **Cargo:**
  - Every cargo command carries the prefix `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
  - Total disk use stays under 120 GB. Before each task, check `du -sh target`. Above 40 GB, run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clean --profile dev` (for the docker dir, add `--target-dir target/linux`).
  - **Never** run plain `cargo clean`: it deletes `target/upstream`.
- **Gate:**
  - Command: `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo fmt --all --check && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy --workspace --all-targets --locked -- -D warnings && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace --locked`.
  - Linux docker gate: `docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target/linux -e CARGO_BUILD_JOBS=2 -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 rust:1.95 bash -c 'apt-get update && apt-get install -y libasound2-dev pkg-config && rustup component add rustfmt clippy && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked'`. `target/linux` counts toward the disk budget.
  - CI must stay green on self-hosted Linux, the `x86_64-pc-windows-gnu` cross build, and hosted macOS/Windows.
  - A task that adds a crate or dependency runs one `cargo build` without `--locked` first and commits the updated `Cargo.lock`.
- **Tests:**
  - No display (`egui_kittest` without snapshots).
  - No network beyond 127.0.0.1. Sockets in tests bind `127.0.0.1` only, through `NetAddrs::loopback()`: never `0.0.0.0`, never a broadcast address.
  - Tests never depend on a fixed port being free: they bind port 0 and read the port back (`local_addr()`).
  - Waits are bounded generously (10 s) and return early on the awaited event (`recv_timeout` loop). Windows CI takes about 2 s per refused connect.
  - On Windows a UDP socket may get `WSAECONNRESET` from `recv_from` after an ICMP "port unreachable". The receiver ignores such errors (Task 5).
  - Generic data only: locators JO50AA / JO60AA, calls `TE1ST`, `TE2ND`, plane calls `TST123`, `TST456`. No real call signs.
- **File size:** new files stay under about 500 lines. `app/mod.rs` (733 lines) grows by at most about 40 lines. 4a app logic goes into `app/integrations.rs` and `app/wintest.rs`.

## Review Focus

1. **Stray, malformed or hostile datagrams on the Win-Test port** (other Win-Test traffic, an empty datagram, binary garbage, a 64 KB datagram, non-ASCII, no `": "`, too few fields, a 4-character locator).
   - Expected: each is dropped without a panic and without a status line per datagram. The receiver keeps answering valid messages. A short locator in ASSHOWPATH changes nothing (upstream pauses first, L3).
   - Tests: Task 1 `decode_never_panics_on_random_bytes`, `decode_errors`; Task 5 `malformed_datagrams_do_not_stop_receiver`, `show_path_short_locator_ignored`.
2. **The UDP port cannot be bound** (port in use without reuse, an address that is not local, a port changed in Options).
   - Expected: one error in the status line, then a quiet retry every 5 s. No busy loop (upstream L1). Stop still ends the thread.
   - Tests: Task 5 `bind_failure_reports_error_once_and_retries`, `stop_releases_port`.
3. **A rotator target fails** (send error, socket cannot be created, `Tmp` not writable, `azel.dat` locked by the reader).
   - Expected: the error text in the status line. Only "not connected" errors count. After more than 10 counted errors the group is switched off, with ROT and TRK in ERROR (1:1). Other outputs keep working. A failed output reconnects at the next step.
   - Tests: Task 3 `fanout_counts_only_not_connected_and_disables_after_11`, `failed_send_is_not_counted_and_reconnects`, `one_failing_output_does_not_stop_the_next`, `file_output_unwritable_dir_reports_error`.
4. **Options changes while running, and exit** (server port changed, server off, tracking off, output switched).
   - Expected: Options open stops both services. Close starts them with the new settings. The old port is free again within 3 s. Events of the old generation are dropped. Nothing is written after stop.
   - Tests: Task 6 `options_open_stops_and_close_restarts`, `stale_track_events_dropped`, `real_services_restart_rebinds_new_port`; Task 3 `service_stop_ends_output`.
5. **ASSHOWPATH / ASADDWATCH / ASREMOVEWATCH in the wrong app state** (in HISTORY, during a maintenance job, while already playing MULTI).
   - Expected: settings and watchlist are updated. Play is not started in HISTORY or during maintenance (the status says why). A running Play is paused and restarted for ASSHOWPATH.
   - Tests: Task 7 `show_path_in_history_does_not_play`, `add_watch_play_refused_during_maintenance`, `show_path_pauses_first_when_playing`.

---

### Task 1: Crate `as-wintest` (wire format, .NET number formatting, track messages)

**Files:**
- Create: `crates/as-wintest/Cargo.toml` (deps: `thiserror`), `crates/as-wintest/src/lib.rs`, `crates/as-wintest/src/dotnet.rs`, `crates/as-wintest/src/track.rs`
- Modify: `Cargo.toml` (`as-wintest = { path = "crates/as-wintest" }` in `[workspace.dependencies]`), `Cargo.lock`
- Test: `crates/as-wintest/tests/wire.rs`, `crates/as-wintest/tests/dotnet.rs`

**Interfaces (WinTest/WinTest.cs:8-148):**
- Produces:
  - `#[repr(u8)] pub enum WtKind { None = 0, Status = 1, PktRcvd = 2, SetAzimuth = 100, SetElevation = 101, SetAzEl = 102, AsAddWatch = 247, AsRemoveWatch = 248, AsWatchlist = 249, AsSetPath = 252, AsShowPath = 253, AsNearest = 254, Unknown = 255 }`.
    - `fn wire_name(self) -> &'static str`: the C# member name (`"NONE"`, `"STATUS"`, `"PKTRCVD"`, `"SETAZIMUTH"`, `"SETELEVATION"`, `"SETAZEL"`, `"ASADDWATCH"`, `"ASREMOVEWATCH"`, `"ASWATCHLIST"`, `"ASSETPATH"`, `"ASSHOWPATH"`, `"ASNEAREST"`, `"UNKNOWN"`).
    - `fn parse(name: &str) -> WtKind` (`Enum.Parse`, :73-80): trim; an exact, case-sensitive name gives its variant; a decimal integer that is a defined value gives its variant; anything else gives `Unknown`. That includes undefined numbers and comma lists (finding).
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct WtMessage { pub kind: WtKind, pub src: String, pub dst: String, pub data: String, pub extra: String }`.
  - `pub struct Decoded { pub msg: WtMessage, pub checksum: u8, pub checksum_ok: bool }`. `extra` is always empty after decode; the extra text stays in `data` (1:1).
  - `#[derive(thiserror::Error, Debug, PartialEq, Eq)] pub enum DecodeError { Empty, NoNameSeparator, MissingField }`.
  - `pub fn checksum(bytes: &[u8]) -> u8`: wrapping sum of all bytes, then `| 0x80`.
  - `pub fn encode(m: &WtMessage) -> Option<Vec<u8>>` (`ToBytes`, :105-148):
    - Text `{NAME}: "{src}" "{dst}" "{data}"{extra}`, then `?` for `AsNearest` or `??` for every other kind.
    - Each non-ASCII `char` becomes `?` (`Encoding.ASCII`).
    - The last byte is replaced by `checksum(&b[..len-1])`.
    - `None` and `Unknown` give `None`.
  - `pub fn decode(bytes: &[u8]) -> Result<Decoded, DecodeError>` (`FromBytes`, :69-103):
    1. Map each byte to a char; bytes >= 0x80 become `?`.
    2. The name is the text before the first `": "` (`NoNameSeparator` if there is none).
    3. `src` is up to the next space, `dst` up to the space after it, each with all `"` removed (`MissingField` if a space is missing).
    4. The rest: an empty rest is `Empty`. Otherwise drop its last char, remove every `"`, then `trim_end_matches('?')`. The result is `data`.
    5. `checksum` = last raw byte `| 0x80`; `checksum_ok` = (it equals `checksum(&bytes[..len-1])`).
  - `pub mod dotnet` (.NET Framework 4.7.2 invariant formatting, `AirScout.csproj:13`):
    - `pub fn fixed(x: f64, decimals: usize) -> String` (`ToString("F<n>")`): first round `x` to 15 significant digits (`format!("{:.14e}")`), then round that decimal string half away from zero to `decimals` places. NaN gives `"NaN"`, ±∞ gives `"Infinity"` / `"-Infinity"`. A negative value that rounds to zero keeps its `-` (pinned assumption, Open question 2).
    - `pub fn zero3(x: f64) -> String` (custom `"000"`): the same 15-digit step, then round half away from zero to an integer, at least 3 digits, `-` prefix for negative values (also when it rounds to 0).
  - `pub mod track` (MapDlg.cs:7864-7895):
    - `pub fn set_azimuth(server: &str, az: f64) -> WtMessage`: kind SetAzimuth, `src = server`, `dst = ""`, `data = "AUTO"`, `extra = " 00 " + zero3(az)`.
    - `pub fn set_elevation(server: &str, el: f64) -> WtMessage`: the same with SetElevation and `zero3(el)`.
    - `pub fn set_azel(server: &str, az: f64, el: f64) -> WtMessage`: SetAzEl, `extra = " 00 " + fixed(az, 8) + " " + fixed(el, 8)`.
    - `pub fn as_nearest(server: &str, dst: &str, data: String) -> WtMessage`: AsNearest, `extra = ""`.

- [ ] **Step 1: Write the failing tests** (`wire.rs`). Each golden datagram is the text shown plus one checksum byte:

```rust
fn golden(text: &str, cs: u8) -> Vec<u8> { let mut b = text.as_bytes().to_vec(); b.push(cs); b }

#[test] fn encode_set_azimuth_golden() {
    assert_eq!(encode(&set_azimuth("AS", 123.0)).unwrap(), golden("SETAZIMUTH: \"AS\" \"\" \"AUTO\" 00 123?", 0xB6));
}
#[test] fn encode_set_azimuth_rounds_359_6_to_360() {
    assert_eq!(encode(&set_azimuth("AS", 359.6)).unwrap(), golden("SETAZIMUTH: \"AS\" \"\" \"AUTO\" 00 360?", 0xB9));
}
#[test] fn encode_set_elevation_golden() {
    assert_eq!(encode(&set_elevation("AS", 5.0)).unwrap(), golden("SETELEVATION: \"AS\" \"\" \"AUTO\" 00 005?", 0xBA));
}
#[test] fn encode_set_azel_golden() {
    let b = encode(&set_azel("AS", 123.456789, 5.0)).unwrap();
    assert_eq!(b, golden("SETAZEL: \"AS\" \"\" \"AUTO\" 00 123.45678900 5.00000000?", 0x98));
    assert_eq!(b.len(), 52);
}
#[test] fn encode_as_nearest_single_placeholder() {
    let d = "2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,0";
    assert_eq!(encode(&as_nearest("AS", "KST", d.into())).unwrap(), golden(&format!("ASNEAREST: \"AS\" \"KST\" \"{d}\""), 0xEA));
    let d1 = "2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,1,TST123,M,120,85,7";
    assert_eq!(encode(&as_nearest("AS", "KST", d1.into())).unwrap().last(), Some(&0xDC));
}
```

  - `encode_none_and_unknown_is_none`.
  - `checksum_sum_mod_256_or_0x80`: `checksum(&[0x01, 0x02]) == 0x83`, `checksum(&[0xFF, 0x02]) == 0x81`.
  - `decode_set_path_golden`: `golden("ASSETPATH: \"KST\" \"AS\" \"1440000,TE1ST,JO50AA,TE2ND,JO60AA\"?", 0xB0)` (59 bytes) gives kind `AsSetPath`, `src "KST"`, `dst "AS"`, `data "1440000,TE1ST,JO50AA,TE2ND,JO60AA"`, `checksum_ok == true`.
  - `decode_show_path_golden`: the same data with `ASSHOWPATH` and checksum `0x85`.
  - `decode_reports_bad_checksum_but_parses`: last byte set to `0x80` gives the same `data` and `checksum_ok == false`.
  - `decode_as_nearest_roundtrip`: `decode(&encode(&as_nearest(..)))` gives the same `data`.
  - `decode_extra_stays_in_data`: decoding `set_azimuth("AS", 123.0)` gives `dst ""` and `data "AUTO 00 123"`.
  - `decode_trims_trailing_question_marks`: data `"WHAT?"` comes back as `"WHAT"`.
  - `decode_strips_quotes_in_data`: raw `ASSETPATH: "K" "AS" "a"b"??` (any last byte) gives `data "ab"`.
  - `decode_non_ascii_bytes_become_question_marks`: a byte `0xE9` inside the data comes back as `?`.
  - `decode_errors`: `b""` → `Empty`; `b"HELLO"` → `NoNameSeparator`; `b"ASSETPATH: \"K\""` → `MissingField`; `b"ASSETPATH: \"K\" \"AS\" "` → `Empty`.
  - `decode_never_panics_on_random_bytes`: 10 000 byte strings of length 0..300 from a fixed-seed LCG, each also with the prefix `ASSETPATH: `. No panic.
  - `kind_parse`: `"ASSETPATH"` and `" ASSETPATH "` → `AsSetPath`; `"assetpath"` → `Unknown`; `"252"` → `AsSetPath`; `"7"` → `Unknown`; `"SETAZIMUTH,ASSETPATH"` → `Unknown`.
  - `wire_names_and_values_match_csharp`: all 13 `(name, value)` pairs above.
  - `dotnet.rs`:
    - `fixed(0.25, 1) == "0.3"` and `fixed(0.35, 1) == "0.4"` (Rust `{:.1}` gives `0.2` and `0.3`).
    - `fixed(123.45678901499996, 8) == "123.45678902"` (the 15-digit step).
    - `fixed(5.0, 8) == "5.00000000"`, `fixed(-0.04, 1) == "-0.0"`, `fixed(f64::NAN, 1) == "NaN"`, `fixed(f64::INFINITY, 1) == "Infinity"`.
    - `zero3(122.5) == "123"`, `zero3(359.6) == "360"`, `zero3(0.4) == "000"`, `zero3(-5.0) == "-005"`, `zero3(-0.4) == "-000"`, `zero3(1234.0) == "1234"`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-wintest`. Expected: FAIL (crate items missing).
- [ ] **Step 3: Implement** the items above. Run one `cargo build -p as-wintest` without `--locked` to add the crate to `Cargo.lock`.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-wintest`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-wintest): Win-Test UDP wire format with golden bytes`.

### Task 2: Settings for 4a

**Files:**
- Create: `crates/as-settings/src/network.rs`
- Modify: `crates/as-settings/src/track.rs`, `crates/as-settings/src/lib.rs`
- Test: `crates/as-settings/tests/settings_4a.rs`

**Interfaces (Settings.settings lines):**
- Produces:
  - `pub struct Server { pub activate: bool = false (:140), pub port: u16 = 9872 (:116), pub name: String = "AS" (:143) }`, `#[serde(default)]`.
  - `Settings.server: Server`.
  - `Settings.watchlist_sync_with_kst: bool = false` (:383).
  - `pub enum TrackUdp { #[default] None, WinTest, AirScout }` (`Track_UDP_None/WinTest/AirScout`, :272, :296, :302). `#[serde(rename_all = "lowercase")]`, so `"none"`, `"wintest"`, `"airscout"`.
  - `pub enum TrackFile { #[default] None, Native, Wsjt }` (:287, :290, :308): `"none"`, `"native"`, `"wsjt"`.
  - New `Track` fields: `udp: TrackUdp`, `udp_wintest_port: u16 = 9871` (:311), `udp_airscout_port: u16 = 9872` (:314), `file: TrackFile`.
  - C# keeps three bools per radio group; one enum value is equivalent (the Options radio buttons keep exactly one true).
  - Update the module doc of `track.rs`: the 4a outputs live here; serial and DDE follow in 4c.

- [ ] **Step 1: Write the failing tests:**
  - `defaults_match_csharp`: the values above, and `Server::default().name == "AS"`.
  - `stage3c_file_loads_unchanged`: serialize `Settings::default()`, remove the `server`, `watchlist_sync_with_kst` and new `track` keys, set the existing key `track.offset_s = 5`, load it. Assert the 4a defaults, `offset_s == 5` and `first_run == false`.
  - `track_udp_roundtrip`: `"wintest"` ↔ `TrackUdp::WinTest`, `"airscout"` ↔ `TrackUdp::AirScout`.
  - `track_file_roundtrip`: `"native"`, `"wsjt"`.
  - `server_name_kept_verbatim`: `" as "` survives a save/load unchanged (the receiver compares exactly).
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings --test settings_4a`. Expected: FAIL (types missing).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(as-settings): stage 4a settings (server, track outputs, KST sync)`.

### Task 3: Track outputs and the Track worker

**Files:**
- Create: `crates/airscout/src/net_addrs.rs`, `crates/airscout/src/track_outputs.rs`, `crates/airscout/src/track_service.rs`
- Modify: `crates/airscout/src/lib.rs` (modules), `crates/airscout/src/tracking.rs` (`RotStatus`, `rot_colors`, `TrackStep`), `crates/airscout/src/services.rs` (`ServiceEvent::Track`), `crates/airscout/src/app/mod.rs` (the `ServiceEvent::Track` arm drops the event until Task 6 replaces it), `crates/airscout/Cargo.toml` (`as-wintest.workspace = true`)
- Test: `crates/airscout/tests/track_outputs.rs`, `crates/airscout/tests/track_service.rs`

**Interfaces (MapDlg.cs:7839-7990, :7992-8350):**
- Consumes: `as_wintest::{encode, track::{set_azimuth, set_elevation, set_azel}, dotnet::fixed}` (Task 1); `as_settings::{TrackUdp, TrackFile}` (Task 2); `tracking::{TrackValues, valid}` (3c).
- Produces:
  - `net_addrs.rs`: `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct NetAddrs { pub bind_ip: Ipv4Addr, pub broadcast_ip: Ipv4Addr }`.
    - `Default` is `(0.0.0.0, 255.255.255.255)`, as upstream.
    - `pub fn loopback() -> Self` is `(127.0.0.1, 127.0.0.1)`, for tests.
  - `tracking.rs`:
    - `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub enum RotStatus { #[default] None, Stopped, Tracking, Error }`.
    - `pub fn rot_colors(s: RotStatus) -> ([u8; 3], [u8; 3])` (`SayRot`, :8403-8417): None `DARK_GRAY/CONTROL`, Stopped `CONTROL/DARK_GRAY`, Tracking `[255,255,255]/[0,100,0]`, Error `[255,255,0]/[255,0,0]`. These are the same constants as `status_colors`.
    - `#[derive(Clone, Debug, PartialEq)] pub struct TrackStep { pub values: Option<TrackValues>, pub mode: TrackMode }`. `values` is `Some` only for `valid` values.
  - `track_outputs.rs`:
    - `#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)] pub enum OutputGroup { Udp, File }`. 4c adds `Dde` and `Serial`.
    - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum OutputError { NotConnected(String), Failed(String) }`. `NotConnected` is the `"[UDP]:"`-prefixed class that C# counts; `Failed` is every other exception (not counted, 1:1).
    - `pub trait TrackOutput: Send { fn group(&self) -> OutputGroup; fn connect(&mut self) -> Result<(), String>; fn is_connected(&self) -> bool; fn send(&mut self, az: f64, el: f64) -> Result<(), OutputError>; fn close(&mut self); }`.
    - `pub enum UdpFormat { WinTest, AirScout }`.
    - `pub fn udp_datagrams(f: UdpFormat, server: &str, az: f64, el: f64) -> Vec<Vec<u8>>`: WinTest gives `[set_azimuth]`; AirScout gives `[set_azimuth, set_elevation, set_azel]`, in this order (:7877-7895).
    - `pub struct UdpOutput`, built by `UdpOutput::new(f: UdpFormat, server: &str, bind_ip: Ipv4Addr, target: SocketAddr)`.
      - `connect` binds `(bind_ip, 0)` and calls `set_broadcast(true)` (survey 2.2, L5).
      - `send` without a socket returns `NotConnected("[UDP]: Client and/or IP endpoint not initialized.")` (:7869).
      - `send_to` errors return `Failed(e.to_string())`.
    - `pub enum FileFormat { Native, Wsjt }`.
    - `pub struct FileOutput`, built by `FileOutput::new(f: FileFormat, path: PathBuf)`.
      - `connect` does `create_dir_all(parent)`.
      - `send` rewrites the file in place (`File::create` + `write_all`). Errors return `Failed`.
    - `pub const NEWLINE: &str`: `"\r\n"` on Windows, `"\n"` elsewhere (`StreamWriter.WriteLine`, `Environment.NewLine`).
    - `pub fn native_azel(az: f64, el: f64) -> String`: `fixed(az,1) + "," + fixed(el,1) + NEWLINE` (:7966-7972).
    - `pub fn wsjt_azel(utc: DateTime<Utc>, az: f64, el: f64) -> String`: four lines (:7974-7986) with `t = utc.format("%H:%M:%S")`: `{t},0,0,Moon`, `{t},0,0,Sun`, `{t},{fixed(az,1)},{fixed(el,1)},Source`, `0,  0, 0,  0, 0,Doppler, R`, each followed by `NEWLINE`. `FileOutput` passes `Utc::now()`.
    - `pub struct TrackConfig { pub server_name: String, pub udp: TrackUdp, pub udp_wintest_port: u16, pub udp_airscout_port: u16, pub file: TrackFile, pub net: NetAddrs, pub azel_path: PathBuf }`.
      - `TrackConfig::from_settings(s: &Settings, data_dir: &Path, net: NetAddrs) -> Self` sets `azel_path = data_dir/Tmp/azel.dat`.
    - `pub fn build_outputs(c: &TrackConfig) -> Vec<Box<dyn TrackOutput>>`: the upstream order (:8156-8197) — [DDE, 4c], UDP (WinTest **or** AirScout, as the C# creates one client, :8024-8033), [Serial, 4c], File.
  - `track_service.rs`:
    - `#[derive(Clone, Debug, PartialEq)] pub enum TrackEvent { Rot(RotStatus), Text(String), Disabled(OutputGroup) }`.
    - `pub const MAX_ERRORS: u32 = 10` (`maxerr`, :8008).
    - `pub struct TrackFanout`.
      - `TrackFanout::new(outputs: Vec<Box<dyn TrackOutput>>) -> Self`. ROT starts as `Stopped`.
      - `fn step(&mut self, s: &TrackStep) -> Vec<TrackEvent>`.
      - `fn close(&mut self)`.
    - `step` rules (port of :8140-8260 with the D12 structure):
      - `values == None`: emit `Rot(Stopped)`.
      - `values == Some(v)`, for each output not disabled, in order:
        1. Emit `Rot(Tracking)`.
        2. If not connected, `connect()`. A connect error leaves it unconnected.
        3. `send(v.my_azimuth, v.my_elevation)`.
        4. `NotConnected(t)` gives `Text(t)` and counts it for the group. `Failed(t)` gives `Text(t)` and `close()`, so the next step reconnects; it is not counted.
        5. Each output has its own error handling: a failing output does not skip the later ones (finding, not reproduced).
        6. When a counter goes **above** `MAX_ERRORS` (the 11th counted error), the output is closed and marked disabled. Emit `Text("Tracking via UDP disabled.")`, `Rot(Error)`, `Disabled(group)` (:8216-8230). Counters are never reset while the fanout lives (1:1).
      - After the outputs, if `mode == TrackMode::Single`, emit `Rot(Stopped)` (:8293-8297).
      - Every `Rot` event is emitted only when the status changes.
    - `pub struct TrackService`.
      - `TrackService::start(cfg: TrackConfig, tx: Sender<ServiceEvent>, generation: u64) -> Self` spawns the thread `track`, which sends `Rot(Stopped)` at start (:8050-8051), then `recv_timeout(250 ms)` in a loop until cancelled, and calls `close()` at the end.
      - `fn step(&self, s: TrackStep)`: a non-blocking send; a full or closed channel drops the step.
      - `fn stop(self)` and `Drop` cancel.
    - `ServiceEvent::Track { generation: u64, event: TrackEvent }`.

- [ ] **Step 1: Write the failing tests:**
  - `track_outputs.rs`:
    - `native_azel_format`: `native_azel(0.25, 5.04) == format!("0.3,5.0{NEWLINE}")`.
    - `wsjt_azel_format`: at `2026-11-24 12:34:56Z`, `wsjt_azel(t, 123.45, 5.0) == ["12:34:56,0,0,Moon", "12:34:56,0,0,Sun", "12:34:56,123.5,5.0,Source", "0,  0, 0,  0, 0,Doppler, R"].map(|l| l.to_string() + NEWLINE).concat()`.
    - `udp_wintest_sends_one_datagram`: a receiver socket on `127.0.0.1:0`; `UdpOutput::new(WinTest, "AS", 127.0.0.1, rx_addr)`; connect, send(123.0, 5.0). One datagram equal to `encode(&set_azimuth("AS", 123.0))`; nothing more within 300 ms.
    - `udp_airscout_sends_three_datagrams_in_order`: SETAZIMUTH, SETELEVATION, SETAZEL with `00 123.00000000 5.00000000`.
    - `udp_send_without_connect_is_not_connected`: returns `NotConnected("[UDP]: Client and/or IP endpoint not initialized.")`.
    - `file_native_overwrites_each_step`: two sends; the file holds only the second line.
    - `file_output_unwritable_dir_reports_error`: the parent path is an existing regular file. `connect` or `send` returns an error, with no panic.
    - `build_outputs_order`: `udp = AirScout, file = Wsjt` gives groups `[Udp, File]`; `udp = None, file = None` gives `[]`.
  - `track_service.rs` (a `Fake` output with scripted results and call counters):
    - `fanout_counts_only_not_connected_and_disables_after_11`: a Fake that always returns `NotConnected`. Steps 1–10 emit no `Disabled`. Step 11 emits `Text("Tracking via UDP disabled.")`, `Rot(Error)` and `Disabled(Udp)`. At step 12 the Fake's send count stays 11.
    - `failed_send_is_not_counted_and_reconnects`: a Fake that always returns `Failed`. 20 steps, never `Disabled`; `connect` is called once per step.
    - `one_failing_output_does_not_stop_the_next`: UDP Fake fails, File Fake records 3 sends in 3 steps.
    - `invalid_values_report_rot_stopped_once`: from `Tracking`, two `None` steps emit exactly one `Rot(Stopped)`.
    - `single_mode_ends_with_rot_stopped`: one `Some` step in `Single` emits `[Rot(Tracking), Rot(Stopped)]` (no texts).
    - `no_output_enabled_stays_stopped`: no outputs and a `Some` step in `Track` emit no events.
    - `service_reports_rot_stopped_and_writes_file`: `TrackService` with `file = Native` in a temp dir and `udp = None`. Wait for `ServiceEvent::Track { event: Rot(Stopped), .. }`. Send one step (az 123.0, el 5.0). Within 10 s, `azel.dat` equals `format!("123.0,5.0{NEWLINE}")`.
    - `service_stop_ends_output`: after `stop()` and 500 ms, a step is not written (remove the file first, then check it stays missing for 500 ms).
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test track_outputs --test track_service`. Expected: FAIL (modules missing).
- [ ] **Step 3: Implement.** In `app/mod.rs`, add the arm `ServiceEvent::Track { .. } => {}` with the comment "wired in Task 6"; Task 6 replaces it.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): track output worker with UDP and azel.dat outputs`.

### Task 4: ASSETPATH answer (path, nearest planes, ASNEAREST)

**Files:**
- Create: `crates/airscout/src/wintest_nearest.rs`
- Modify: `crates/as-stations/src/db.rs` (`location_find_or_create`), `crates/airscout/src/lib.rs`, `crates/airscout/tests/common/mod.rs` (move `data_dir_with_globe` there from `tests/horizon.rs`, unchanged; `horizon.rs` then uses it from `common`)
- Test: `crates/as-stations/tests/find_or_create.rs`, `crates/airscout/tests/wintest_nearest.rs`

**Interfaces (MapDlg.cs:7189-7310, StationDatabase.cs:428-439, MapDlg.cs:3551-3578):**
- Consumes: `as_wintest::{encode, track::as_nearest}` (Task 1); `NetAddrs` (Task 3); `services::{Paths, ground_elevation, local_obstruction}`; `best_case::ui_position`; `as_aircraft::crossing::get_nearest_planes`; `PlaneInfoCache::get_all`.
- Produces:
  - `StationDb::location_find_or_create(&self, call: &str, loc: &str) -> Result<Option<Location>, StationError>`: `None` for an empty call or locator. If `location_find(call, Some(loc))` finds nothing, insert `Location::new(call, loc, centre.lat, centre.lon, GeoSource::FromLoc, 0, Utc::now())`, with the centre from `gpoint_from_loc(loc, MiddleMiddle)`, and return it.
  - `#[derive(Clone, Debug, PartialEq)] pub struct NearestSettings { pub band: Band, pub elevation_enabled: ElevationEnabled, pub band_settings: BTreeMap<Band, BandSettings>, pub best_case: bool, pub ttl_min: i64, pub max_circumcircle_km: f64, pub max_alt_m: f64, pub max_delta_m: f64 }`.
    - `NearestSettings::from_settings(s: &Settings) -> Self` reads `band`, `elevation_enabled`, `band_settings`, `path_best_case_elevation`, `planes_position_ttl_min`, `gui.planes_filter_max_circumcircle_km`, `planes_max_alt_m as f64` and `planes_max_delta_m`.
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct SetPathRequest { pub src: String, pub band: Band, pub my_call: String, pub my_loc: String, pub dx_call: String, pub dx_loc: String }`.
  - `pub fn parse_set_path(src: &str, data: &str) -> Option<SetPathRequest>` (:7193-7220). Split on `,`; fewer than 5 fields give `None`; `"` is removed from each field.
    - `qrg` = `a[0].trim().parse::<i32>()`, or 0 on failure (`int.TryParse`); `band = Band::from_i32(qrg / 10000)`. It must be a real band (`all_except_none_and_all`), else `None`.
    - The locators are cut to the first 6 chars; shorter ones give `None`.
    - `callsign::check` on both calls and `maidenhead::check` on both locators, else `None`.
    - The calls are not trimmed or upper-cased (1:1).
    - Every `None` is a silent drop: no reply and no DB writes (finding).
  - `pub fn set_path_path(paths: &Paths, n: &NearestSettings, r: &SetPathRequest, cancel: &AtomicBool) -> Result<Option<PropagationPath>, String>` (:7222-7262):
    - `model = n.elevation_enabled.effective_model()`; `None` is an error.
    - Station = `location_find_or_create(call, loc)`. If `n.best_case`, its position goes through `best_case::ui_position(&n.elevation_enabled, pos, |m, l| store.max_elv_loc(m, l).ok().flatten())` (`MapDlg.LocationFindOrCreate`, not stored).
    - Antenna height: `qrv_find_or_create_default(call, loc, r.band)`. If it is 0, use `default_antenna_height(n.band)`: the **requested** band for the QRV row and the **GUI** band for the default (1:1, L4).
    - Obstruction: `local_obstruction(store, model, my, dx, h_my)`.
    - `PathParams`:
      - `h1 = ground_elevation(store, &n.elevation_enabled, my) + h_my`, `h2` the same for DX.
      - `qrg_ghz = n.band.to_ghz()`.
      - `radius_km = EARTH_RADIUS_KM * n.band_settings[&n.band].k_factor`, `f1_clearance` of `n.band`. Both use the GUI band (1:1, L4).
      - `step_width_m = model.default_step_width_m()`.
    - `paths.engine.find_or_create_path(model, &p, true, Some(cancel))`.
  - `pub fn nearest_data(now: DateTime<Utc>, r: &SetPathRequest, nearest: &[PlaneInfo]) -> String` (:7266-7293):
    - The head is `"{now:%Y-%m-%d %H:%M:%SZ},{my_call},{my_loc},{dx_call},{dx_loc},"`, then `count`, `","`, then the planes, then `trim_end_matches(',')`.
    - Per plane with `int_point.is_some() && potential > 0`: `"{call},{category.short()},{int_qrb as i32},{potential},{mins},"`. `mins = (int_qrb / speed_kts / 1.852 * 60.0) as i32` if `speed_kts > 0`, else 0. Both casts truncate (1:1).
  - `pub fn answer(paths: &Paths, cache: &PlaneInfoCache, n: &NearestSettings, r: &SetPathRequest, now: DateTime<Utc>, cancel: &AtomicBool) -> Result<Option<String>, String>`:
    - `planes = cache.get_all(now, n.ttl_min)`.
    - `get_nearest_planes(now, &path, &mut planes, n.max_circumcircle_km, n.band_settings[&r.band].max_distance, n.max_alt_m, n.max_delta_m)`, with the max distance of the **requested** band (1:1, L4).
    - The result is `nearest_data(..)`.
    - `Ok(None)` if cancelled.
  - `pub fn send_reply(server_name: &str, r: &SetPathRequest, data: String, bind_ip: Ipv4Addr, to: SocketAddr) -> io::Result<()>` (:7294-7301): a new socket on `(bind_ip, 0)` with `set_broadcast(true)` sends one datagram, `encode(&as_nearest(server_name, &r.src, data))`, then closes.

- [ ] **Step 1: Write the failing tests:**
  - `find_or_create.rs`: `location_find_or_create_creates_centre_once`: the first call inserts `TE1ST`/`JO50AA` at the square centre with `FromLoc`; a second call returns the same row (`location_count() == 1`); `("", "JO50AA")` gives `None`.
  - `wintest_nearest.rs`:
    - `parse_set_path_rules`:
      - `("KST", "1440000,TE1ST,JO50AA,TE2ND,JO60AA")` gives `band == B144M` and `src == "KST"`.
      - `"12960000,…"` gives `B1_2G`; `"500000,…"` gives `B50M`.
      - `"28000,…"`, `"x,…"` and `"1440001x,…"` give `None`.
      - A locator `JO50` gives `None`; `JO50AA12` is cut to `JO50AA`.
      - Four fields give `None`; an invalid call `"1"` gives `None`.
    - `nearest_data_without_planes`: at `2026-11-24 12:00:00Z`, the result is exactly `"2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,0"`.
    - `nearest_data_lists_crossing_planes_only`. Planes:
      - `TST123`: Medium, `int_qrb 120.9`, `potential 85`, `speed_kts 450`.
      - `TST456`: Heavy, `int_qrb 50.2`, `potential 50`, `speed_kts 0`.
      - One with `potential 0` and one with `int_point None`.
      - The result is exactly `"2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,2,TST123,M,120,85,8,TST456,H,50,50,0"`.
    - `answer_end_to_end_with_globe_tile` (`common::data_dir_with_globe`, `open_data`, an empty cache):
      - `Ok(Some(data))` with `data` ending in `",0"`.
      - The station DB now has both locations.
      - The QRV rows exist for `(TE1ST, JO50AA, B144M)` and `(TE2ND, JO60AA, B144M)`.
    - `answer_path_uses_gui_band`: GUI band `B432M`, request `B144M`. The path from `set_path_path` has `qrg_ghz == 0.432` and `radius_km == EARTH_RADIUS_KM * default_band_table()[&Band::B432M].k_factor`.
    - `undefined_band_writes_nothing`: `parse_set_path` gives `None`; `qrv_count() == 0` and `location_count() == 0` afterwards.
    - `reply_bytes_on_loopback`: a receiver on `127.0.0.1:0`. `send_reply("AS", &req /* src "KST" */, d, 127.0.0.1, rx)` gives one datagram equal to `encode(&as_nearest("AS", "KST", d))`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-stations --test find_or_create` and `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test wintest_nearest`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-stations -p airscout`. Expected: PASS (also `--test horizon` after the helper move).
- [ ] **Step 5: Commit** `feat(airscout): ASSETPATH answer with ASNEAREST reply`.

### Task 5: Win-Test UDP receiver service

**Files:**
- Create: `crates/airscout/src/wintest_service.rs`
- Modify: `Cargo.toml` (`socket2 = { version = "0.6", features = ["all"] }`, owner-approved), `crates/airscout/Cargo.toml`, `Cargo.lock`, `crates/airscout/src/lib.rs`, `crates/airscout/src/services.rs` (`ServiceEvent::WinTest`), `crates/airscout/src/app/mod.rs` (the arm drops the event until Task 7)
- Test: `crates/airscout/tests/wintest_service.rs`

**Interfaces (MapDlg.cs:7005-7069, :7071-7187, :7312-7564, :7566-7599):**
- Consumes: `as_wintest::decode` (Task 1); `NetAddrs` (Task 3); `wintest_nearest::{NearestSettings, parse_set_path, answer, send_reply}` and `StationDb::location_find_or_create` (Task 4); `Paths::my_station()`; `callsign::{check, cut}`; `maidenhead::{check, gpoint_from_loc, loc_from_lat_lon}`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct WinTestConfig { pub bind: SocketAddr, pub reply_to: SocketAddr, pub reply_bind_ip: Ipv4Addr, pub server_name: String, pub sync_with_kst: bool }`.
    - `WinTestConfig::from_settings(s: &Settings, net: NetAddrs) -> Self`: `bind = (net.bind_ip, server.port)`, `reply_to = (net.broadcast_ip, server.port)`, `reply_bind_ip = net.bind_ip`.
  - `#[derive(Clone, Debug, PartialEq)] pub enum WinTestCommand { ShowPath(ShowPath), Watchlist(Vec<WatchEntry>), AddWatch(Vec<WatchEntry>), RemoveWatch(RemoveWatch) }`.
  - `pub struct ShowPath { pub band: Option<Band>, pub my_call: String, pub my: GPoint, pub dx_call: String, pub dx: GPoint }`.
  - `pub struct WatchEntry { pub call: String, pub loc: String, pub pos: GPoint, pub checked: bool }`.
  - `pub enum RemoveWatch { All, Calls(Vec<String>) }`.
  - `ServiceEvent::WinTest { generation: u64, cmd: WinTestCommand }`.
  - `pub struct WinTestCtx { pub paths: Paths, pub cache: Arc<PlaneInfoCache>, pub live: Arc<RwLock<NearestSettings>>, pub tx: Sender<ServiceEvent>, pub generation: u64 }`.
  - `pub fn bind_socket(addr: SocketAddr) -> io::Result<UdpSocket>`: `socket2` with `set_reuse_address(true)` (:7032), plus `set_reuse_port(true)` on unix other than Windows, for the same shared-port effect as Windows `ReuseAddress`. Then bind, convert to std, `set_read_timeout(250 ms)`.
  - `pub struct WinTestService`.
    - `WinTestService::start(cfg: WinTestConfig, ctx: WinTestCtx) -> Self` binds at once (so `local_addr` is known), then spawns the thread `wintest`.
    - If the bind fails: one `ServiceEvent::Error(format!("Win-Test UDP server: cannot listen on {addr}: {e}"))`, then a retry every 5 s (`sleep_cancellable`). Each further failure is silent. After a later success: `ServiceEvent::Status(format!("Win-Test UDP server listening on {addr}"))`.
    - `fn local_addr(&self) -> Option<SocketAddr>`, `fn stop(self)`, and `Drop` cancels.
  - Receive loop:
    - `recv_from` into a 65 535-byte buffer.
    - `WouldBlock`/`TimedOut`: check cancel.
    - Any other error (Windows `WSAECONNRESET`): sleep 50 ms and continue.
    - `decode` errors are dropped silently (finding: upstream logs each one).
  - `pub fn dispatch(d: &Decoded, cfg: &WinTestConfig, ctx: &WinTestCtx, cancel: &AtomicBool)`. Only `d.msg.dst == cfg.server_name` (exact) is dispatched. `src` and `checksum_ok` are not checked (D9).
    - `AsSetPath`: `parse_set_path(&src, &data)`, then `answer(.., Utc::now(), cancel)`, then `send_reply(&cfg.server_name, .., cfg.reply_bind_ip, cfg.reply_to)`, all on this thread. Errors are dropped (upstream logs them).
    - `AsShowPath`: `prepare_show_path`.
    - `AsWatchlist`: only if `cfg.sync_with_kst`.
    - `AsAddWatch` and `AsRemoveWatch`: only if `!cfg.sync_with_kst`.
    - Each command is sent as `ServiceEvent::WinTest { generation, cmd }`.
  - `pub fn show_path_band(qrg: &str) -> Option<Band>` (:7132-7155): an exact string match:
    - `"500000"` B50M, `"700000"` B70M, `"1440000"` B144M, `"4320000"` B432M,
    - `"12960000"` B1_2G, `"23200000"` B2_3G, `"34000000"` B3_4G, `"57600000"` B5_7G,
    - `"103680000"` B10G, `"240480000"` B24G, `"470880000"` B47G, `"760320000"` B76G,
    - else `None`.
  - `pub fn prepare_show_path(data: &str, stations: &StationDb) -> Option<ShowPath>`:
    - At least 5 fields; locators of 6 chars or more, cut to 6. Both calls and locators valid, else `None` (validated first; L3 not reproduced).
    - Per station: `location_find(call, None)`. If found and `loc_from_lat_lon(lat, lon, false, 3, true) == loc`, take the stored position. Otherwise take the locator centre and store it (`UpdateLocation(.., FROMLOC)`, `location_insert_or_update_if_newer`, :7114-7116).
    - `band = show_path_band(a[0])`.
  - `pub fn prepare_watchlist(data: &str, my_call: &str, stations: &StationDb) -> Vec<WatchEntry>` (:7330-7357):
    - Pairs `i = 1, 3, ..` while `i < len - 1`.
    - Call and locator are `trim().to_uppercase()`.
    - Skip an invalid call, an invalid locator, or `call == callsign::cut(my_call)`.
    - `pos` comes from `location_find_or_create`; `checked = false`.
  - `pub fn prepare_add_watch(data: &str, my_call: &str, stations: &StationDb) -> Option<Vec<WatchEntry>>` (:7395-7440):
    - `None` if `data` has no `,` or `(len - 1) % 3 != 0`.
    - Triples while `i < len - 2`.
    - An empty locator is filled from `location_find(call, None)`.
    - The same skips as above; `checked = (field == "1")`.
  - `pub fn prepare_remove_watch(data: &str) -> RemoveWatch` (:7500-7530): without `,` it is `All`; otherwise `Calls(a[1..] trim upper)`.
  - `my_call` comes from `ctx.paths.my_station().call` (already kept current by `sync_my`).

- [ ] **Step 1: Write the failing tests** (`open_data` on a temp dir; service on `127.0.0.1:0` via `NetAddrs::loopback()`; a client socket sends `encode(..)`; `recv_timeout` loops of at most 10 s):
  - `receiver_dispatches_show_path_to_app`: ASSHOWPATH `dst "AS"` with data `1440000,TE1ST,JO50AA,TE2ND,JO60AA` gives `ShowPath { band: Some(B144M), my_call: "TE1ST", my: centre(JO50AA), dx_call: "TE2ND", dx: centre(JO60AA) }`.
  - `other_destination_is_ignored`: `dst "XX"` gives no event within 1 s. A following `dst "AS"` message gives an event, which shows the receiver is still alive.
  - `malformed_datagrams_do_not_stop_receiver`: send `b""`, `b"garbage"`, 200 LCG byte strings and one 65 000-byte datagram, then a valid ASSHOWPATH. The event arrives, and no `Error`/`Status` event comes for the bad ones.
  - `show_path_uses_stored_location_when_locator_matches`: `TE1ST` stored at a precise point inside JO50AA gives `my == stored`. A `TE2ND` stored in another square gives `dx == centre(JO60AA)`, and the DB now has a `TE2ND`/`JO60AA` row.
  - `show_path_short_locator_ignored`: locator `JO50` gives `None`.
  - `show_path_band_mapping`: all 12 strings; `"1440001"` gives `None`.
  - `set_path_reply_received`: a second loopback socket is the `reply_to`. ASSETPATH from `src "KST"` gives an ASNEAREST datagram that decodes to `src "AS"`, `dst "KST"`, and `data` ending in `",TE1ST,JO50AA,TE2ND,JO60AA,0"` (use `common::data_dir_with_globe`).
  - `watchlist_only_when_sync_on` and `add_watch_only_when_sync_off`: for each flag value, one message gives an event or none within 1 s.
  - `prepare_add_watch_rules` (`my_call "TE9MY"`, `TE2ND` stored at JO60AA):
    - `"144,te1st , jo50aa,1,TE2ND,,0"` gives `[TE1ST JO50AA checked, TE2ND JO60AA unchecked]`.
    - `"144,TE1ST,JO50AA"` and `"nocomma"` give `None`.
    - `"144,TE9MY,JO50AA,1"` gives `Some([])`.
    - `"144,1,JO50AA,1"` gives `Some([])`.
  - `prepare_watchlist_pairs`: `"144,te1st , jo50aa,TE2ND"` gives one entry `TE1ST`/`JO50AA` (the odd trailing field is ignored).
  - `prepare_remove_watch_forms`: `"144"` gives `All`; `"144,te1st,TE2ND"` gives `Calls(["TE1ST", "TE2ND"])`.
  - `bind_failure_reports_error_once_and_retries`: bind `192.0.2.1:0` (TEST-NET-1, never local; nothing is sent). Exactly one `Error` containing `"cannot listen"` within 10 s, and no second one within the following 2 s. Then `stop()`.
  - `stop_releases_port`: start, note port `p`, `stop()`. Within 3 s a plain `std::net::UdpSocket::bind(("127.0.0.1", p))` succeeds (retry every 100 ms).
  - `two_receivers_share_a_port`: a second service on the same port has `local_addr() == Some(..)`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test wintest_service`. Expected: FAIL.
- [ ] **Step 3: Implement.** Run one `cargo build -p airscout` without `--locked` for the new direct `socket2` edge, and commit `Cargo.lock`. Check that the windows-gnu cross build still links (CI). In `app/mod.rs`, add the arm `ServiceEvent::WinTest { .. } => {}` with the comment "wired in Task 7".
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): Win-Test UDP receiver`.

### Task 6: App wiring — lifecycle, track steps, ROT field

**Files:**
- Create: `crates/airscout/src/app/integrations.rs`
- Modify:
  - `crates/airscout/src/app/services_api.rs` (trait methods, `RealServices`)
  - `crates/airscout/src/app/mod.rs` (fields, the `ServiceEvent::Track` arm, the `on_exit` call)
  - `crates/airscout/src/app/tracking.rs` (`track_tick`)
  - `crates/airscout/src/app/dialogs.rs` (`startup`, `finish_wizard`, `open_options_dlg`)
  - `crates/airscout/src/ui/dialogs/mod.rs` (both Options outcomes call `options_closed`)
  - `crates/airscout/src/app/shell.rs` (`sync_integrations` after `sync_my`)
  - `crates/airscout/src/ui/status.rs` (ROT field after TRK)
  - `crates/airscout/src/ui/options/track.rs` and `crates/airscout/src/tracking.rs` (module docs: the outputs now exist)
- Test: `crates/airscout/tests/integrations.rs`; update the 3c tests in `crates/airscout/tests/tracking.rs` that pinned "no step while paused".

**Interfaces:**
- Consumes: `TrackService`, `TrackConfig`, `TrackEvent`, `TrackStep`, `RotStatus`, `rot_colors`, `NetAddrs`, `OutputGroup` (Task 3); `NearestSettings` (Task 4); `WinTestService`, `WinTestConfig`, `WinTestCtx` (Task 5).
- Produces:
  - New `Services` methods, each with a no-op default:
    - `fn start_integrations(&mut self, _s: &Settings, _generation: u64, _cache: Arc<PlaneInfoCache>) {}`
    - `fn stop_integrations(&mut self) {}`
    - `fn track_step(&mut self, _step: TrackStep) {}`
    - `fn set_nearest_settings(&mut self, _n: NearestSettings) {}`
    - `fn wintest_local_addr(&self) -> Option<SocketAddr> { None }`
  - `RealServices`:
    - New fields `wintest: Option<WinTestService>`, `track: Option<TrackService>`, `nearest: Arc<RwLock<NearestSettings>>`, `net: NetAddrs` (default `NetAddrs::default()`).
    - `pub fn with_net(self, net: NetAddrs) -> Self` (tests).
    - `start_integrations` stops both services, writes `NearestSettings::from_settings(s)` into `nearest`, starts `WinTestService` if `s.server.activate`, and starts `TrackService` if `s.track.activate`.
    - `Drop` stops both.
  - `AppState`:
    - `pub rot_status: RotStatus`, `integration_gen: u64`, `nearest_synced: Option<NearestSettings>`.
    - `pub fn start_integrations(&mut self)`: `integration_gen += 1`, `rot_status = None`, `services.start_integrations(&settings, gen, cache.clone())`, `nearest_synced = Some(..)`.
    - `pub fn stop_integrations(&mut self)`: services stop, `rot_status = None`, `integration_gen += 1`.
    - `pub fn options_closed(&mut self)`: calls `start_integrations()`, after OK or Cancel, also in HISTORY (MapDlg.cs:3976).
    - `pub fn sync_integrations(&mut self)`: if `server.activate`, push `NearestSettings::from_settings` when it differs from `nearest_synced`.
  - Call sites:
    - `startup` (non-wizard branch) and `finish_wizard` call `start_integrations()` after `start_background()`.
    - `open_options_dlg` calls `stop_integrations()` once the dialog opens (MapDlg.cs:3893).
    - `on_exit` calls `stop_integrations()`.
    - `enter_history`/`leave_history` do **not** touch the integrations.
  - `on_event`: `ServiceEvent::Track { generation, event }` with `generation == integration_gen`:
    - `Rot(s)` sets `rot_status = s`.
    - `Text(t)` sets `status = t`.
    - `Disabled(Udp)` sets `settings.track.udp = TrackUdp::None`; `Disabled(File)` sets `settings.track.file = TrackFile::None`. Both set `track_status = TrackStatus::Error` (:8216-8230).
    - Other generations are dropped.
  - `track_tick` (MapDlg.cs:8054-8315; the C# `bw_Track` runs whenever `Track_Activate` is on, also while paused):
    - Steps run when `life_mode == Life && settings.track.activate`, at most once a second, playing **or paused**. `select_plane` still enters TRACK only while playing.
    - Station heights are needed only in `TrackMode::Track`. SINGLE and NONE compute without them, so no request is made.
    - Every step calls `services.track_step(TrackStep { values: valid.then_some(v), mode })`. `self.track` and `track_status` behave as in 3c.
    - Finding #181 changes: SINGLE set while paused is processed every second until Play or Pause resets it (1:1 restored).
    - While `settings.track.activate` is on, `update` calls `ctx.request_repaint_after(Duration::from_secs(1))`, so steps continue without input. A minimised window may still stop `update` on some platforms; README notes this 4a limitation (stepping moves into `TrackService` in 4c).
  - Status bar: `"ROT"` coloured by `rot_colors(state.rot_status)` after the TRK field.

- [ ] **Step 1: Write the failing tests** (`integrations.rs`; a fake `Services` that records calls, as in `tests/tracking.rs`):
  - `startup_starts_integrations` and `wizard_finish_starts_integrations`: one `start_integrations` each, `integration_gen == 1`.
  - `options_open_stops_and_close_restarts`: `open_options_dlg()` records `stop`; `options_closed()` records `start` with generation +2, after OK and after Cancel.
  - `options_close_in_history_restarts_integrations`.
  - `stale_track_events_dropped`: `Rot(Tracking)` with the old generation leaves `rot_status == None`.
  - `track_disabled_clears_udp_setting_and_sets_error`: `Disabled(Udp)` gives `settings.track.udp == TrackUdp::None` and `track_status == TrackStatus::Error`; `Text("Tracking via UDP disabled.")` gives that status line.
  - `single_while_paused_steps_outputs`: paused, `track.activate`, `right_click_dx()`, `track_tick`. The fake records `TrackStep { mode: Single, values: Some(v) }` with `v.my_azimuth == bearing(my, dx)` and `v.my_elevation == 0.0`, and 0 height requests.
  - `paused_none_mode_sends_invalid_step`: `TrackStep { values: None, mode: TrackMode::None }`.
  - `track_step_once_per_second`: steps at `t`, `t + 0.5 s` and `t + 1 s` give two recorded steps.
  - `no_steps_without_activate_or_in_history`.
  - `nearest_settings_pushed_on_change`: `sync_integrations()` twice gives one push; a band change gives a second push; with `server.activate == false`, no push.
  - `real_services_restart_rebinds_new_port` (`RealServices::new(open_data(tmp), tx).with_net(NetAddrs::loopback())`):
    - Start with `server.activate`, port 0: `wintest_local_addr() == Some(127.0.0.1:p1)`.
    - Restart: a new address; within 3 s a plain bind on `p1` succeeds.
    - `stop_integrations()`: `wintest_local_addr() == None`.
  - `real_services_track_writes_azel`: `track.activate`, `file = Native`, data dir in temp. `track_step` with valid values (az 10.0, el 1.0) gives `Tmp/azel.dat == format!("10.0,1.0{NEWLINE}")` within 10 s.
  - Update in `tests/tracking.rs` every 3c test that asserts "nothing happens while paused" for SINGLE. The report names each one.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test integrations --test tracking`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS. Report `wc -l crates/airscout/src/app/mod.rs` (at most about 773).
- [ ] **Step 5: Commit** `feat(airscout): start, stop and restart the integrations, track steps to the outputs`.

### Task 7: App handlers for the Win-Test commands

**Files:**
- Create: `crates/airscout/src/wintest_watch.rs` (pure list operations), `crates/airscout/src/app/wintest.rs`
- Modify: `crates/airscout/src/app/mod.rs` (the `ServiceEvent::WinTest` arm), `crates/airscout/src/ui/multi.rs` (Manage button), `crates/airscout/src/lib.rs`
- Test: `crates/airscout/tests/wintest_app.rs`

**Interfaces:**
- Consumes: `WinTestCommand`, `ShowPath`, `WatchEntry`, `RemoveWatch` (Task 5); `integration_gen` (Task 6); `AppState::{play, pause, sync_loc_texts, is_playing, path_mode}`, `maint.running`, `watchlist_dirty`, `alarm_focus`.
- Produces:
  - `wintest_watch.rs`:
    - `pub fn sync_watchlist(list: &mut Watchlist, e: &[WatchEntry], my: GPoint, max_len_km: f64, max_count: usize)` (ASWATCHLIST, :7322-7360):
      - Mark every unchecked item `remove = true`.
      - Per entry: `qrb = distance(my, e.pos)`. If found by `index_of_loc`, set `remove = false` and `out_of_range = qrb > max_len_km`. Otherwise `insert_capped(WatchlistItem { call, loc, out_of_range: qrb > max_len_km, checked: false, .. }, max_count)`.
      - Then `retain(!remove)`. The order is kept and new items are appended (C# `Add`, no sort).
    - `pub fn add_watch(list: &mut Watchlist, e: &[WatchEntry], my: GPoint, max_len_km: f64, max_count: usize)` (:7440-7466):
      - Found: `remove = false`, `out_of_range = qrb > max`, and `checked = e.checked` only if not out of range.
      - New: out of range gives `(oor = true, checked = false)`; otherwise `(oor = false, checked = e.checked)`, via `insert_capped`.
    - `pub fn remove_watch(list: &mut Watchlist, r: &RemoveWatch)` (:7500-7536): `All` removes every item, also checked ones; `Calls` removes every item whose call matches, for all locators.
    - `pub fn manage_enabled(s: &Settings) -> bool` = `!(s.watchlist_sync_with_kst && s.server.activate)` (MapDlg.cs:3902, :6049).
    - The cap at `watchlist_max_count` is a port deviation (robustness against network input).
  - `app/wintest.rs`, `pub(super) fn on_wintest(&mut self, cmd: WinTestCommand)`, called for `generation == integration_gen`. My position is `(settings.my_lat, my_lon)`; the limit is `settings.path_max_length_km`.
    - `ShowPath` (:7078-7180, validation already done by the worker):
      1. If playing, `pause()`.
      2. `control_tab = ControlTab::Single`.
      3. Set `my_call`, `my_lat/lon = p.my`, `my_height_m = 10.0`, `dx_call`, `dx_lat/lon = p.dx`, `dx_height_m = 10.0`, and `band` if `Some`; then `sync_loc_texts()`.
      4. `track.set_az = bearing(my, dx)`, `track.set_el = 0.0`.
      5. `alarm_focus = true` (the window comes to the front; replaces user32).
      6. `play()` only if `life_mode == Life` (in HISTORY: no Play, finding). `play()` keeps its own refusals (maintenance) and status texts.
    - `Watchlist(e)`: `sync_watchlist` only. Upstream does not refresh paths here (1:1).
    - `AddWatch(e)`: `add_watch`. If playing in MULTI, `watchlist_dirty = true` (the port's `UpdatePaths`). If any item is checked and the app is paused in LIFE, `play()`.
    - `RemoveWatch(r)`: `remove_watch`. If playing in MULTI, `watchlist_dirty = true`. If no item is checked and the app is playing in LIFE, `pause()`.
  - `ui/multi.rs`: the Manage button is `add_enabled(manage_enabled(&state.settings), ..)`.

- [ ] **Step 1: Write the failing tests:**
  - `sync_marks_unchecked_for_removal_and_keeps_checked`: `[A checked, B, C]` with entries `[C, D]` gives `[A, C, D]`, and D is unchecked.
  - `sync_sets_out_of_range`: `max_len_km = 1000`, an entry about 1500 km from My gives `out_of_range == true`.
  - `sync_respects_max_count`: `max_count = 2` and 5 new entries give 2 items.
  - `add_watch_checks_only_in_range`: an existing out-of-range item with `checked: true` stays unchecked.
  - `add_watch_new_out_of_range_unchecked`.
  - `add_watch_starts_play_when_checked_and_paused`: the fake records `start_feed`, and `mode == Playing`.
  - `add_watch_no_play_in_history`.
  - `add_watch_play_refused_during_maintenance`: `maint.running` set gives no Play, and `status == "database maintenance is running"`.
  - `remove_watch_all_and_calls`: `All` empties the list; `Calls(["TE1ST"])` removes both `TE1ST` locators.
  - `remove_watch_pauses_when_none_checked`.
  - `show_path_sets_stations_heights_band_and_az`: `my_height_m == 10.0`, `dx_height_m == 10.0`, `band == B144M`, `track.set_az == bearing(my, dx)`, `set_el == 0.0`, `control_tab == Single`, `alarm_focus`, `mode == Playing`, and `my_loc_text == "JO50AA"` (the locator of the centre position).
  - `show_path_pauses_first_when_playing`: playing MULTI before gives playing again after, in SINGLE with a new `feed_gen`.
  - `show_path_in_history_does_not_play`: the settings are updated and `life_mode == History`.
  - `show_path_unknown_band_keeps_band`.
  - `stale_wintest_generation_dropped`.
  - `manage_disabled_only_when_synced_and_server_on`: the truth table over the two flags.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test wintest_app`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(airscout): Win-Test show path and watchlist commands`.

### Task 8: Options — Network tab, full Track tab, KST sync checkbox

**Files:**
- Create: `crates/airscout/src/ui/options/network.rs`
- Modify: `crates/airscout/src/ui/options/track.rs`, `crates/airscout/src/ui/options/watchlist.rs`, `crates/airscout/src/ui/options/mod.rs` (tab `Network` before `Track`, `validate` calls `track::validate`, warnings shown)
- Test: `crates/airscout/tests/ui_options_4a.rs`

**Interfaces (OptionsDlg.cs:2583-2742, OptionsDlg.Designer.cs texts):**
- Consumes: `Server`, `TrackUdp`, `TrackFile`, `watchlist_sync_with_kst` (Task 2).
- Produces:
  - `network::show(ui, s)`:
    - Group "Activate Server" with the checkbox **"Activate Network Server"** (`server.activate`).
    - Group "UDP Server Settings": **"AirScout UDP Server Port:"** (`u16`, `0..=65535`) and **"AirScout UDP Server Name:"** (text).
    - The line: "The UDP server listens on all network interfaces (0.0.0.0) without authentication. Win-Test and wtKST send to it."
    - No HTTP group (4d).
  - `pub fn port_warnings(s: &Settings) -> Vec<String>` in `network.rs` (D10). When `server.activate && track.activate` and the selected track UDP port equals `server.port`, one entry: `format!("Network: port {p} is used by the UDP server and by the tracking output ({name}).")`, with `name` `"Win-Test"` or `"AirScout"`. The dialog shows it in yellow above the buttons; it does not block OK.
  - `track::validate(s: &Settings) -> Vec<String>`: `udp_wintest_port == udp_airscout_port` gives `"Track: Win-Test port and AirScout port must have different values."` (:2663-2669, checked always, 1:1). `validate` in `mod.rs` appends it.
  - `track::show(ui, s)`, in this order:
    - Checkbox **"Activate Tracking"**.
    - **"Tracking Time Offset: [s]"** (`offset_s`, `-3600..=3600`).
    - Group **"Rotator Control Network Output"**, enabled only while activated (:2682-2703):
      - Radio "None".
      - Radio **"UDP Broadcast (Win-Test) Az only"** with **"Port:"** (`udp_wintest_port`).
      - Radio **"UDP Broadcast (AirScout) Az/El"** with **"Port:"** (`udp_airscout_port`).
    - Group **"Rotator Control File Output"**, enabled only while activated: radios "None", **"Native Az/El"**, **"WSJT Az/El"**, and the line "Writes azel.dat to the Tmp folder of the data directory."
    - The 3c items, unchanged: "Show tracking values on the map", dial frequency, Doppler strategy.
    - The line: "Serial (GS-232A) and DDE (Ham Radio Deluxe) rotator outputs and the radio Doppler control are not available yet."
    - Remove the 3c "follow in stage 4" label and update the module doc.
  - `watchlist::show`:
    - Checkbox **"Keep in sync with KST user list (needs wtKST > V3.1 and network functions activated):"** (`watchlist_sync_with_kst`), enabled only while `server.activate` (survey 2.6).
    - "Clear watchlist" is disabled while the sync is checked (:3171-3172).

- [ ] **Step 1: Write the failing tests:**
  - `track_ports_equal_blocks_ok`: `validate(&s)` contains exactly `"Track: Win-Test port and AirScout port must have different values."`.
  - `port_warning_when_server_and_airscout_share_9872`:
    - Defaults plus `server.activate`, `track.activate` and `udp = AirScout` give `["Network: port 9872 is used by the UDP server and by the tracking output (AirScout)."]`.
    - `udp = WinTest` (9871) gives `[]`. `server.activate = false` gives `[]`.
  - `ok_allowed_with_warning`: in a kittest, clicking OK with only a warning gives `OptionsOutcome::Ok`.
  - kittest `network_tab_toggles_server`: click "Activate Network Server", then OK; `draft.server.activate == true`.
  - kittest `track_output_groups_disabled_until_activate`: the "Native Az/El" radio is disabled while tracking is off and enabled after "Activate Tracking".
  - kittest `watchlist_sync_checkbox_disabled_without_server`.
  - kittest `no_serial_or_dde_controls`: no widget labelled "GS-232A Az only" or "Ham Radio Deluxe (HRDRotator.exe)".
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test ui_options_4a`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout`. Expected: PASS. Report `wc -l` of `ui/options/mod.rs` (it was 497; keep it under about 520 by putting the logic in `network.rs`/`track.rs`).
- [ ] **Step 5: Commit** `feat(airscout): Options Network and Track tabs for stage 4a`.

### Task 9: Docs, findings, gates

**Files:**
- Modify: `README.md`, `docs/logic-findings.md`

**Steps:**
- [ ] **Step 1: README:**
  - Status row `4a | Win-Test UDP (receiver, ASNEAREST), track outputs (UDP Win-Test/AirScout, azel.dat), Network and Track tabs | done`.
  - Desktop app section, stage 4a: what each message does; the Track outputs; the `azel.dat` path `<data dir>/Tmp/azel.dat` with its two formats; the ROT field; the KST sync. Rotator serial/DDE, CAT and the web server follow in 4b–4d. Replace the 3c sentence "rotator and radio control are stage 4" accordingly.
  - New section "Network sockets", a table of the three sockets from Global Constraints (address, port, direction, when active, authentication: none; checksum not checked, as upstream).
    - Ports 9872 and 9871 are LAN-visible while enabled. There is no other outbound traffic.
    - With the defaults, the UDP server and the AirScout tracking output share port 9872, and Options warns about it.
  - Privacy section: the ASNEAREST broadcast carries the requested calls and locators and nearby plane calls on the LAN; the Win-Test receiver accepts commands from any host on the LAN.
  - Findings range "#194 to #N".
- [ ] **Step 2:** Add findings from **#194**, in this order. Each cites C# file:line and says Ported 1:1 / Not reproduced (GUI) / Port deviation.
  1. D9: the checksum is computed but never checked; `Src` is not checked; any LAN host can send ASSHOWPATH and the watch messages (WinTest/WinTest.cs:92-102, MapDlg.cs:7054-7056). Ported 1:1.
  2. `Enum.Parse` (WinTest.cs:73-80): numeric names, whitespace, comma lists (`"SETAZIMUTH,ASSETPATH"` ORs to 252 = ASSETPATH in C#). Port deviation: undefined numbers and comma lists give UNKNOWN.
  3. Malformed datagrams throw, and every one is logged (L2, MapDlg.cs:7061-7064). Not reproduced: dropped silently.
  4. A bind failure is only logged, then a busy loop (L1, :7036-7041). Not reproduced: one error, a retry every 5 s.
  5. Broadcast without `EnableBroadcast` (L5, :7295-7301, :8024-8033). Not reproduced: `set_broadcast(true)`. Also note `SO_REUSEPORT` on unix.
  6. ASSHOWPATH pauses and switches to SINGLE before validating; a short locator throws (L3, :7077-7092). Not reproduced (GUI): invalid messages change nothing.
  7. ASSETPATH band mix (L4, :7223-7265): path and QRV defaults use the GUI band, the QRV row and MaxDistance the requested band. Ported 1:1. An undefined band throws in C# after DB writes; the port drops it before any write (Port deviation).
  8. ASNEAREST: `(int)` truncation of IntQRB and minutes, `u` time format, trailing commas trimmed, the `?` single-placeholder form for wtKST (:7274-7293, WinTest.cs:115-127). Ported 1:1.
  9. `"000"` rounding: 359.6 gives `360`, negative elevation gives `-005`; the range check runs before rounding (L9, :7870, :7886, :7890, :8142-8146). Ported 1:1, with .NET Framework 15-digit rounding (`as_wintest::dotnet`). The sign of a negative value that rounds to zero is a pinned assumption.
  10. Error counters (L18, :8198-8247): only "[UDP]:"-class errors count; the output is switched off after the 11th; counters never reset; real send and file errors are never counted. Ported 1:1. (The doubled serial count is 4c.)
  11. One try block around all outputs: an error skips the later outputs of that second (:8156-8260). Not reproduced: each output is isolated.
  12. Clients re-created per outer pass without closing, a second `Open` on an open port (L10, :8011-8049, :8305-8319). Port deviation (D12): connect once, close on error, reconnect at the next step, close on stop.
  13. `azel.dat` is rewritten in place, not atomically; platform newline; invariant `F1` (:7964-7990). Ported 1:1.
  14. Update #181: SINGLE while paused now steps every second, as `bw_Track` does (1:1 restored). ROT, like TRK, reports SINGLE then STOPPED in the same step. Update #177: `Track_Update` is still not read.
  15. ASWATCHLIST/ASADDWATCH add without a limit (:7350-7360, :7455-7466). Port deviation: capped at `watchlist_max_count`.
  16. ASSHOWPATH/ASADDWATCH call `Play()` in any mode (:7158, :7472-7476). Not reproduced (GUI): no Play in HISTORY. The window-to-front calls (user32, :7160-7172) become a viewport focus request.
  17. The Options dialog stops all workers while open (MapDlg.cs:3893), so Win-Test datagrams sent meanwhile are lost. Ported 1:1.
  18. `Server_Port` default 9872 equals `Track_UDP_AirScout_Port` (Settings.settings:116, :314) and the UI does not compare them (D10). Kept; the port adds a warning.
  19. ASSETPATH runs on the receive thread, so no datagram is received while it computes (:7578-7583). Ported 1:1.
  20. `WinTest_Port` is declared and never read; `AirScout/WinTest.cs` is a stale uncompiled copy. Not ported.
  21. L6, the garbled null check in `Track_UDP_WinTest` (:7866-7868). Not applicable: the port has no null client.
- [ ] **Step 3:** Run the full gate locally on macOS, then the Linux docker gate (both commands in Global Constraints). Expected: green; report the test counts.
- [ ] **Step 4: Commit** `docs: stage 4a README, sockets and logic findings`.

## Self-review against the spec and the brief

- **`as-wintest`, byte-exact, checksum ignored (D9), golden bytes:** Task 1 (eight golden datagrams, checksum vectors, fuzz), finding 1.
- **Receiver with ASNEAREST and the other messages; restart on Options OK:** Tasks 4, 5 and 7; lifecycle in Task 6 (`options_open_stops_and_close_restarts`, `real_services_restart_rebinds_new_port`).
- **Track worker core, correct connect/reconnect (D12); UDP Win-Test, UDP AirScout, azel.dat; rounding, clamping and counters 1:1 with findings:** Task 3; findings 9–13.
- **No rotator serial/DDE or CAT; clean extension points; no stub UI:** Stage split, `TrackOutput`/`OutputGroup`/`build_outputs`, Task 8 `no_serial_or_dde_controls`.
- **Network tab and full Track tab; port defaults 1:1 with a D10 warning:** Tasks 2 and 8; finding 18.
- **Bind addresses as upstream, each in the README; no other outbound traffic:** Global Constraints "Sockets in 4a", Task 9 Step 1; tests bind loopback only through `NetAddrs::loopback()`.
- **Final docs task:** Task 9.
- **Global Constraints:** they include the cargo env prefix, the 120 GB budget, `cargo clean --profile dev` only, loopback-only tests on port 0, generous waits for Windows, and every 3c security and privacy rule.
- **Placeholders:** none. Every test names its exact values; the finding numbers start at #194 and are assigned in Task 9's order.
- **Type consistency:** `TrackStep`, `TrackEvent`, `NearestSettings`, `WinTestCommand` and `NetAddrs` are defined once (Tasks 3–5) and used with the same names in Tasks 6–7.

## Open questions (owner) and controller rulings

1. Task 5 makes `socket2` (already in `Cargo.lock` as 0.6.5, transitive) a direct dependency for `SO_REUSEADDR`/`SO_REUSEPORT`. Without it the receiver uses plain `std` bind, and the bind fails when another program on the host holds port 9872. Owner approved socket2 0.6 as a direct dependency (2026-11-24).

Controller rulings (binding for the implementers; each with its cost if wrong):

- Ruling: .NET Framework formatting of a negative value that rounds to zero is pinned as `-000` / `-0.0` (Task 1) and logged as an unverified finding — no .NET Framework 4.7.2 run is available — cost if wrong: a rotator/logger receives `-000` instead of `000` for azimuths in (-0.5, 0); one formatter line to change.
- Ruling: SINGLE while paused drives the outputs, 1:1 with `bw_Track` (Task 6, #181 updated) — the port policy is 1:1 for behaviour and the 3c deviation existed only because 3c had no outputs — cost if wrong: right-click on DX while paused sends one-off positions to the outputs.
- Ruling: the track step stays in the UI frame loop for 4a; the app requests a repaint every second while `track.activate` is on. Moving the stepping into `TrackService` is deferred to 4c, where the rotator outputs land — in 4a the outputs are UDP and azel.dat, the rework touches the whole 3c tracking design — cost if wrong: with a minimised window some platforms stop calling `update`, so UDP/azel.dat updates pause until the window is shown again; README states this limitation for 4a.
- Ruling: each track output gets its own error handling, unlike the one shared C# try block where an error skips the later outputs — a GUI/integration robustness deviation, not prediction behaviour; logged as a finding — cost if wrong: an output keeps updating where upstream would have skipped it after another output failed.
- Ruling: ASWATCHLIST/ASADDWATCH are capped at `watchlist_max_count` (finding 15) — network input must not grow the watchlist without bound — cost if wrong: a Win-Test client sending more entries than the cap loses the extra ones.
- Ruling: the Network tab ships without its HTTP group until 4d — the web server does not exist before 4d and no UI may pretend to work — cost if wrong: none; the group appears in 4d.
