# Stage 5a — Developer Tools (`wintest-sim`, `horizon-gen`, `catcheck`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stage 5a of the spec, "Suite tools", as split in the survey: the WinTestSimulator and HorizonGenerator as `airscout-cli` subcommands, CATCheck as the `catcheck` binary over `as-cat`, and the three `Locator_*` members in `/settings.json`, with the binding owner decisions Q4, Q5, Q6 and Q10.

**Architecture:**
- **`airscout-cli wintest-sim`** (`crates/as-cli/src/wintest_sim.rs`): one Win-Test datagram built with `as_wintest::encode`, sent from an ephemeral socket with `SO_BROADCAST`. `--listen <secs>` sends from a shared socket on `<bind>:<port>` instead and prints the datagrams that arrive until the deadline. The shared-socket code moves from `airscout` into `as_wintest::socket`, so the app and the tool bind the same way.
- **`airscout-cli horizon-gen`** (`crates/as-cli/src/horizon_gen.rs`): for each station row of `stations.db3`, 360 elevation paths (`create_path_from_bearing`, not saved) and, per bearing, `epsilon_max_from_path` for the 12 real bands on one shared, truncating path (1:1). One CSV per station with 12 correct band columns (Q5).
- **`catcheck`** (new crate `crates/catcheck`, binary `catcheck`): a CLI over the `as_cat::Rig` trait and the `CatWorker`. The connection comes from the app's settings file, with flags on top. The settings-to-`CatConfig` mapping moves from `airscout` into `as-cat` behind a cargo feature `settings`, so the app and the tool build the same config.
- **`Locator_*` members** (`crates/airscout/src/web_settings_json.rs`): three members in `Settings.Designer.cs` order. Two are fixed port constants that move to `as-settings`.
- `as-settings` stops showing the text of a settings file in its parse error. `catcheck` reads that file, and a line of it can hold a credential.

**Tech Stack:** Rust 1.95+, std threads, clap 4, serde_json, socket2 0.6, serial2 0.2, assert_cmd 2 (dev), tempfile (dev). No new external crate: every dependency is already in the workspace and in `Cargo.lock`.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`:
- stage 5 "Suite tools" (`:113`);
- the binary rows `horizon-gen` … (bins), `wintest-sim` (bin), `catcheck` (bin) (`:85-88`), changed by owner decision Q6 (Task 5);
- `as-cat`: "rigctld TCP (all OS), optional hamlib FFI, OmniRig only `cfg(windows)`" (`:76`).

**Inputs:**
- Survey `docs/superpowers/surveys/2026-11-24-suite-tools-survey.md`: sections 4, 7, 8, 10, 11 (S10, S19), 12 and "Decisions (project owner, 2026-11-24)". Corrections to the survey are in the closing section; the plan follows the corrected facts.
- Ledger `docs/superpowers/ledgers/2026-11-24-stage4d-web.md`: the shared test-code rule (`#[path]` include, C14) and the cite rule (C15).
- Upstream `target/upstream/AirScout/` @ `91b02cb`, read-only. C# references are relative to `target/upstream/AirScout/` and start with the project directory (`WinTestSimulator/`, `HorizonGenerator/`, `CATCheck/`, `WinTest/`, `ScoutBase/…`, `AirScout/…`, `AirScoutViewClient/`).

**Stage split:**
- **5a (this plan):** `wintest-sim`, `horizon-gen`, `catcheck`, the `Locator_*` members, README, spec and findings.
- **Not in 5a:** `elevation-tile-gen` and `elevation-coverage` (5b), `as-maps`, `maptile-gen` and the mirror preloader (5c), `airscout-planeserver` (5d). ViewClient and RainScout follow stage 6; their S-ids (S6-S9, S20-S24) are logged when they are ported.

## Global Constraints

Everything in the 4b, 4c and 4d Global Constraints still applies where it concerns the touched crates. Changes and additions:

- **Port policy:**
  - Prediction behaviour (horizon maths, the shared truncated path, the C# parameter defaults) is ported 1:1 even when wrong and logged in `docs/logic-findings.md`. Logic fixes are stage 6.
  - Tool-layer bugs (UI, file handling, error handling, sockets) are not reproduced. Each gets a finding: "Not reproduced", "Port deviation" or "New design".
  - The next finding number is **#278**; check the last row first. Task 5 assigns the numbers in its list order.
  - Every C# `file:line` in code comments and findings is checked against `target/upstream`.
- **Owner decisions (binding):**
  - Q5: `horizon-gen` writes the 12 correct band columns; the deviation is logged (S10).
  - Q6: `horizon-gen` and `wintest-sim` are `airscout-cli` subcommands; `catcheck` is a separate binary. The spec's binary list changes (Task 5).
  - Q10: `catcheck` is a command-line tool over `as-cat`. No INI engine, no shipped `.ini` rig files.
  - Q4: the three `Locator_*` members go into `/settings.json` now.
- **Secrets:**
  - No secret in code, tests, fixtures or docs.
  - Credentials come only from the user's settings and are never logged or shown. `catcheck` prints nothing from the settings file except the rig ident (`[rigctld] host:port`, `[OmniRig] Rig n`, the hamlib model).
  - A settings parse error shows the path, the line and the column, never the file's text (Task 4).
- **Network:**
  - No 5a tool makes an HTTP request. `as_net::user_agent()` stays the only User-Agent of the project; no tool sends another one.
  - `horizon-gen` is offline: it reads `stations.db3` and the elevation store and downloads nothing.
  - `wintest-sim` sends one UDP datagram per run. It opens a listening socket only with `--listen`, only for the given seconds (1 to 3600). README documents both (Task 5).
  - `catcheck` opens only the configured CAT connection (rigctld TCP, a hamlib device, the local OmniRig COM server). It listens on nothing.
  - Never write the upstream author's private LAN address anywhere (code, tests, docs). Examples use `localhost` or `127.0.0.1`.
- **Rig safety (4b hard rule):** no frequency ≤ 0 Hz and none above `as_cat::MAX_FREQ_HZ` (100 GHz) is ever written to a rig. `catcheck` checks every frequency, including every Doppler step, before it connects for a set.
- **Untrusted input:** datagrams received by `wintest-sim --listen` and rigctld replies never panic and never allocate in proportion to a received value. The receive buffer is 65 536 bytes; at most 1000 datagrams are listed.
- **Threads:** the tools are single-threaded. A rig is created and used on the main thread only (OmniRig COM initialises on that thread, `as-cat/src/omnirig_com.rs:50-85`). Waits are cut into steps of at most 250 ms.
- **Platform:** macOS, Linux and Windows. OmniRig is `cfg(windows)` inside `as-cat`; `catcheck` itself has no `cfg` code. The hamlib engine needs the cargo feature `catcheck/hamlib`, which only forwards `as-cat/hamlib`.
- **Unsafe:** `catcheck` and `as-cli` have `#![forbid(unsafe_code)]`. `as-wintest` keeps it.
- **Cargo:**
  - Every cargo command carries `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
  - Disk: total under 120 GB. Check `du -sh target` before each task. Above 40 GB run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clean --profile dev`; add `--target-dir target/linux` or `--target-dir target/wingnu` for the docker dirs. **Never** run plain `cargo clean`: it deletes `target/upstream`.
  - Tasks 2, 3 and 4 change local dependency edges. Each runs one `cargo build` without `--locked` and commits `Cargo.lock`. Check: `git diff Cargo.lock | grep '^+name'` prints nothing in Tasks 2 and 3 and only `+name = "catcheck"` in Task 4.
  - New entries: `as-wintest` gets `socket2.workspace = true`; `as-cli` gets `as-wintest` and `as-stations`; `as-cat` gets `as-settings = { workspace = true, optional = true }` and the feature `settings = ["dep:as-settings"]`; `airscout` uses `as-cat = { workspace = true, features = ["settings"] }` and drops `socket2` once nothing in `crates/airscout/src` uses it.
- **Gate:**
  - `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo fmt --all --check && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy --workspace --all-targets --locked -- -D warnings && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace --locked`.
  - hamlib feature check (4b, unchanged): `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy -p as-cat -p airscout --all-targets --locked --features as-cat/hamlib,airscout/hamlib -- -D warnings`.
  - Linux docker gate (4b, unchanged): `docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target/linux -e CARGO_BUILD_JOBS=2 -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 rust:1.95 bash -c 'apt-get update && apt-get install -y libasound2-dev pkg-config && rustup component add rustfmt clippy && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked && cargo clippy -p as-cat -p airscout --all-targets --locked --features as-cat/hamlib,airscout/hamlib -- -D warnings'`.
  - Windows check (4c, unchanged): `docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target/wingnu -e CARGO_BUILD_JOBS=2 -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 rust:1.95 bash -c 'apt-get update && apt-get install -y gcc-mingw-w64-x86-64 && rustup target add x86_64-pc-windows-gnu && rustup component add clippy && cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings && cargo build --workspace --all-targets --locked --target x86_64-pc-windows-gnu'`. Tasks 2, 4 and 5 run it. `target/wingnu` counts toward the disk budget.
  - `#[cfg(windows)]` tests run only in the hosted Windows CI job. The stage is not done until that job is green on the branch.
- **Tests:**
  - No network beyond 127.0.0.1. No test sends to `255.255.255.255`: every `wintest-sim` test passes `--addr 127.0.0.1 --bind 127.0.0.1`. Ephemeral ports (port 0, or bind-and-release for a known free port).
  - No real serial port, rig, COM server or rigctld. rigctld tests use the 4b fake, included with `#[path = "../../as-cat/tests/common/fake_rigctld.rs"] mod fake_rigctld;`.
  - Every `catcheck` test passes `--settings <tempdir>/settings.toml`, so the user's real settings file is never read.
  - Synthetic data only: calls `TE1ST`/`TE2ND`, locators `JO50AA`/`JO60AA`/`JO61AA`/`JO50XX`/`JO72AA`, 144 300 000 Hz, secrets `SECRET-TEST-1` … `SECRET-TEST-9`. No real call sign, position or e-mail.
  - Waits bounded at 10 s, returning early.
- **File size:** new files stay under about 500 lines. `crates/as-cli/src/main.rs` (604) grows only by the two `mod` lines, the two `Cmd` variants and their match arms (≤ 620).
- **Docs and commits:**
  - README and findings use plain English, short sentences and one idea per sentence (`docs/logic-findings.md` table format: `| # | Location | What looks wrong | What the port does |`).
  - Commits are signed: `export SSH_AUTH_SOCK=~/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock; git commit -S -m "…"`.
  - Linear history: no merge commits; rebase onto `main` if it moved.

## Review Focus

1. **Garbage arrives on the Win-Test port while `wintest-sim --listen` runs** (an empty datagram, `abc`, 9000 bytes of `0xFF`, text without `": "`). Expected: each is listed as `{"from", "error", "len"}`, no panic, listening goes on until the deadline, and valid datagrams after it are still listed. Test: Task 2 `listen_survives_garbage`.
2. **The rigctld daemon is down or hangs** (refused port, a server that reads and never answers). Expected: one status line (`NOCAT …` or `OFFLINE`) within the timeout plus 1 s, exit code 2, no hang. Tests: Task 4 `status_without_rigctld_reports_nocat`, `silent_rigctld_times_out`.
3. **A frequency outside 1 Hz to 100 GHz** (from `set-freq 0`, `set-freq -5`, `set-freq 100000000001`, or a Doppler step that goes below 1 Hz). Expected: exit 1 with `frequency must be 1 to 100000000000 Hz`, and the rig receives no set command at all. Test: Task 4 `out_of_range_freq_never_sent`.
4. **The settings file has a syntax error on a line with a credential.** Expected: the error names the path, line and column; neither stderr nor the app's error text contains the credential. Tests: Task 4 `parse_error_has_no_source_text` (`as-settings`), `broken_settings_never_echo_secret` (`catcheck`).
5. **Station rows whose call is not a safe file name, two rows that map to one name, a station outside the elevation data** (`TE1ST/P`, `TE1ST_P`, `../TE1ST`, a station at `JO72AA`). Expected: every CSV is written inside `--out` under a unique name; the station without data gets a horizon of zeros (1:1) and is listed under `incomplete`. Tests: Task 3 `file_names_are_safe_and_unique`, `outside_elevation_data_is_listed_incomplete`.

---

### Task 1: `Locator_*` members in `/settings.json`

**Files:**
- Modify:
  - `crates/as-settings/src/gui.rs` (two constants), `crates/as-settings/src/lib.rs:19` (re-export);
  - `crates/airscout/src/ui/options/stations.rs:14-15`, `:127-136` (use the constants);
  - `crates/airscout/src/web_settings_json.rs:173-174` (three fields), `:248` (their values).
- Test: `crates/airscout/tests/web_api.rs` (`SETTINGS_MEMBERS` at `:56-97`, `settings_json_members_exact` at `:108`)

**Interfaces (`AirScout/Properties/Settings.Designer.cs:1496`, `:1508`, `:1520`; `AirScout/Properties/Settings.settings:368-376`; `AirScoutViewClient/MapViewDlg.cs:672-674`):**
- Produces (`as-settings`):
  - `pub const LOCATOR_MAX_LENGTH: i32 = 10;` (`Locator_MaxLength` default, `Settings.settings:369`).
  - `pub const LOCATOR_AUTO_LENGTH: bool = true;` (`Locator_AutoLength` default, `:372`).
  - Doc line on both: a C# setting without an Options field in the port, fixed at its C# default.
- `stations.rs`: `const LOCATOR_PRECISION: i32 = as_settings::LOCATOR_MAX_LENGTH / 2;`, and `loc_text_for` passes `as_settings::LOCATOR_AUTO_LENGTH` instead of the literal `true`. Behaviour is unchanged.
- `SettingsJson` gets three fields after `planes_filter_max_circumcircle` (`:1304`) and before `watchlist` (`:1531`), in this order:
  - `/// :1496, \`decimal\`; written like a double (\`10.0\`)` `#[serde(rename = "Locator_MaxLength")] locator_max_length: Num` = `Num(f64::from(LOCATOR_MAX_LENGTH))`;
  - `/// :1508` `#[serde(rename = "Locator_AutoLength")] locator_auto_length: bool` = `LOCATOR_AUTO_LENGTH`;
  - `/// :1520` `#[serde(rename = "Locator_SmallLettersForSubsquares")] locator_small_letters_for_subsquares: bool` = `s.gui.locator_small_letters_for_subsquares`.

- [ ] **Step 1: Write the failing tests** in `web_api.rs`:
  - `SETTINGS_MEMBERS` becomes `[&str; 43]` with `"Locator_MaxLength"`, `"Locator_AutoLength"`, `"Locator_SmallLettersForSubsquares"` after `"Planes_Filter_Max_Circumcircle"`. `settings_json_members_exact` then checks key set and text order as today.
  - In `settings_json_members_exact` add: `assert_eq!(o["Locator_MaxLength"], 10.0)`; `assert!(text.contains("\"Locator_MaxLength\": 10.0,"))`; `assert_eq!(o["Locator_AutoLength"], true)`; `assert_eq!(o["Locator_SmallLettersForSubsquares"], false)`.
  - New `settings_json_locator_small_letters`: `s.gui.locator_small_letters_for_subsquares = true` gives `o["Locator_SmallLettersForSubsquares"] == true`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test web_api`. Expected: FAIL (key set differs).
- [ ] **Step 3: Implement** the constants, the `stations.rs` change and the three fields.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings -p airscout`. Expected: PASS, including `ui_stations` (locator text unchanged).
- [ ] **Step 5: Commit** `feat(airscout): serve the three Locator members in /settings.json`.

### Task 2: `airscout-cli wintest-sim`

**Files:**
- Create: `crates/as-wintest/src/socket.rs`, `crates/as-cli/src/wintest_sim.rs`
- Modify:
  - `crates/as-wintest/Cargo.toml` (`socket2`), `crates/as-wintest/src/lib.rs` (`pub mod socket;`);
  - `crates/airscout/src/wintest_service.rs:16`, `:95-114` (`bind_socket` delegates), `crates/airscout/Cargo.toml` (drop `socket2`);
  - `crates/as-cli/Cargo.toml` (`as-wintest`), `crates/as-cli/src/main.rs` (`mod wintest_sim;`, `Cmd::WintestSim(wintest_sim::WintestSimArgs)`, match arm), `Cargo.lock`.
- Test: `crates/as-cli/tests/wintest_sim.rs`

**Interfaces (`WinTestSimulator/MainDlg.cs:22-41`; `WinTestSimulator/Properties/Settings.settings:5-19`; `WinTest/WinTest.cs:105-148`; `AirScout/MapDlg.cs:7032`):**
- Consumes: `as_wintest::{WtKind::parse, WtKind::wire_name, WtMessage, encode, decode, Decoded}` (`crates/as-wintest/src/lib.rs:64`, `:112`, `:140`).
- Produces (`as_wintest::socket`):
  - `pub fn bind_shared(addr: SocketAddr) -> io::Result<UdpSocket>`: the body of today's `airscout::wintest_service::bind_socket` without the read timeout (address reuse, `SO_REUSEPORT` on the same unix `cfg` list, bind). Doc comment keeps the `MapDlg.cs:7032` cite.
  - `airscout::wintest_service::bind_socket(addr)` keeps its signature: `bind_shared(addr)`, then `set_read_timeout(Some(READ_TIMEOUT))`.
- Produces (`wintest_sim.rs`):
  - `#[derive(clap::Args)] pub struct WintestSimArgs` with the C# defaults:
    - `--msg` `String` = `"ASADDWATCH"`: a `WTMESSAGES` name in any case, or its decimal value;
    - `--from` = `"Test"`, `--to` = `"AS"`, `--data` = `""`;
    - `--port` `u16` = `9872`;
    - `--addr` `Ipv4Addr` = `255.255.255.255`, `--bind` `Ipv4Addr` = `0.0.0.0`;
    - `--listen` `Option<u64>`: seconds to listen on `<bind>:<port>` after the send.
  - `pub const MAX_RECEIVED: usize = 1000;`
  - `pub fn message_of(a: &WintestSimArgs) -> Result<WtMessage, String>`:
    - `kind = WtKind::parse(&a.msg.trim().to_uppercase())` (C# `ToUpper().Trim()`, `MainDlg.cs:25`).
    - `None` and `Unknown` give `Err(format!("unknown Win-Test message: {name}; known: STATUS, PKTRCVD, SETAZIMUTH, SETELEVATION, SETAZEL, ASADDWATCH, ASREMOVEWATCH, ASWATCHLIST, ASSETPATH, ASSHOWPATH, ASNEAREST"))`, with `name` the trimmed, upper-cased input.
    - `extra` is empty. `encode` sets the checksum (C# `HasChecksum = true`, `:30`).
  - `pub fn run(a: &WintestSimArgs) -> crate::Res<serde_json::Value>`:
    - `--port 0` gives `"port must be 1-65535"`. `--listen` outside 1..=3600 gives `"--listen must be 1 to 3600 seconds"`. Both before any socket.
    - Without `--listen`: `UdpSocket::bind((a.bind, 0))`, `set_broadcast(true)`, one `send_to((a.addr, a.port))`, close.
    - With `--listen`: `bind_shared((a.bind, a.port))`, `set_broadcast(true)`, one `send_to`, then receive until the deadline with read timeouts of at most 250 ms into a 65 536-byte buffer. Skip the own echo: a datagram whose source port is `a.port` and whose bytes equal the sent bytes. Stop listing after `MAX_RECEIVED` entries but keep the deadline.
    - Output: `{"sent": {"to": "<addr>:<port>", "text": <bytes without the checksum byte, lossy UTF-8>, "checksum": <last byte>, "bytes": <len>}}`, plus `"received": [...]` with `--listen`. An entry is `{"from": "<ip>:<port>", "kind": wire_name, "src", "dst", "data", "checksum_ok"}`, or `{"from", "error": <DecodeError text>, "len"}`.

- [ ] **Step 1: Write the failing tests** (`run(args) -> Value` and `run_err(args) -> String` helpers as in `cli.rs`; `free_udp_port()` by bind-and-release on `127.0.0.1:0`; a receiver `UdpSocket::bind("127.0.0.1:0")` with a 10 s read timeout):
  - `sends_csharp_bytes`: `--msg assetpath --from Test --to AS --data TE1ST,JO50AA,TE2ND,JO60AA --addr 127.0.0.1 --bind 127.0.0.1 --port <rx>`. The received bytes equal `encode(&WtMessage { kind: WtKind::AsSetPath, src: "Test".into(), dst: "AS".into(), data: "TE1ST,JO50AA,TE2ND,JO60AA".into(), extra: String::new() }).unwrap()`. `v["sent"]["bytes"]` equals their length, and `v["sent"]["text"]` starts with `ASSETPATH: "Test" "AS" "TE1ST`.
  - `defaults_match_csharp`: only `--addr`, `--bind`, `--port`. `decode` of the received bytes gives `AsAddWatch`, `"Test"`, `"AS"`, data `""`, `checksum_ok == true`.
  - `msg_trimmed_and_upper_cased`: `--msg " asnearest "` gives `AsNearest`, and the datagram ends with one checksum byte after the closing `"` (the single `?` placeholder, `WinTest.cs:118`).
  - `numeric_msg_accepted`: `--msg 252` gives `AsSetPath`.
  - `unknown_msg_is_error`: `--msg FOO`, `--msg none`, `--msg UNKNOWN` and `--msg 7` each exit non-zero; stderr contains `unknown Win-Test message: FOO` (and `NONE`, `UNKNOWN`, `7`). The receiver gets nothing within 500 ms.
  - `bad_port_and_listen_rejected`: `--port 0`, `--listen 0` and `--listen 3601` exit non-zero with the two texts.
  - `listen_reports_replies_not_own_echo`: port P free. Spawn `airscout-cli wintest-sim --addr 127.0.0.1 --bind 127.0.0.1 --port P --listen 2` (`assert_cmd::cargo::cargo_bin("airscout-cli")`). From an ephemeral socket, send the bytes of `WtMessage { kind: AsNearest, src: "AS", dst: "Test", data: "TE1ST,JO50AA", .. }` to `127.0.0.1:P` every 100 ms for 1.5 s. The output has at least one `received` entry. Every entry has `kind == "ASNEAREST"`, `src == "AS"`, `checksum_ok == true`; no entry has `kind == "ASADDWATCH"`.
  - `listen_shares_port_with_receiver`: hold `as_wintest::socket::bind_shared("127.0.0.1:P")` as AirScout would. `wintest-sim --listen 1` on P exits 0.
  - `listen_survives_garbage` (Review Focus 1): as `listen_reports_replies_not_own_echo`, but each round sends an empty datagram, `abc`, 9000 bytes of `0xFF` and then the valid ASNEAREST. Exit 0. `received` holds entries with `"error"` and at least one valid `ASNEAREST` entry. (9000 bytes stays below the macOS loopback datagram limit.)
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cli --test wintest_sim`. Expected: FAIL (no subcommand).
- [ ] **Step 3: Implement** `socket.rs`, the `bind_socket` delegation and `wintest_sim.rs`. Run one `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build -p as-cli -p airscout` without `--locked`, then the `Cargo.lock` check from Global Constraints.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-wintest -p as-cli -p airscout`, then the Windows check. Expected: PASS, including `wintest_service` and `integrations`.
- [ ] **Step 5: Commit** `feat(as-cli): wintest-sim subcommand; shared Win-Test socket in as-wintest`.

### Task 3: `airscout-cli horizon-gen`

**Files:**
- Create: `crates/as-cli/src/horizon_gen.rs`, `crates/as-cli/tests/common/mod.rs` (`fixture_data_dir`, moved from `cli.rs:9-26`)
- Modify: `crates/as-cli/Cargo.toml` (`as-stations`), `crates/as-cli/src/main.rs` (`mod horizon_gen;`, `Cmd::HorizonGen(horizon_gen::HorizonGenArgs)`, match arm), `crates/as-cli/tests/cli.rs` (`mod common;`), `Cargo.lock`
- Test: `crates/as-cli/tests/horizon_gen.rs`

**Interfaces (`HorizonGenerator/MainDlg.cs:36-100`; `ScoutBase/ScoutBase.Propagation/PropagationHorizon.cs:153-185`; `ScoutBase/ScoutBase.Elevation/ElevationDatabase.cs:738-744`, `:776-787`, `:1365-1369`; `ScoutBase/ScoutBase.Stations/StationDatabase.cs:633-643`):**
- Consumes:
  - `StationDb::open(&Path)`, `StationDb::location_get_all(None) -> Result<Vec<Location>, StationError>` (ordered by call, `crates/as-stations/src/db.rs:210`).
  - `ElevationStore::open(&Path)`, `ElevationStore::elevation(model, lat, lon, true) -> Result<i16, ElevationError>`, `ElevationStore::create_path_from_bearing(model, lat, lon, bearing, distance_km, step_width_m, false, None) -> Result<Option<ElevationPath>, ElevationError>` (`crates/as-elevation/src/path.rs:258`; `ElevationPath.path: Vec<i16>`, `.valid`).
  - `as_propagation::epsilon_max_from_path(h1, &mut Vec<i16>, dist, step_width_m, freq_ghz, f1_clearance, max_elv, re) -> HorizonPoint` (`crates/as-propagation/src/horizon.rs:117`).
  - `Band::all_except_none_and_all()`, `Band::to_ghz()`, `Band::to_mhz()`, `Band::name()`; `as_settings::default_band_table()`; `as_geo::latlon::EARTH_RADIUS_KM`; `as_wintest::dotnet::fixed(x, 8)`; `crate::parse_model`.
- Produces (`horizon_gen.rs`):
  - `#[derive(clap::Args)] pub struct HorizonGenArgs` with the C# values as defaults (`MainDlg.cs:45-52`):
    - `--data` `PathBuf` (required): elevation store and `stations.db3`;
    - `--model` = `"srtm1"`, `--height` `f64` = `40.0` [m], `--dist` `f64` = `500.0` [km], `--k` `f64` = `1.33`, `--max-elv` `f64` = `4797.0` [m, 0 = no cut];
    - `--legacy-f1` (flag): the C# thresholds instead of the default band table;
    - `--stations` = `"all"`: `all` in any case, or one call;
    - `--out` `PathBuf` = `"."`.
  - `pub fn csv_header() -> String`: `bearing[deg]`, then `;eps_{band.name()}[deg]` for each of the 12 bands. That is the C# header text (`:80`) with `eps[144M[deg]` corrected to `eps_144M[deg]`.
  - `pub fn legacy_f1(b: Band) -> f64` (`:66-70`): `to_mhz() <= 144.0` gives 0.2, `<= 432.0` gives 0.4, else 0.6.
  - `pub fn f1_table(legacy: bool) -> [f64; 12]`: per band in band order, `legacy_f1(b)` or `default_band_table()[&b].f1_clearance`.
  - `pub fn horizon_rows(store: &ElevationStore, model: ElevationModel, lat: f64, lon: f64, h1: f64, dist_km: f64, max_elv: f64, re: f64, f1: &[f64; 12]) -> Result<(Vec<[f64; 12]>, bool), ElevationError>`: 360 rows in degrees, and whether every path was valid. Algorithm (the shared path is the 1:1 point):

    ```rust
    let step = model.default_step_width_m();
    for j in 0..360 {
        let ep = store
            .create_path_from_bearing(model, lat, lon, f64::from(j), dist_km, step, false, None)?
            .expect("no cancel flag");
        valid &= ep.valid;
        let mut elv = ep.path; // one buffer for all bands: a cut shortens it for the next band
        for (i, b) in Band::all_except_none_and_all().iter().enumerate() {
            let hp = epsilon_max_from_path(h1, &mut elv, dist_km, step, b.to_ghz(), f1[i], max_elv, re);
            row[i] = hp.eps_min / std::f64::consts::PI * 180.0; // C# order of operations (:86)
        }
    }
    ```
  - `pub fn csv_text(rows: &[[f64; 12]]) -> String`: `csv_header()`, then per row `{j};` and the 12 values as `fixed(v, 8)` joined by `;`. Every line ends with `\n`.
  - `pub fn file_name(call: &str, loc: &str) -> String`: `{call}_{loc}.csv`, every character outside `A-Z`, `a-z`, `0-9` and `-` replaced by `_`.
  - `pub fn run(a: &HorizonGenArgs) -> crate::Res<serde_json::Value>`:
    - `<data>/stations.db3` missing gives `"no station database at {path}"`, and no file is created.
    - The filter gives no row: `"no station matches {x} in {path}"`.
    - `h1 = f64::from(store.elevation(model, lat, lon, true)?) + a.height` (C# indexer, `setinvalidtozero = true`, `ElevationDatabase.cs:738-744`); `re = EARTH_RADIUS_KM * a.k`.
    - It creates `--out` if needed. A name already written in this run gets `-2`, `-3`, … before `.csv`.
    - Per station: one stderr line `horizon {call} {loc}: {file}`. A station error goes to stderr as `{call} {loc}: {e}`, and the run goes on.
    - After all stations: any error gives `Err(format!("{n} of {m} stations failed"))`. Otherwise `{"model": dir_name, "stations": m, "files": [...], "incomplete": [file names with an invalid path]}`.

- [ ] **Step 1: Write the failing tests** (`common::fixture_data_dir()` with the GLOBE squares JO50, JO51, JO60, JO61; stations written with `StationDb::open(&dir.join("stations.db3"))` and `location_insert_or_update_if_newer(&Location::new(call, loc, lat, lon, GeoSource::FromUser, 0, Utc::now()))` at the locator centre; every run uses `--model globe --dist 20 --out <tempdir>`):
  - `writes_one_csv_per_station`: `TE1ST` at `JO61AA` and `TE2ND` at `JO50XX`. Files `TE1ST_JO61AA.csv` and `TE2ND_JO50XX.csv` exist. Each has 361 lines. Line 1 is exactly `bearing[deg];eps_50M[deg];eps_70M[deg];eps_144M[deg];eps_432M[deg];eps_1.2G[deg];eps_2.3G[deg];eps_3.4G[deg];eps_5.7G[deg];eps_10G[deg];eps_24G[deg];eps_47G[deg];eps_76G[deg]`. Each data line has 13 `;`-separated fields; field 0 is the row index 0 to 359; every value has exactly 8 digits after the `.`. `v["stations"] == 2`.
  - `columns_match_library`: for `TE1ST`, bearings 0, 90 and 271: recompute in the test with `ElevationStore`, `create_path_from_bearing` and one shared `elv` across the 12 bands with `default_band_table()` F1, `h1 = elevation + 40`, `re = 6371 * 1.33`, `max_elv = 4797`. Each CSV field equals `fixed(eps_min / PI * 180.0, 8)`.
  - `legacy_f1_uses_csharp_thresholds`: two runs, with and without `--legacy-f1`. The `eps_50M` column differs in at least one row (0.2 against 0.1). The `eps_1.2G` column is equal in every row (0.6 in both).
  - `bnone_pass_takes_no_point` (evidence for the Q5 finding): on the bearing-90 path of `TE1ST`, `epsilon_max_from_path(h1, &mut elv, 20.0, 1000.0, 0.0, 0.2, 4797.0, re)` returns `eps_min == f64::MIN`, and `elv.len()` is unchanged.
  - `stations_filter_and_missing_db`: `--stations te2nd` writes only `TE2ND_JO50XX.csv`. `--stations TE9XX` exits non-zero with `no station matches TE9XX`. A data dir without `stations.db3` exits non-zero with `no station database at`, and `stations.db3` still does not exist.
  - `file_names_are_safe_and_unique` (Review Focus 5): rows `TE1ST/P`, `TE1ST_P` and `../TE1ST` at `JO61AA`. The out dir holds exactly `TE1ST_P_JO61AA.csv`, `TE1ST_P_JO61AA-2.csv` and `___TE1ST_JO61AA.csv`; nothing is written outside it.
  - `outside_elevation_data_is_listed_incomplete` (Review Focus 5): `TE2ND` at `JO72AA` (no fixture tile). Exit 0. The file is written with 361 lines, and `v["incomplete"]` contains `TE2ND_JO72AA.csv`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cli --test horizon_gen`. Expected: FAIL (no subcommand).
- [ ] **Step 3: Implement.** Run one `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build -p as-cli` without `--locked`, then the `Cargo.lock` check. If `bnone_pass_takes_no_point` fails, keep the test with the observed values and report it: Task 5 finding 1 must then say the 1:1 columns are not a plain shift.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cli`. Expected: PASS. Report `wc -l crates/as-cli/src/main.rs` (≤ 620).
- [ ] **Step 5: Commit** `feat(as-cli): horizon-gen subcommand with the correct band columns`.

### Task 4: `catcheck` over `as-cat`

**Files:**
- Create:
  - `crates/as-cat/src/from_settings.rs` (`#[cfg(feature = "settings")]`);
  - `crates/catcheck/Cargo.toml`, `crates/catcheck/src/main.rs`.
- Modify:
  - `crates/as-settings/src/lib.rs:36-51` (`SettingsError::Parse`), `:370-394` (`load_or_default`);
  - `crates/as-cat/Cargo.toml` (feature `settings`), `crates/as-cat/src/lib.rs` (`#[cfg(feature = "settings")] pub mod from_settings;`);
  - `crates/airscout/src/cat_service.rs:23-89` (the mapping moves out; `pub use as_cat::from_settings::cat_config;`), `crates/airscout/Cargo.toml`, `Cargo.lock`.
- Test: `crates/as-settings/tests/settings_5a.rs`, `crates/catcheck/tests/cli.rs`

**Interfaces (`CATCheck/MainDlg.cs:391-398`, `:676-686`, `:688-699`, `:817-845`; `CATCheck/MainDlg.Designer.cs:1200`, `:1209`):**
- Produces (`as-settings`):
  - `SettingsError::Parse { path: PathBuf, line: usize, column: usize }` with `#[error("cannot parse {path} at line {line}, column {column} (the text is not shown: it may hold a password)")]`. `line` and `column` are 1-based, from `toml::de::Error::span()` over the file text; without a span both are 0. The toml message is dropped: it can quote a value.
  - The app's `load_settings` (`app/mod.rs:97-102`) is unchanged; its text gets safe with this.
- Produces (`as_cat::from_settings`, feature `settings`):
  - `pub fn cat_config(s: &as_settings::Settings) -> CatConfig`, moved verbatim with `parity`, `stop_bits`, `flow`, `MIN_MS` and `MAX_MS` and their doc comments. `airscout::cat_service::cat_config` stays importable through `pub use`, so `crates/airscout/tests/cat_service.rs` is unchanged.
- Produces (`catcheck`, `#![forbid(unsafe_code)]`; `[features] hamlib = ["as-cat/hamlib"]`; deps `as-cat` with `settings`, `as-settings`, `clap`, `serial2`; dev deps `assert_cmd`, `tempfile`):
  - Global flags (override the settings file):
    - `--settings <FILE>` (default `Settings::default_path()`; a missing file gives the defaults);
    - `--engine <none|rigctld|hamlib|omnirig>` (clap `ValueEnum`, mapped to `CatEngine`);
    - `--host`, `--port` (rigctld); `--model`, `--serial-port`, `--baud` (hamlib); `--rig` (OmniRig 1-4); `--timeout-ms`.
  - They are written into `s.cat`, then `cfg = cat_config(&s)`. So the clamps of the app apply.
  - Engine checks before any connect:
    - `Engine::None`: `"no CAT engine selected (settings [cat] engine or --engine)"`;
    - `Hamlib` and `!hamlib_available()`: `"the hamlib engine is not compiled in; build catcheck with --features hamlib (Linux and macOS)"`;
    - `OmniRig` and `!omnirig_available()`: `"OmniRig is available on Windows only"`.
  - Subcommands:
    - `status [--count N] [--interval-ms MS]`: `--count 0` (default) polls until Ctrl-C. `--interval-ms` defaults to `cfg.update`.
    - `set-freq <HZ> [--tx]` (`allow_negative_numbers`): `set_rx_frequency`, or `set_tx_frequency` with `--tx`.
    - `set-mode <MODE>`: a `RigMode::name()` in any case except `NONE` and `OTHER`. Anything else gives `"unknown mode: {x} (CW, CW_R, LSB, USB, DIG, DIG_R, AM, FM)"`.
    - `split <on|off>`, `rit <on|off>`: `set_split(SplitOn|SplitOff)`, `set_rit(RitOn|RitOff)`.
    - `doppler-test [--strategy A|B|C|D] [--shift-hz N] [--steps N] [--step-ms MS] [--base-hz HZ]`. Defaults: A, 1000, 5, 1000. Ranges: steps 1-100, step-ms 100-10000.
    - `list-ports`: `serial2::SerialPort::available_ports()`, one path per line, or `no serial ports found`.
  - Output lines (exact; tests assert them):
    - first `rig: {ident.rig_type}`;
    - online: `ONLINE rx={rx} tx={tx} mode={mode.name()} split={ON|OFF|NONE} rit={ON|OFF|NONE}`;
    - otherwise `{status.name()}` plus ` {take_error text}` when there is one.
  - Exit codes: 0 done; 1 bad input, a rejected set or a failed restore; 2 the rig is not ONLINE (status: at the last poll; set commands: at the `get_status(Doppler::None)` before the set, because setters never reconnect).
  - `pub fn check_freq(hz: i64) -> Result<i64, String>`: `1..=MAX_FREQ_HZ`, else `"frequency must be 1 to 100000000000 Hz"`. Every set command and every Doppler step goes through it before the rig is opened.
  - `doppler-test` runs in this order:
    1. With `--base-hz`, check every step first.
    2. A rig from `make_rig(&cfg)` must be ONLINE. Read mode, split, RIT, RX and TX and print `saved: rx=… tx=… mode=… split=… rit=…`. Drop the rig.
    3. `base = --base-hz` or the saved RX. Check every step `rx = base + k·shift`, `tx = base − k·shift`, `k = 1..=steps` (checked arithmetic) before anything is set.
    4. `w = CatWorker::new(factory(cfg.clone()), cfg.update)`. For each `k`: `w.pass(DopplerTarget { strategy, rx_hz, tx_hz })`, print its events and `step {k}: rx={rx} tx={tx}`, sleep `step_ms` in 250 ms steps.
    5. `w.pass(DopplerTarget::default())` (the leave), then `w.finish()`. Print the events: `text: …`, `status: NAME`, `info: rx=… tx=… mode=… split=… rit=…`; `Ident` events are not printed.
    6. A fresh rig reads the state again. Print `restored: yes`, or `restored: no` with both states; exit 0 or 1.
  - `explorer.exe` (`MainDlg.cs:685`) has no counterpart; `catcheck` writes no log.

- [ ] **Step 1: Write the failing tests:**
  - `settings_5a.rs` → `parse_error_has_no_source_text` (Review Focus 4): a file `my_call = "TE1ST"\nsecret = "SECRET-TEST-2` (unterminated). `load_or_default` is `Err`. Its text contains the path and `line 2`, and does not contain `SECRET-TEST-2`.
  - `catcheck/tests/cli.rs` (`fn cc(dir: &Path) -> Command` adds `--settings <dir>/settings.toml`; `FakeRigctld::start(RigState::default())`):
    - `status_online_from_fake`: `--engine rigctld --host 127.0.0.1 --port P status --count 2 --interval-ms 100` exits 0. stdout is `rig: [rigctld] 127.0.0.1:P` and then twice `ONLINE rx=144300000 tx=144300000 mode=USB split=OFF rit=OFF`.
    - `status_reads_settings_file`: `settings.toml` with `[cat]`, `engine = "rigctld"`, `rigctld_host = "127.0.0.1"`, `rigctld_port = P` gives the same output without connection flags.
    - `status_without_rigctld_reports_nocat` (Review Focus 2): a closed port (bind-and-release TCP) with `--timeout-ms 200 status --count 1` exits 2 within 10 s. The status line starts with `NOCAT rigctld 127.0.0.1:`.
    - `silent_rigctld_times_out` (Review Focus 2): `Behaviour::Silent`, `--timeout-ms 200 status --count 1` exits 2 within 5 s, and the status line is `OFFLINE`.
    - `set_freq_rx_and_tx`: `set-freq 144310000` gives `fake.state().freq == 144_310_000`. `set-freq 144320000 --tx` with split off sets `freq`. With `RigState { split: true, .. }` it sets `split_freq` (`I 144320000` in `fake.lines()`).
    - `out_of_range_freq_never_sent` (Review Focus 3):
      - `set-freq 0`, `set-freq -5` and `set-freq 100000000001` exit 1 with `frequency must be 1 to 100000000000 Hz`, and `fake.accepts() == 0`.
      - `doppler-test --base-hz 1000 --shift-hz 1000 --steps 2` exits 1 the same way, and `fake.accepts() == 0`.
      - `doppler-test --shift-hz 100000000000 --steps 1` (base from the rig) exits 1. No line of `fake.lines()` starts with `F `, `I `, `S `, `M `, `X ` or `U `.
    - `set_mode_split_rit`: `set-mode cw` gives mode `"CW"`; `split on` gives `split == true`; `rit on` gives `rit == true`. `set-mode other` exits 1 with `unknown mode: other`.
    - `doppler_test_restores_rig`: `doppler-test --shift-hz 1000 --steps 2 --step-ms 100` exits 0. The first line after `rig:` is `saved: rx=144300000 tx=144300000 mode=USB split=OFF rit=OFF`; the last line is `restored: yes`. `fake.lines()` contains `F 144301000`, `I 144299000`, `F 144302000` and `I 144298000`. Afterwards `fake.state()` has `freq == 144_300_000`, `split == false`, `mode == "USB"`.
    - `no_engine_is_error`: no flags and no file exit 1 with `no CAT engine selected`.
    - `#[cfg(not(feature = "hamlib"))] hamlib_not_compiled_is_error`: `--engine hamlib status` exits 1 with `not compiled in`.
    - `#[cfg(not(windows))] omnirig_off_windows_is_error`: `--engine omnirig status` exits 1 with `OmniRig is available on Windows only`.
    - `#[cfg(windows)] omnirig_without_server_reports_nocat`: `--engine omnirig --rig 1 status --count 1` exits 2, and the status line starts with `NOCAT` (finding #247).
    - `broken_settings_never_echo_secret` (Review Focus 4): `settings.toml` is `[cat]\nengine = "rigctld"\n[feed_settings.OpenSky]\nclient_secret = "SECRET-TEST-1` (unterminated). `status --count 1` exits 1; stderr contains `settings.toml` and `line 4`, and stdout and stderr do not contain `SECRET-TEST-1`.
    - `list_ports_does_not_panic`: `list-ports` exits with code 0 or 1, never 101.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings --test settings_5a` and `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p catcheck`. Expected: FAIL.
- [ ] **Step 3: Implement** the error change, the move of `cat_config` and the `catcheck` crate. Run one `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build -p catcheck -p airscout` without `--locked`, then the `Cargo.lock` check.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-settings -p as-cat -p airscout -p catcheck`, the hamlib feature check, then the Windows check. Expected: PASS, including `airscout` `app::corrupt_settings_are_not_overwritten` and `cat_service`. Report `wc -l crates/catcheck/src/main.rs` (< 500).
- [ ] **Step 5: Commit** `feat(catcheck): CAT test bench over as-cat; settings parse errors hide the file text`.

### Task 5: Docs, spec, findings, gates

**Files:**
- Modify: `README.md`, `docs/superpowers/specs/2026-11-23-airscout-rs-design.md:85-88`, `docs/logic-findings.md`

**Steps:**
- [ ] **Step 1: README:**
  - Status row: `5a | Developer tools: airscout-cli wintest-sim and horizon-gen, catcheck | done; catcheck not tested with a real rig`.
  - Build: one line listing the binaries `airscout`, `airscout-cli` and `catcheck`, and `cargo build -p catcheck --release --features hamlib` for the hamlib engine.
  - Command line: "`airscout-cli` is a smoke tool for the core and holds the developer tools." New section "Developer tools (stage 5a)" with one example each:
    - `airscout-cli wintest-sim --msg ASSETPATH --data … --listen 5`;
    - `airscout-cli horizon-gen --data ./data --stations all --out ./horizons`;
    - `catcheck --engine rigctld --host localhost --port 4532 status`, `catcheck set-freq 144300000`, `catcheck doppler-test --shift-hz 500`.
    - Say: `horizon-gen` is offline and writes one CSV per station (12 band columns, `;`, 8 decimals, `.`). `catcheck` reads the CAT settings of the app and flags override them. `doppler-test` prints the saved state first; after a Ctrl-C, restore the rig with `catcheck split off` and `catcheck set-freq`.
  - "Network sockets": a second table "Developer tools":
    - `wintest-sim send | <--addr>:<--port> (default 255.255.255.255:9872), UDP, from an ephemeral port on --bind | outbound broadcast | each run`;
    - `wintest-sim --listen | <--bind>:<--port> (default 0.0.0.0:9872), UDP, address reuse | inbound | only for the --listen seconds`;
    - `catcheck rigctld | <host>:<port> (default localhost:4532), TCP | outbound | engine rigctld`;
    - `catcheck hamlib / OmniRig | local device / local COM server | open, connect | engine hamlib or OmniRig`;
    - `horizon-gen | none | - | never`.
  - Privacy: `wintest-sim` puts the given From, To and Data on the LAN. `--listen` prints every datagram that reaches the port, from any host. `catcheck` shows no setting except the rig ident, and a broken settings file is reported by line and column only.
  - Findings range: "Stage 5a's are #278 to #N".
- [ ] **Step 2: Spec** (owner decision Q6, 2026-11-24): replace the rows `horizon-gen, maptile-gen, elevation-tile-gen, elevation-coverage (bins)`, `wintest-sim (bin)` and `catcheck (bin)` with:
  - `| maptile-gen, elevation-tile-gen (bins) | MapTileGenerator, ElevationTileGenerator | CLI (clap) |`
  - `| airscout-cli subcommands horizon-gen, elevation-coverage, wintest-sim (as-cli) | HorizonGenerator, ElevationCoverageMapper, WinTestSimulator | CLI (clap); owner decision Q6, 2026-11-24 |`
  - `| catcheck (bin) | CATCheck | CLI over as-cat (rigctld, hamlib, OmniRig) |`
- [ ] **Step 3:** Add findings from **#278** in this order. Each cites C# file:line and says Ported 1:1 / Not reproduced / Port deviation / New design.
  1. S10 columns: `HorizonGenerator/MainDlg.cs:64-73`, `:80-93`. The band loop runs over every `BAND` value, including `BNONE` (0) and `BALL` (999999999): 14 values under a 12-band header, so every column is shifted by one. The header also has the typo `eps[144M[deg]`. **Port deviation** (owner Q5): 12 columns in band order, typo fixed. The `BNONE` pass has an F1 radius of `300 / 0`, so every angle is NaN and no point is taken; it does not cut the path (test `bnone_pass_takes_no_point`). The 12 values therefore equal C# columns 2 to 13.
  2. S10 files and errors: `HorizonGenerator/MainDlg.cs:78`, `:95-98`. `Horizon.csv` in the working directory is overwritten for each station, so only the last station survives. Exceptions are swallowed. **Port deviation**: one file per station row, `<CALL>_<LOC>.csv` in `--out`, with safe and unique names. A failed station is reported on stderr, and the exit code is 1.
  3. `HorizonGenerator/MainDlg.cs:61` passes the form `this` where `ElevationPathCreateFromBearing` expects a `BackgroundWorker` (`ScoutBase/ScoutBase.Elevation/ElevationDatabase.cs:1365`). The project does not build against the current tree. **Not reproduced**.
  4. Hard-coded parameters (`HorizonGenerator/MainDlg.cs:45-52`, `:66-71`): SRTM1, antenna 40 m, cut height 4797 m, 500 km, radius `R * 1.33` for every band. F1 clearance 0.2 up to 144 MHz, 0.4 up to 432 MHz, 0.6 above, not the band settings. `band = BAND.B1_2G` and `count` are set and never used. **New design**: flags with these defaults; F1 from the default band table, or the C# thresholds with `--legacy-f1`.
  5. One shared, cut path for all bands: `ScoutBase/ScoutBase.Propagation/PropagationHorizon.cs:173-180` cuts `elv` with `Array.Resize(ref elv, i)`, and `HorizonGenerator/MainDlg.cs:62`, `:71` pass the same `elv` to every band. A cut made for one band also shortens the path of every later band. The cut test uses `maxelv` without the Fresnel term. **Ported 1:1**.
  6. `HorizonGenerator/MainDlg.cs:61` stores every computed path: `ElevationPathCreateFromBearing` uses `savetodatabase = true` (`ElevationDatabase.cs:1368`, `:1397`, `:1501`), so a run writes 360 paths per station into the elevation database. **Port deviation**: `horizon-gen` only reads (`save = false`).
  7. `HorizonGenerator/MainDlg.cs:78`, `:86`: `ToString("F8")` uses the current culture (a comma on a German Windows), and `StreamWriter` writes `\r\n`. **Port deviation**: invariant `.` and `\n` on every OS.
  8. S19, corrected: `WinTestSimulator/MainDlg.cs:24-25`, `:37-38`; `WinTest/WinTest.cs:113-114`, `:128-129`. An unknown name fails `Enum.TryParse` and leaves `NONE`. `ToBytes` returns `null` for `NONE` and `UNKNOWN`, `UdpClient.Send(null, …)` throws, and the click handler has no `catch`: the user sees the unhandled-exception dialog and nothing is sent. A decimal name (`7`) parses to an undefined value and is sent with the number as its name. **Port deviation**: an unknown name, `NONE`, `UNKNOWN` or an undefined number is an error and nothing is sent. Defined decimal values are accepted (`WtKind::parse`).
  9. S19 broadcast: `WinTestSimulator/MainDlg.cs:32-36`. The socket connects to `255.255.255.255` without `EnableBroadcast` (L5 of the stage 4 survey). It sets `ReuseAddress` and a 10 s receive timeout on a socket that never receives. **Port deviation**: `SO_BROADCAST` is set, and address reuse only on the `--listen` socket. `--listen` is **New design**.
  10. CATCheck tests the ScoutBase.CAT INI engine, which stage 4 dropped (D7). "Show log" starts `explorer.exe` (`CATCheck/MainDlg.cs:676-686`). The frequency dialog casts to `int` (`:697`), so it cannot set more than 2 147 483 647 Hz. **New design** (owner Q10): `catcheck`, a CLI over `as-cat` (rigctld, hamlib, OmniRig) with `status`, `set-freq`, `set-mode`, `split`, `rit`, `doppler-test` and `list-ports`. No log, no `explorer.exe`. Frequencies are 64-bit, from 1 Hz to `MAX_FREQ_HZ`.
  11. `AirScoutViewClient/MapViewDlg.cs:672-674` reads `Locator_AutoLength`, `Locator_SmallLettersForSubsquares` and `Locator_MaxLength`. The curated 4d `/settings.json` (#255) had none of them, so the C# ViewClient lost its locator display settings (S7). **Port deviation** (owner Q4): the three members follow `Planes_Filter_Max_Circumcircle` in `Settings.Designer.cs` order (`:1496`, `:1508`, `:1520`). `Locator_MaxLength` is `10.0` and `Locator_AutoLength` is `true`, the fixed values of the port (no Options field). `Locator_SmallLettersForSubsquares` comes from the setting.
- [ ] **Step 4:** Run the full gate on macOS, the hamlib feature check, the Linux docker gate and the Windows check (Global Constraints). Expected: green. Report the test counts. Push and check the hosted Windows CI job.
- [ ] **Step 5: Commit** `docs: stage 5a README, tool sockets, spec binary list and logic findings`.

## Self-review against the spec

- **Spec stage 5, 5a share:** `wintest-sim` (Task 2), `horizon-gen` (Task 3), `catcheck` (Task 4). The binary list follows Q6 (Task 5 Step 2). 5b-5d stay out.
- **Owner decisions:** Q4 (Task 1), Q5 (Task 3 columns, findings 1-2), Q6 (Tasks 2-3 as subcommands, Task 4 as a binary, the spec rows), Q10 (Task 4).
- **Survey 4, 7, 8:** every recommended flag is covered: `--model`, `--height`, `--dist`, `--k`, `--legacy-f1`, `--stations`, `--out`; `--msg/--from/--to/--data/--port/--addr/--listen`; `status`, `set-freq`, `set-mode`, `split`, `rit`, `doppler-test`, `--list-ports` (as the subcommand `list-ports`). The rigctld path runs against the 4b fake.
- **Survey 10:** every socket is in the README table (Task 5); no tool listens unless started for it; credentials are never shown (Task 4).
- **Survey 11:** S10 (findings 1, 2) and S19 (findings 8, 9). The other S-ids concern 5b-5d or later tools.
- **Project rules:** no secrets, honest UA (no HTTP in 5a), documented sockets, synthetic test data, no private LAN address, three OS, disk rules, signed commits, linear history (Global Constraints).
- **Type consistency:** `bind_shared` (Task 2) is used by `bind_socket` and `wintest_sim::run`. `cat_config(&Settings) -> CatConfig` (Task 4) keeps the airscout path. `LOCATOR_MAX_LENGTH`/`LOCATOR_AUTO_LENGTH` (Task 1) are used by `stations.rs` and `web_settings_json.rs`. `fixed` (as-wintest) is used by `csv_text`. `crate::Res` and `crate::parse_model` come from `as-cli/src/main.rs`.
- **Proportion:** 5 tasks. The only code block is the shared-path loop, which the signatures do not determine.

## Open questions (owner) and controller rulings

Owner questions: none. The binding decisions cover the scope, and every open choice below is a controller ruling.

Controller rulings (binding for the implementers; each with its cost if wrong):

- Ruling: `cat_config` moves into `as-cat` behind the feature `settings` — `catcheck` must build the same `CatConfig` as the app, and pulling in the `airscout` crate would bring eframe and the audio stack; a non-optional `as-settings` dependency would make every `as-cat` build compile the database crates — cost if wrong: one feature flag more in `as-cat`.
- Ruling: the shared UDP socket moves into `as_wintest::socket` — `wintest-sim --listen` must bind like the app's receiver, or the two would not share port 9872 — cost if wrong: `as-wintest` gains `socket2`, already in the tree.
- Ruling: `SettingsError::Parse` shows only path, line and column — the toml error text quotes the broken line, and its message can quote a value; the app shows this text in the control panel today, so the change also closes a leak in the app — cost if wrong: the user sees no toml explanation and must look at the line.
- Ruling: `horizon-gen` takes F1 from the default band table unless `--legacy-f1` — the survey's recommendation, since the C# thresholds ignore the band settings — cost if wrong: one flag to get the C# numbers.
- Ruling: `horizon-gen` keeps the shared, cut path across bands (finding 5) — prediction behaviour is ported 1:1 — cost if wrong: higher bands may get a horizon that is too low, as upstream; stage 6 decides.
- Ruling: `horizon-gen` does not store paths (finding 6) — a developer tool should not grow the user's database by 360 rows per station — cost if wrong: a later app run recomputes those paths.
- Ruling: `horizon-gen` uses no settings file — every value is a flag, and no settings file means nothing secret to handle — cost if wrong: a user with custom band settings passes `--legacy-f1` or waits for a later flag.
- Ruling: `Locator_MaxLength` and `Locator_AutoLength` are served as fixed `10.0` and `true` — the port has no setting for either, and these are the C# defaults it uses everywhere (`ui/options/stations.rs`) — cost if wrong: a ViewClient user with other values on the C# side gets the defaults.
- Ruling: `catcheck status` polls until Ctrl-C by default — the C# CATCheck polls while it is open — cost if wrong: scripts must pass `--count`.
- Ruling: `doppler-test` drives the real `CatWorker` and checks the restore — this is what the app does with a rig, and the check makes the tool useful — cost if wrong: a Ctrl-C during the test leaves the rig in split; the printed `saved:` line and the README say how to restore it.
- Ruling: `wintest-sim` skips its own echo by source port and bytes — the listening socket also receives the datagram it sent — cost if wrong: an identical datagram from another program on the same port and host is hidden.

Survey corrections found while planning (the plan follows these):

- §7 and S19 say an unknown name "silently sends `NONE`". It sends nothing: `ToBytes` returns `null` for `NONE` (`WinTest/WinTest.cs:113-114`), and `UdpClient.Send(null, …)` throws into a handler without `catch` (`WinTestSimulator/MainDlg.cs:37-38`). The name is also upper-cased and trimmed first (`:25`), and a decimal value is accepted.
- §4 does not say that `HorizonGenerator` fails to build (`MainDlg.cs:61` against `ElevationDatabase.cs:1365`) or that it stores every path (`ElevationDatabase.cs:1501`). It also misses the header typo `eps[144M[deg]` (`:80`), the current-culture `F8` (`:86`), and the shared cut path across bands (`PropagationHorizon.cs:178`).
- §8 lists `rit ..`. `as_cat::Rig` has only RIT on and off, no offset (`crates/as-cat/src/rig.rs:20`), so `catcheck` offers `rit on|off`.
- §3.4 places the `Locator_*` members in 5e; the owner moved them to 5a (Q4).
- §12 says 5a needs "no new crates". That holds for external crates. The plan adds the workspace crate `catcheck` and an optional workspace dependency of `as-cat`.
