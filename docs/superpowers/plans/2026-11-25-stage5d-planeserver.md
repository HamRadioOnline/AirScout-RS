# Stage 5d — `airscout-planeserver` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stage 5d of the spec, as split in the survey: `airscout-planeserver`, a plane feed collector that runs up to three feeds over an area and serves the planes as `/planes.json` in the format the AirScoutServer feed reads. It has one binary with two modes: `--headless` (daemon) or an egui window with a tray icon (owner decisions Q1, Q2).

**Architecture:**
- **`as-web`** gets two shared pieces.
  - The `planes.json` writer moves here from `crates/airscout/src/web_service.rs`, so a non-GUI binary can use it.
  - A router hook in `WebConfig`, so a server can run with its own route table. The bounded pool, the timeouts and the connection cap stay as they are (4d, #272).
- **`airscout-planeserver`** is a new crate at `crates/airscout-planeserver`, built as a lib plus a bin.
  - `config` holds `PlaneServerConfig`, read from its own TOML file.
  - `service` holds `Service`. It runs the feeds into one `PlaneInfoCache` and the web server with a route table of `/planes.json` and `/` only.
  - `gui` holds an eframe window and the tray.
  - `main.rs` holds clap and the headless loop.
- **No upstream code to run.** The C# PlaneServer is dead and does not build (survey §2.1, S1). So the port follows the intent and the formats (risk R1). The defaults come from `AirScoutPlaneServer/Properties/Settings.settings`. Each deviation is logged.

**Tech Stack:**
- Rust 1.95+.
- Existing crates: as-feeds (`FeedWorker`, `registry`), as-aircraft (`PlaneInfoCache`, `AircraftDb`), as-web (`WebServer`), as-settings (`FeedSettings`, `Settings::default_path`), as-net.
- Libraries: eframe/egui 0.37, egui_kittest, clap 4, toml, serde, directories, chrono, assert_cmd, tempfile, reqwest blocking (tests only, already in the workspace).
- **New:**
  - `tray-icon = "0.26"`, owner decision 2026-11-25: tray on every OS. Linux CI gets `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`.
  - Possibly `ctrlc = "3"` (Task 4 decides).

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`:
- binary row `airscout-planeserver` (bin), AirScoutPlaneServer, "headless" (`:83`). Task 6 changes this row to "headless daemon or egui window with tray (owner Q1)".

**Inputs:**
- Survey `docs/superpowers/surveys/2026-11-24-suite-tools-survey.md`:
  - §2 (whole section), §10 (sockets, credentials, platform);
  - §11 S1–S5, §12 (5d), risk R1 and R5;
  - "Decisions (project owner, 2026-11-24)" Q1 and Q2.
- Ledger `docs/superpowers/ledgers/2026-11-25-stage5c-maps.md`: deferred minors and the patterns for untrusted input and gates.
- Upstream `target/upstream/AirScout/` @ `91b02cb`, read-only:
  - `AirScoutPlaneServer/Properties/Settings.settings`: `Webserver_Active` True `:14`, `Webserver_Port` 9872 `:17`, `Webserver_Welcome` `:20`, the area `:44-53`, `Planes_MinAlt` 5000 and `Planes_MaxAlt` 20000 (metres, `MainDlg.Designer.cs:464` "Max. Altitude [m]:"), `Planes_Lifetime` 5 `:71`, `Planes_WebFeed1..3` `[none]`, `Windows_Startup_Systray`.
  - `AirScoutPlaneServer/Webserver.cs:16-103`, `JSONWriter.cs:20-94`, `MainDlg.cs` (feeds `:255-295`, tray `:433-466`, status timer `:652`).
  - `MainDlg.Designer.cs:304-1062`: the tabs.
  - `AirScout.PlaneFeeds.Plugin.AirScoutServer/AirScoutServer.cs:349-387`: the FR24 branch needs `"full_count":` and `"version":1`.
  - `AirScout/MapDlg.cs:8463-8545` (the writer) and `AirScout/Properties/AssemblyInfo.cs:35` (major 1).
- Rust code to reuse:
  - `crates/as-cli/src/main.rs:433-505` (`planes_cmd`, a headless feed loop);
  - `crates/airscout/src/feed_service.rs:25-93`;
  - `crates/as-feeds/src/worker.rs:150` (`FeedWorker::spawn`);
  - `crates/as-feeds/tests/worker.rs` (`FakeFeed`);
  - `crates/as-web/src/server.rs:76-191` and `route.rs:36-75`.

**Stage split:**
- **In 5d (this plan):** the as-web changes, the planeserver crate, the CI packages, the README and the findings.
- **Not in 5d:**
  - the coverage map tab;
  - the 60 s database updater (dead URLs, S2);
  - the autorun registry key (README documents service setup instead);
  - the `netsh urlacl` call;
  - the status-strip CPU, memory and DB size;
  - IPv6 (as #271).
  - ViewClient and RainScout come after stage 6.

## Global Constraints

Everything in the 5c Global Constraints still applies where it concerns the touched crates. Changes and additions:

- **Port policy:**
  - Upstream never ran, so "1:1" means the same intent, the same defaults and the same wire format (R1).
  - Every deviation from the C# source is logged in `docs/logic-findings.md`. The next number is **#325**; check the last row first. Task 6 assigns the numbers in its list order.
  - Every C# `file:line` in code comments and findings is checked against `target/upstream`.
- **Owner decisions (binding):**
  - Q1: one binary `airscout-planeserver`. `--headless` runs the daemon; without it, an egui window with a tray icon. No autorun registry key.
  - Q2: bind `0.0.0.0:9872`, server on by default (`server_active = true`). The README warns that the port is open to the whole LAN without authentication.
  - Tray (2026-11-25): `tray-icon` on all three OS, not behind a feature. CI installs the Linux packages.
- **Wire format:**
  - `/planes.json` is the C# FR24 layout (`MapDlg.cs:8481-8528`, already in `planes_json`).
  - The `version` member is the literal **`1`** in both writers (app and planeserver): `pub const PLANES_JSON_VERSION: u64 = 1`. Reason: the AirScoutServer feed (C# `AirScoutServer.cs:353`, Rust `airscoutserver.rs:123`) reads the FR24 branch only with `"version":1`. The AirScout-RS major version is 0, so `"version":0` from 4d is unreadable. This amends #269.
- **Server rules (R5, 4d):**
  - `as-web` pool and caps are unchanged: `MAX_WORKERS` 4, `HEADER_TIMEOUT` 10 s, `MAX_CONNECTIONS` 64, `RETRY` 10 s.
  - Routes, compared on the lower-cased path as in `route.rs`:
    - `/planes.json`: the JSON answer.
    - `/`: the welcome HTML of `Settings.settings:20`.
    - Everything else: 404 (deviation S4, which answers 200 for everything).
  - No file is read for a route. `/planes.json` is built from memory per request, and no `Tmp/planes.json` file is written.
  - No credential, path or config value is ever served.
- **Credentials:**
  - Feed credentials live only in `feed_settings` of the planeserver config file.
  - They never appear in logs, stderr, status texts, error texts, the window's status tab, `Debug` output or the User-Agent.
  - Secret keys (`PlaneFeed::secret_keys`) are password fields in the GUI.
- **Restricted feeds:**
  - These are the feeds with `enabled_by_default() == false`: BinCraft, PublicBinCraft and ADSBSupport.
  - Such a feed starts only if its name is in `feeds_disclaimer_accepted`. Otherwise the feed is not started and its status says `disclaimer not accepted (<name>)`.
  - The GUI shows the disclaimer text and adds the name on accept, as the app does (`airscout/src/ui/dialogs/disclaimer.rs:47-73`).
  - Headless mode never accepts by itself.
- **Network:**
  - Outbound requests go only to the configured feeds, with the honest User-Agent the feeds already send.
  - `my_lat`/`my_lon` in `FeedArgs` are the area centre, never a station.
  - No map tiles and no airscout.eu database polling.
- **Platform:**
  - macOS, Linux and Windows.
  - `cfg` code only where `tray-icon` needs it (the Linux gtk thread).
  - `#![forbid(unsafe_code)]` in the planeserver crate.
- **Cargo:**
  - Every cargo command carries `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
  - Disk: total under 120 GB; `target/` under 60 GB, clean at 40 GB with `cargo clean --profile dev` (same env); never plain `cargo clean`.
  - Tasks 1, 2 and 5 (and 4 if it adds `ctrlc`) change dependency edges. Each runs one `cargo build` without `--locked` and commits `Cargo.lock`.
  - New workspace entries: `airscout-planeserver` is a bin crate and gets no workspace dep entry; `tray-icon = "0.26"` (Task 5).
- **Gates (every task that changes Rust code):**
  - Every task: `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked`.
  - Task 6 also runs:
    - the hamlib clippy pair;
    - the Linux docker gate (rust:1.95, `target/linux`, apt line extended with `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`, prints `LINUX_OK`);
    - the Windows mingw gate (`target/wingnu`, prints `WINGNU_OK`).
  - Afterwards: `rm -rf target/linux target/wingnu` and `docker system prune -f` (keep the rust and dotnet images).
- **Tests:**
  - Servers bind `127.0.0.1:0` and read the port back. Never pick a free port first and bind later (the 5b flake).
  - No test listens on `0.0.0.0`.
  - No test makes a request to the internet.
- **Privacy:** tests, fixtures and docs use synthetic values only (TE1ST, JO50AA, hex `3C0001`). Examples use `localhost`.
- **Commits:** signed (`export SSH_AUTH_SOCK=~/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock`, `git commit -S`). No push.

## Review Focus

1. **A hostile or broken client on the LAN port.** Cases: a slow header, 100 open connections, a 1 MB header, `/../planes.json`, `/PLANES.JSON`, `/planes.json?x=1`, `POST`. Expected: the server stays up and answers within the caps. Only the two routes answer 200, and nothing from disk is served. Tests in Task 1 (`planeserver_router_only_two_routes`) and Task 3 (`serves_only_planes_and_welcome`, `survives_many_connections`).
2. **A feed that fails, hangs or returns garbage while others run.** Expected: the other feeds and the server keep running; the status shows the error text without credentials; stop still ends within 2 s. Test in Task 3 (`failing_feed_does_not_stop_others`, `stop_is_prompt`).
3. **A hand-edited config with a restricted feed and credentials but no disclaimer acceptance.** Expected: that feed is not started, the status says so, and no credential appears in status, stderr or `Debug`. Tests in Task 2 (`debug_hides_secrets`) and Task 3 (`restricted_feed_needs_disclaimer`).
4. **A broken or out-of-range config.** Cases: not TOML, port 0, lat 95, min above max, four feeds, an unknown feed name, a missing file. Expected:
   - A missing file gives the defaults.
   - Not TOML is an error naming the file, without echoing its contents.
   - Ranges are clamped or refused with a clear message.
   - An unknown feed name gives a status error for that slot.
   Tests in Task 2 (`config_validation`) and Task 4 (`headless_refuses_broken_config`).
5. **Port 9872 is already taken** (a second instance, or another program on TCP 9872). Expected: the error is shown (stderr or status) with the address, the server retries every `RETRY`, and the feeds keep collecting. Test in Task 3 (`port_in_use_reports_and_retries`).

---

### Task 1: `as-web` — shared `planes.json` writer, version 1, router hook

**Files:**
- Create: `crates/as-web/src/planes.rs`
- Modify: `crates/as-web/src/lib.rs`, `crates/as-web/src/server.rs` (`WebConfig`, `handle`), `crates/as-web/Cargo.toml` (as-aircraft, as-wintest; dev: as-feeds), `crates/airscout/src/web_service.rs` (use the moved fn, drop `major()`)
- Test: `crates/as-web/tests/planes.rs`, `crates/as-web/tests/router.rs`, the existing `web_service` tests in `crates/airscout`

**Interfaces:**
- Produces:
  - `as_web::planes::{planes_json(planes: &[PlaneInfo], version: u64) -> String, PLANES_JSON_VERSION: u64 = 1}`, with the body moved unchanged from `web_service.rs:40-66`, `quoted` included.
  - `pub type Router = fn(&str) -> Route;`.
  - `WebConfig.router: Router`. `WebConfig::new(bind)` sets it to `route::route`, so the app is unchanged.
  - `pub fn WebConfig::with_router(self, r: Router) -> Self`.
  - `Route`, `ApiRoute`, `Asset` stay public as now.
- Consumes: nothing new.

- [ ] **Step 1: Write the failing tests**
  - `planes.rs`:
    - `version_is_one`: `planes_json(&[], PLANES_JSON_VERSION)` starts with `{"full_count":0,"version":1`.
    - `airscoutserver_reads_it`: two `PlaneInfo` (hex `3C0001`, call `TE1ST`, lat 50.1, lon 10.2, alt, speed, track, type `A320`, time now) are written. The Rust AirScoutServer feed's parse fn, run on that text, gives two planes with the same hex, call, type and position (4 decimals). Use the fn the existing `crates/as-feeds/tests/json_feeds.rs:285-347` tests call. If it is private, add a `pub fn parse_planes_json(text: &str) -> Result<Vec<FeedPlane>, FeedError>` there and use it in `get_planes`.
    - `layout_unchanged`: the existing `web_service` golden or unit expectations pass with the moved fn.
  - `router.rs`:
    - `default_router_is_route`: `WebConfig::new(addr).router == route::route as Router` (compare by calling both on 5 paths).
    - `custom_router_used`: a server started on `127.0.0.1:0` with a router that maps every path to `Route::NotFound` answers 404 for `/planes.json`. With the default router, the same server answers 200 for `/planes.json` (use a tiny `Api` returning `"{}"`).
- [ ] **Step 2: Run them to see them fail**
  - Run: `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-web --test planes --test router`
  - Expected: FAIL (module `planes` and `with_router` missing).
- [ ] **Step 3: Implement**
  - Move the writer. `crates/airscout/src/web_service.rs` calls `as_web::planes::planes_json(&planes, PLANES_JSON_VERSION)` and deletes `major()`.
  - In `server.rs` `handle`, call `(state.router)(&raw)` instead of `route(&raw)`. The router travels in `AppState`.
  - Update the comment at the old `major()` call site to cite `AirScoutServer.cs:353` and `AssemblyInfo.cs:35` for the literal 1.
- [ ] **Step 4: Run the gates**
  - Run: `cargo build` (once, without `--locked`), then fmt, clippy and the workspace tests.
  - Expected: green.
  - `git diff Cargo.lock | grep '^+'` shows only the new dependency lines of `as-web`. No new package appears.
- [ ] **Step 5: Commit** `fix(as-web): shared planes.json writer with version 1, router hook`

### Task 2: `airscout-planeserver` crate and config

**Files:**
- Create: `crates/airscout-planeserver/Cargo.toml` (`[lib] name = "airscout_planeserver"`, `[[bin]] name = "airscout-planeserver" path = "src/main.rs"`), `src/lib.rs` (`#![forbid(unsafe_code)]`, `pub mod config;`), `src/config.rs`, `src/main.rs` (a stub that prints `--help` via clap for now)
- Test: `crates/airscout-planeserver/tests/config.rs`

**Interfaces:**
- Produces, in `airscout_planeserver::config`:
  - `pub const DEFAULT_PORT: u16 = 9872;`
  - `pub const MAX_FEEDS: usize = 3;`
  - `pub const WELCOME: &str = "<HTML><BODY> Welcome to AirScout PlaneServer</BODY></HTML>";` (`Settings.settings:20`)
  - `#[derive(Clone, Serialize, Deserialize, PartialEq)] #[serde(default)] pub struct PlaneServerConfig`, with these fields and defaults:
    - `server_active: bool` (true);
    - `bind: IpAddr` (`0.0.0.0`);
    - `port: u16` (9872);
    - `min_lat: f64` 35, `max_lat` 60, `min_lon` -15, `max_lon` 30;
    - `min_alt_m: i32` 5000, `max_alt_m` 20000;
    - `lifetime_min: i64` 5;
    - `interval_s: u64` 60 (the app's `planes_interval_s`; the C# server has none);
    - `feeds: Vec<String>` (empty; C# `[none]`);
    - `feed_settings: as_settings::FeedSettings`;
    - `feeds_disclaimer_accepted: Vec<String>`;
    - `extended_plausibility: bool` true, `max_error_dist: f64` 30;
    - `data_dir: Option<PathBuf>`;
    - `start_in_tray: bool` false (`Windows_Startup_Systray`).
  - A hand-written `impl Debug` that prints `feed_settings` only as its key names, as `FeedSettings` already does.
  - `pub fn default_path() -> PathBuf`: `planeserver.toml` next to `as_settings::Settings::default_path()`.
  - `pub fn load_or_default(path: &Path) -> Result<PlaneServerConfig, ConfigError>`, `pub fn save(&self, path: &Path) -> io::Result<()>` (temp file and rename), `pub fn validate(&mut self) -> Vec<String>`.
  - `validate` runs on the file values only; the CLI overrides of Task 4 (`--port 0` for tests) are applied after it.
  - `validate` clamps lat to ±90 and lon to ±180, swaps min and max, refuses port 0 (sets 9872), keeps the first `MAX_FEEDS` feeds, and clamps `interval_s` to at least 1 and `lifetime_min` to `1..=1440`. It returns one warning line per change.
  - `pub fn data_dir(&self) -> PathBuf`: `data_dir`, or the same `ProjectDirs` data dir the app uses (`crates/airscout/src/app/mod.rs:107`; copy the three lines, do not depend on `airscout`).
  - `ConfigError`: `Display` names the path and never the file text.

- [ ] **Step 1: Write the failing tests**
  - `defaults_match_csharp`: every default above, field by field.
  - `missing_file_gives_defaults`.
  - `broken_file_names_path_not_content`: the file `port = "x" secret = "hunter2"` gives an error that contains the path and not `hunter2`.
  - `config_validation`: port 0, lat 95, min_lat > max_lat, four feeds, interval 0. Check the clamped values and 5 warnings.
  - `save_load_roundtrip` (tempdir).
  - `debug_hides_secrets`: `feed_settings.ADSBSupport.password = "hunter2"`; `format!("{cfg:?}")` does not contain `hunter2`.
- [ ] **Step 2: Run them to see them fail.** `cargo test -p airscout-planeserver --test config`. Expected: FAIL (crate empty).
- [ ] **Step 3: Implement** `config.rs` as listed. `as-settings` already exports `FeedSettings`; reuse its serde.
- [ ] **Step 4: Run the gates.** Run one `cargo build` without `--locked`. Expected: green. `git diff Cargo.lock | grep '^+name'` prints only `+name = "airscout-planeserver"`.
- [ ] **Step 5: Commit** `feat(planeserver): crate and config file with C# defaults`

### Task 3: `Service` (feeds, cache, server, status)

**Files:**
- Create: `crates/airscout-planeserver/src/service.rs`
- Test: `crates/airscout-planeserver/tests/service.rs` (with a `FakeFeed` copied from `crates/as-feeds/tests/worker.rs`)

**Interfaces:**
- Consumes:
  - `PlaneServerConfig` (Task 2);
  - `as_web::{WebServer, WebConfig, Router, Route, ApiRoute, Api, Asset}`, `as_web::planes::{planes_json, PLANES_JSON_VERSION}` (Task 1);
  - `as_feeds::{worker::{FeedWorker, WorkerConfig, FeedEvent, FeedHandle}, registry::feed_by_name, plugin::{FeedArgs, PlaneFeed}}`;
  - `as_aircraft::{PlaneInfoCache, db::AircraftDb}`.
- Produces:
  - `pub fn router(raw: &str) -> Route`. It maps `/planes.json` to `Route::Api(ApiRoute::Planes)` and `/` to `Route::Asset(&WELCOME_ASSET)` (a `static Asset { path: "", mime: "text/html", bytes: WELCOME.as_bytes() }`). Everything else is `Route::NotFound`. The comparison is on the lower-cased path; the query and everything after `?` is cut off first. This deviates from `route.rs:75`: the AirScoutServer feed replaces `%MINLAT%`-style variables, and a user URL may carry a query.
  - `pub struct Service`, with:
    - `pub fn start(cfg: &PlaneServerConfig, feeds: Vec<Result<Box<dyn PlaneFeed>, String>>) -> Result<Service, String>`. `feeds` is built by `pub fn make_feeds(cfg) -> Vec<Result<Box<dyn PlaneFeed>, String>>`: one entry per configured slot; an `Err` is the slot's status text (unknown name, disclaimer not accepted, `load_settings` error) and starts no worker. Tests inject a `FakeFeed` as `Ok`.
    - `pub fn status(&self) -> Status`.
    - `pub fn local_addr(&self) -> Option<SocketAddr>`.
    - `pub fn stop(self)`.
  - `pub struct Status`, with:
    - `started: DateTime<Utc>`;
    - `planes: usize`: the count from `get_all(now, lifetime_min)`;
    - `feeds: Vec<FeedStatus { name: String, state: String, last_ok: Option<DateTime<Utc>>, last_error: Option<String> }>`;
    - `server: String`: `listening on <addr>`, `off` or the last server error.
- **Behaviour:**
  - One shared `Arc<PlaneInfoCache>`, and one `AircraftDb::open(cfg.data_dir())` (an error is a status line and the db falls back to `open_in_memory()`).
  - A `FeedWorker` per started feed, with `FeedArgs` area, altitude and centre from the config. The `*_dir` fields come under `data_dir()`.
  - One forwarder thread drains the shared `FeedEvent` channel into the status.
  - A restricted feed without acceptance is not started (status `disclaimer not accepted (<name>)`), and neither is an unknown name (status `unknown feed (<name>)`).
  - The server starts only when `server_active`. `Api::answer` for `Planes` returns `planes_json(&cache.get_all(Utc::now(), lifetime), PLANES_JSON_VERSION)`.
  - Server errors arrive via `on_error` into the status.
  - `stop` stops the workers and the server, then joins all threads within 2 s.

- [ ] **Step 1: Write the failing tests** (server on `127.0.0.1:0`; set `cfg.bind = 127.0.0.1`, `cfg.port = 0`)
  - `planeserver_router_only_two_routes`: `/planes.json`, `/PLANES.JSON` and `/planes.json?minlat=1` are Planes. `/` is the welcome asset. `/index.html`, `/../planes.json`, `/settings.json` and `/planes.json/x` are NotFound.
  - `serves_planes_from_fake_feed`: a FakeFeed with two planes in the area. After the first `Planes` event, a GET of `/planes.json` (reqwest blocking) parses through the AirScoutServer feed parse fn (Task 1) to the two planes.
  - `serves_only_planes_and_welcome`: `/` is 200 `text/html` containing `Welcome to AirScout PlaneServer`. `/settings.json` and `/index.html` are 404.
  - `survives_many_connections`: open 100 TCP connections and send nothing. A GET of `/planes.json` on a fresh connection answers within `HEADER_TIMEOUT + 2 s`.
  - `failing_feed_does_not_stop_others`: feed 1 is a FakeFeed that always errors, feed 2 a good one. Planes from feed 2 are served, and feed 1's status has `last_error`.
  - `restricted_feed_needs_disclaimer`: config `feeds = ["ADSBSupport"]` with a `password` in `feed_settings` and no acceptance. Its status is `disclaimer not accepted (ADSBSupport)`. `format!("{:?}", status)` and every status text do not contain the password. No request goes out: this needs no network.
  - `port_in_use_reports_and_retries`: bind a `std::net::TcpListener` on `127.0.0.1:0`, then start the service on that port. Within 2 s the status server text contains the address and an error. Drop the listener; within `RETRY + 2 s` the server is listening.
  - `stop_is_prompt`: with a FakeFeed and the server running, `stop` returns within 2 s.
- [ ] **Step 2: Run them to see them fail.** `cargo test -p airscout-planeserver --test service`. Expected: FAIL.
- [ ] **Step 3: Implement** `service.rs`.
- [ ] **Step 4: Run the gates.** Expected: green.
- [ ] **Step 5: Commit** `feat(planeserver): service with feeds, cache and /planes.json server`

### Task 4: CLI and headless mode

**Files:**
- Modify: `crates/airscout-planeserver/src/main.rs`
- Test: `crates/airscout-planeserver/tests/cli.rs` (assert_cmd)

**Interfaces:**
- Consumes: `config` (Task 2), `service` (Task 3).
- Produces: `airscout-planeserver [--headless] [--config <path>] [--bind <ip>] [--port <n>] [--data <dir>] [--run-for <s>]`.
  - `--bind`, `--port` and `--data` override the file for this run only. They are not saved.
  - `--run-for` (hidden, `1..=3600`) stops headless after that many seconds. It exists for the tests.
- **Headless behaviour:**
  - Load the config: a missing file gives the defaults, a broken one exits 2 with the error. Then `validate`, printing each warning to stderr.
  - Print one start line to stderr: `airscout-planeserver: serving /planes.json on http://<bind>:<port>/ (no authentication)`. When bind is not loopback, also print `warning: port open to the whole network`.
  - Print one line per feed state change, and the server status when it changes. Never print credentials, feed URLs with user info, or config values other than bind and port.
  - Stop on Ctrl-C / SIGTERM (`ctrlc` crate with feature `termination`, or `tokio::signal` if it can do it without new tokio features; the implementer picks and logs a ruling). Also stop after `--run-for`. Exit 0.
  - With no feeds configured, print `no feed configured; edit <path> or start without --headless` and keep serving (empty `planes.json`, as C#).
- Without `--headless`, run `gui::run(cfg, path)` (Task 5). Until Task 5 lands, print `GUI not built yet` and exit 1.

- [ ] **Step 1: Write the failing tests**
  - `headless_serves_and_stops`: `--headless --bind 127.0.0.1 --port 0 --run-for 2 --config <tmp>/ps.toml`, with no feeds. Exit 0, and stderr contains `serving /planes.json on http://127.0.0.1:`.
    - Port 0: the start line prints the real bound port. Add `Service::local_addr` use.
    - A second test reads that port from stderr while the process runs and GETs `/planes.json` (`{"full_count":0,"version":1`).
  - `headless_refuses_broken_config`: exit 2, stderr names the file and not its content.
  - `headless_warns_on_lan_bind`: with bind `0.0.0.0` it would listen on the LAN, so never run it. Instead, unit-test the line builder `start_lines(cfg) -> Vec<String>` in `main.rs` (move it to `lib.rs` as `pub fn start_lines`).
  - `overrides_not_saved`: after a `--port 0` run the config file, if it exists, is unchanged.
  - `help_lists_flags`.
- [ ] **Step 2: Run them to see them fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the gates** (plus a `cargo build` without `--locked` if `ctrlc` is added). Expected: green.
- [ ] **Step 5: Commit** `feat(planeserver): headless daemon`

### Task 5: egui window and tray icon

**Files:**
- Create: `crates/airscout-planeserver/src/gui.rs` (window), `src/tray.rs` (tray, `cfg` per OS inside)
- Modify: `Cargo.toml` (workspace entry `tray-icon = "0.26"`; crate deps `eframe`, `tray-icon`), `src/main.rs`, `.github/scripts/ci-prepare.sh` (Linux packages)
- Test: `crates/airscout-planeserver/tests/gui.rs` (egui_kittest)

**Interfaces:**
- Consumes: `config`, `service`, `as_feeds::registry::all_feeds` (names, `disclaimer()`, `enabled_by_default()`, `settings()`, `secret_keys()`).
- Produces:
  - `pub fn run(cfg: PlaneServerConfig, path: PathBuf) -> Result<(), String>`.
  - `pub struct PlaneServerApp` with `pub fn new(cfg, path, service_factory) -> Self` and `impl eframe::App`. The factory makes kittest possible without network.
- **Window:** title `AirScout Plane Server` (`MainDlg.Designer.cs`), size 634 × 452. Tabs:
  - **Status:** uptime, plane count, one row per feed (name, state, last ok, last error) and the server line. Refreshed every 1 s (`ti_Main_Tick`, `MainDlg.cs:652`).
  - **Feeds:** three combo boxes (`[none]` plus all feed names). Below each selected feed, its `settings()` keys as text fields, with `secret_keys` as password fields. Selecting a restricted feed opens its `disclaimer()` text with Accept / Cancel; Accept adds the name to `feeds_disclaimer_accepted`.
  - **Area:** min/max lat/lon, min/max altitude [m], lifetime [min], interval [s].
  - **Server:** "Activate Server" checkbox, bind address, port (1024..=65535 as in the C# spinner, plus the config's own range), and the LAN warning text as a label.
  - **General:** "Start in tray" checkbox.
  - Buttons Apply and Revert. Apply runs `validate`, shows the warnings, saves the file, then stops the service and starts a new one. Revert reloads the file.
- **Tray:**
  - Icon: the existing app icon if `crates/airscout` embeds one (reuse the bytes via `include_bytes!` on the same file), else a 32 × 32 generated PNG.
  - Menu: `Show window`, `Quit`, and a disabled line with the plane count, updated every 1 s.
  - `Show window` sends `ViewportCommand::Visible(true)` and `Focus`. The window close button hides the window when a tray icon exists, else it quits. `Quit` stops the service and closes.
  - `start_in_tray` starts hidden.
  - Linux: create the tray on a separate thread that runs `gtk::init()` and `gtk::main()` (the `tray-icon` README pattern). Events go through `TrayIconEvent::receiver()` and `MenuEvent::receiver()`, polled in `update`.
  - A failed tray creation (no tray host, no display) never ends the program. The window stays and closing quits; the Status tab shows `tray: <error>`.
- **CI:**
  - `ci-prepare.sh` adds, Linux native build only, the same `pkg-config --exists` pattern as ALSA: `gtk+-3.0` → `libgtk-3-dev`, `ayatana-appindicator3-0.1` → `libayatana-appindicator3-dev`, `xdo` → `libxdo-dev` (`libxdo` has no `.pc` on some distros; check with `ldconfig -p | grep -q libxdo.so` or `test -e /usr/include/xdo.h`).
  - The windows-gnu cross build needs nothing new.
  - The hosted macOS and Windows runners need nothing.

- [ ] **Step 1: Write the failing tests** (kittest, a service factory returning a fake status, no tray)
  - `tabs_render`: each of the five tabs, clicked, shows a label unique to it. The click goes through kittest. Assert that the tab switched: see the 5c deferred minor about `every_tab_renders`, and do not repeat that trap.
  - `restricted_feed_shows_disclaimer`: choose `ADSBSupport` in slot 1, and the disclaimer text appears. Cancel resets the slot to `[none]`. Accept keeps it and adds the name.
  - `secret_field_is_password`: the `password` field of ADSBSupport is a password `TextEdit`. Query by label; the AccessKit node has the password flag or its value is masked.
  - `apply_saves_and_restarts`: change the port to 9999 and press Apply. The file holds 9999 and the factory was called twice.
  - `status_shows_feed_error`: a fake status with `last_error = "timeout"` shows `timeout`.
- [ ] **Step 2: Run them to see them fail.**
- [ ] **Step 3: Implement** `gui.rs` and `tray.rs`.
- [ ] **Step 4: Run the gates.**
  - Run one `cargo build` without `--locked`.
  - Then the macOS gates and the Windows mingw gate (the tray crate has Windows code).
  - Then the Linux docker gate with the extended apt line, printing `LINUX_OK`.
  - Expected: green.
  - Record the new `Cargo.lock` package names in the report. Expected: `tray-icon` and its tree (`muda`, gtk-rs on Linux targets, …).
- [ ] **Step 5: Commit** `feat(planeserver): egui window and tray icon`

### Task 6: Docs, findings, spec, gates

**Files:**
- Modify: `README.md`, `docs/logic-findings.md`, `docs/superpowers/specs/2026-11-23-airscout-rs-design.md:83`

- [ ] **Step 1: README**
  - Status row (`:22`): add 5d.
  - Binary list (`:35-37`): add `airscout-planeserver`.
  - New section `## Plane server (airscout-planeserver)` after `## Plane feeds`, covering:
    - the purpose and both modes;
    - the config path and every key with its default;
    - restricted feeds and the disclaimer key;
    - the AirScoutServer feed URL to use, `http://<host>:9872/planes.json`. Upstream S5: the plugin's default is 9880, which is AirScout's own web server.
  - **Warning:** the server listens on all interfaces, without authentication, on by default. Say how to bind loopback.
  - Service setup: a systemd user unit, a launchd agent plist, and a Task Scheduler `schtasks` line. Each runs `airscout-planeserver --headless`, with `localhost`/placeholder paths only.
  - Linux build needs `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`.
  - Network sockets table (`:518-528`): add TCP 9872. Note that it does not collide with the Win-Test UDP 9872 receiver. Adjust the sentence at `:513`.
  - Privacy (`:564`): feed credentials live only in `planeserver.toml`, and the planes list is visible to anyone on the LAN.
- [ ] **Step 2: Findings** from #325, in this order:
  - S1 (the writer stub, the dead types);
  - S2 (the database updater parses an empty string, dead URLs; not ported);
  - S3 (feed index from `cb_WebFeed1`);
  - S4 (one `HttpListener` per request, and 200 for every path; Rust: one listener, the 4d caps, 404 for unknown paths);
  - S5 (plugin default 9880 against 9872);
  - the `version` literal 1 in both writers (amends #269; also fixes the 4d app writer);
  - `/planes.json` built per request from memory, not from `Tmp/planes.json` every 60 s;
  - the query cut before routing;
  - no coverage map, no autorun key, no `netsh`, no CPU/memory/DB status;
  - restricted feeds gated by the disclaimer key, in headless mode too;
  - the feed interval setting (C# none);
  - the config clamps;
  - IPv4 only (as #271).
  - Each finding gets a verified `file:line` and the label **Ported 1:1**, **Port deviation** or **New design**.
- [ ] **Step 3: Spec** `:83`: "headless daemon or egui window with tray (owner Q1)".
- [ ] **Step 4: Full gates:**
  - the macOS gates;
  - the hamlib clippy pair;
  - the Linux docker gate (extended apt line, `LINUX_OK`);
  - the Windows mingw gate (`WINGNU_OK`).
  - Then cleanup: `rm -rf target/linux target/wingnu` and `docker system prune -f`.
- [ ] **Step 5: Commit** `docs: stage 5d README, planeserver sockets and logic findings`
