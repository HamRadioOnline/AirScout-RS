# Stage 5c — Maps (`as-maps`, `maptile-gen`, mirror preloader) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stage 5c of the spec, as split in the survey: the `as-maps` tile store (ScoutBase.Maps, GMap `.gmdb` schema), the `maptile-gen` binary (MapTileGenerator, owner decision Q6), a store-first `walkers` tile source in the app, and the map preloader ported 1:1 for the airscout.eu mirror only and off by default (owner decision Q3).

**Architecture:**
- **`as-maps`** (new crate `crates/as-maps`, no GUI, no network): `TileStore` over SQLite in the GMap.NET `.gmdb` schema (`Tiles`, `TilesData`), so a C# `Data.gmdb` opens as is; the GMap provider `DbId`s; the C# slippy-tile maths and the preloader tile list.
- **`maptile-gen`** (new crate `crates/maptile-gen`, binary): `export` (the C# Start button: store to a `z/x/y.png` tree, file time = cache time), `import` (a `.gmdb` or a `z/x/y` tree into a store) and `combine` (the C# "Create PNG" button). Offline.
- **App** (`crates/airscout`): `StoreFirst`, a `walkers::TiledSource` that serves a tile from the store when it is there and otherwise asks the existing `HttpSource` (whose HTTP cache in `<data>/tiles` stays untouched, risk R4). The four `HttpTiles` call sites switch to one `tiles::tile_layer` helper. The preloader is one more background job that fills the store from the mirror.
- **Golden vectors** (`tools/golden-harness/MapsCases.cs`): the GMap `DbId` formula and the C# `long2tilex`/`lat2tiley` tile ranges, run in the docker harness.

**Tech Stack:** Rust 1.95+, rusqlite 0.32 (bundled, already in the workspace), chrono, clap 4, png 0.18 (workspace), fastrand 2 (already in `Cargo.lock` through tempfile; new direct edge), walkers 0.66 (`TiledSource`, `HttpSource`, `StyledSources`), reqwest blocking (preloader), mockito, assert_cmd, tempfile. Golden harness: .NET 8 SDK in docker `mcr.microsoft.com/dotnet/sdk:8.0`.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`:
- the crate row `as-maps` (`:74`): "offline OSM tile cache in SQLite, feeds `walkers` tile source; used by `maptile-gen`";
- the binary row `maptile-gen` (`:85`);
- "Maps work offline via `as-maps` cache (prefilled by `maptile-gen` or on-demand browsing), honouring OSM tile usage policy (User-Agent, rate limit)" (`:149-150`).

**Inputs:**
- Survey `docs/superpowers/surveys/2026-11-24-suite-tools-survey.md`: §6, §10, §11 (S17, S18), §12 (5c) and "Decisions (project owner, 2026-11-24)" (Q3, Q6).
- Ledger `docs/superpowers/ledgers/2026-11-24-stage5b-elevation-tools.md` (deferred minors; the 1 GiB / `.part` / untrusted-input patterns).
- Upstream `target/upstream/AirScout/` @ `91b02cb`, read-only:
  - `ScoutBase/ScoutBase.Maps/MapDatabase.cs` (schema `:110-130` area, `TileExists`, `TileInsert` `:222-253`);
  - `MapTileGenerator/MainDlg.cs` (`btn_Start_Click` `:52-88`, `CombineImages` `:94-142`);
  - `AirScout/MapPreloader.cs` (whole file, 270 lines), started at `AirScout/MapDlg.cs:2288-2289` (RUNONCE) and `:2393-2394` (RUNPERIODICALLY);
  - `AirScout/OptionsDlg.Designer.cs:7069-7117` (checkbox "Enable Map Preloading", spinner "down to \r\nZoom Level:" max 11);
  - `AirScout/Properties/Settings.settings` (`Map_Preloader_Enabled` False, `Map_Preloader_MaxZoom` 11);
  - `GreatMaps/GMap.NET.Core/GMap.NET.MapProviders/GMapProvider.cs:232-236` (`DbId`) and the provider GUIDs (`OpenStreetMapProvider.cs:461`, `OpenTopoMapProvider.cs:27`, `OpenCycleLandscapeMapProvider.cs:25`);
  - `GreatMaps/GMap.NET.Core/GMap.NET.CacheProviders/SQLitePureImageCache.cs` (`CacheTime` written as `DateTime.Now`, `:725`).

**Stage split:**
- **5c (this plan):** `as-maps`, `maptile-gen`, `StoreFirst` in the app, the mirror preloader, README, findings.
- **Not in 5c:** MBTiles (survey §6.5 option, no owner decision; later if asked), an "Import tiles…" button in the app (`maptile-gen import` covers it), `airscout-planeserver` (5d).

## Global Constraints

Everything in the 5b Global Constraints still applies where it concerns the touched crates. Changes and additions:

- **Port policy:**
  - The preloader is ported 1:1 (owner Q3): zoom 5 up to the max zoom, the whole settings area, C# loop order, Fisher–Yates shuffle, 100 ms between tiles, RUNONCE / RUNPERIODICALLY as the other background jobs, the status texts. Deviations are only those listed in this plan, each logged in `docs/logic-findings.md`.
  - The next finding number is **#306**; check the last row first. Task 6 assigns the numbers in its list order.
  - Every C# `file:line` in code comments and findings is checked against `target/upstream`.
- **Owner decisions (binding):** Q3: preloader for the airscout.eu mirror only (OSM provider), off by default (`map_preloader_enabled = false`). No bulk request to any other provider's server and never to `tile.openstreetmap.org`. Honest User-Agent. Q6: `maptile-gen` is a separate binary.
- **Network:**
  - `as-maps` and `maptile-gen` make no network request.
  - The preloader requests only `<map_mirror_url><z>/<x>/<y>.png` (default `http://www.airscout.eu/downloads/ScoutBase/1/MapData/OpenStreetMaps/`), one tile at a time, at least 100 ms apart, with a `reqwest` client that sends `as_net::user_agent()` (as `crate::services::client()`, 30 s timeout). It runs only when `map_preloader_enabled` is on and the map provider is `OpenStreetMap`. No call sign, locator or position in any request.
  - Map browsing stays as it is: `walkers` `HttpSource` with `tiles::http_options` (honest UA, HTTP cache headers). The store is read, never written, by the map view.
- **Data:** no tile image is committed. Tests build tiny PNGs in memory (`png` crate, 1 × 1 or 256 × 256) or use byte strings; the golden file holds numbers only.
- **Untrusted input:** a `.gmdb` from elsewhere, a `z/x/y` tree and mirror answers never panic. A tile blob over `MAX_TILE_BYTES` (4 MiB) or a body that is not a PNG/JPEG is refused (error or skip, never stored). A store that is not a GMap database (missing tables) is an error naming the file; `TileStore::open` creates the tables only in a new or empty file.
- **Files:** every file `maptile-gen` writes goes to `<target>.part` first and is then renamed. No tool deletes a user file. Store writes are one transaction per tile (`Tiles` + `TilesData`).
- **Platform:** macOS, Linux and Windows; no `cfg` code. `#![forbid(unsafe_code)]` in `as-maps` and `maptile-gen`.
- **Cargo:**
  - Every cargo command carries `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
  - Disk: total under 120 GB; `target/` under 60 GB, clean at 40 GB with `cargo clean --profile dev` (same env); never plain `cargo clean`.
  - Tasks 2, 3 and 5 change dependency edges. Each runs one `cargo build` without `--locked` and commits `Cargo.lock`. `git diff Cargo.lock | grep '^+name'` prints only `+name = "as-maps"` (Task 2), `+name = "maptile-gen"` (Task 3), nothing in Task 5.
  - New workspace entry: `fastrand = "2"` (Task 5).
- **Gates (every task that changes Rust code):** `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked`. Task 6 also runs the hamlib clippy pair, the Linux docker gate (rust:1.95, `target/linux`, prints `LINUX_OK`) and the Windows mingw gate (`target/wingnu`, prints `WINGNU_OK`), then `rm -rf target/linux target/wingnu` and `docker system prune -f`.
- **Commits:** signed (`export SSH_AUTH_SOCK=~/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock`, `git commit -S`). No push.

## Review Focus

1. **A foreign or damaged `.gmdb`** (not SQLite, SQLite without `Tiles`, a `Tiles` row without `TilesData`, a NULL or 50 MB blob, `CacheTime` NULL or in an odd text format): `maptile-gen` and the app report the file, skip the row or fail cleanly; nothing panics, nothing huge is allocated. Tests in Task 2 (`open_refuses_foreign_sqlite`, `get_refuses_oversized_blob`, `cache_time_formats`) and Task 3 (`export_skips_tile_without_data`).
2. **A mirror that misbehaves** (404, HTML error page with 200, a 20 MB body, a stalled connection): nothing is stored, the run goes on with the next tile, memory stays bounded. Tests in Task 5 (`preload_skips_bad_answers`, `preload_caps_body`).
3. **An area near the poles or across ±180°** (settings allow up to ±90° latitude): C# computes y < 0 or ≥ 2^z and would request tiles that do not exist. The Rust list clamps x and y to `0..2^z-1` (deviation, logged). Test in Task 2 (`preload_tiles_clamped_at_poles`); the raw C# values are in the golden.
4. **The store is busy or broken while the map draws** (preloader writing, file deleted, disk full): the map never blocks longer than the busy timeout and falls back to HTTP for that tile. Test in Task 4 (`store_error_falls_back`).
5. **Settings change while the preloader runs** (switched off, provider changed, area changed): the background service restarts as for the other jobs; the old run stops within the cancel slice (200 ms) and sends nothing more. Test in Task 5 (`preload_stops_on_cancel`).

---

### Task 1: Golden vectors (`DbId`, tile ranges)

**Files:**
- Create: `tools/golden-harness/MapsCases.cs`
- Modify: `tools/golden-harness/Program.cs` (register the case set, as the other `*Cases.cs`)
- Create: `tests/golden/maps.json`
- Test: none in Rust yet (Task 2 consumes the file); the check is the `jq` command in Step 3

**Interfaces:**
- Produces: `tests/golden/maps.json` with
  - `db_ids`: `[{ "name": "OpenStreetMap", "guid": "0521335C-92EC-47A8-98A5-6FD333DDA9C0", "db_id": <int> }, { "name": "OpenTopoMap", "guid": "B8A6A7B4-3034-495D-BC83-96F6034748B1", … }, { "name": "OCM Landscape", "guid": "BDBAA939-6597-4D87-8F4F-261C49E35F56", … }]` — computed with the verbatim C# formula `Math.Abs(BitConverter.ToInt32(new SHA1CryptoServiceProvider().ComputeHash(Id.ToByteArray()), 0))` (`GMapProvider.cs:236`);
  - `ranges`: for each area in `[{35,60,-15,30} (the default area), {50,51,10,11}, {-10,10,170,180}, {80,90,-180,180}, {-90,-80,-10,10}]` (min_lat, max_lat, min_lon, max_lon) and each zoom 0..=11: `{ "area": [...], "z": z, "xmin", "xmax", "ymin", "ymax", "total" }` from verbatim copies of `long2tilex`, `lat2tiley` (`MapPreloader.cs:56-64`) and the loop bounds (`:140-150`: `xmin = long2tilex(MinLon)`, `xmax = long2tilex(MaxLon)`, `ymin = lat2tiley(MaxLat)`, `ymax = lat2tiley(MinLat)`, `total = (xmax-xmin+1)*(ymax-ymin+1)` when both spans are ≥ 0, else 0). Values that C# gives as `int.MinValue` (NaN cast) are written as they come.

- [ ] **Step 1:** Write `MapsCases.cs` and register it in `Program.cs`.
- [ ] **Step 2:** Run the harness (`tools/golden-harness/run.sh`, docker) and write `tests/golden/maps.json`.
- [ ] **Step 3:** Check: `jq '[.db_ids|length, .ranges|length]' tests/golden/maps.json` prints `[3, 60]`. Expected: the OSM `db_id` is `1492776782` and the default area at z = 11 has `total` in the order of 65,000 (survey §6.3); if not, stop and report (it means the formula copy is wrong).
- [ ] **Step 4:** Commit `test(golden): GMap provider DbIds and preloader tile ranges`.

### Task 2: `as-maps` (store, `DbId`s, tile list)

**Files:**
- Create: `crates/as-maps/Cargo.toml`, `crates/as-maps/src/lib.rs`, `crates/as-maps/src/store.rs`, `crates/as-maps/src/tiles.rs`
- Test: `crates/as-maps/tests/store.rs`, `crates/as-maps/tests/tiles.rs`

**Interfaces:**
- Consumes: `tests/golden/maps.json` (Task 1); `as_geo::latlon::GRect` (area).
- Produces (all `pub`, re-exported from `lib.rs`):
  - `pub struct TileKey { pub x: u32, pub y: u32, pub zoom: u8, pub db_id: i32 }` (`Copy`, `Eq`, `Hash`, `Debug`).
  - `pub struct TileInfo { pub id: i64, pub key: TileKey, pub cache_time: Option<chrono::NaiveDateTime> }`.
  - `pub enum MapsError` (`thiserror`): `Sql { path: PathBuf, source: rusqlite::Error }`, `NotAGmdb(PathBuf)`, `TooLarge { key: TileKey, len: usize }`, `Io(std::io::Error)`.
  - `pub struct TileStore` (`Send + Sync`, one `rusqlite::Connection` behind a `Mutex`), with
    - `pub fn open(path: &Path) -> Result<Self, MapsError>` — creates the file, the two tables with the exact C# DDL (`MapDatabase.cs`, `TilesCreateTable` / `TilesDataCreateTable`) and `CREATE INDEX IF NOT EXISTS IndexOfTiles ON Tiles (X, Y, Zoom, Type)` only when the file has no tables at all; a file with other tables but no `Tiles`/`TilesData` is `NotAGmdb`. WAL journal, `busy_timeout` 2 s, `foreign_keys = ON`.
    - `pub fn open_read_only(path: &Path) -> Result<Self, MapsError>` — never creates or migrates; missing tables is `NotAGmdb`.
    - `pub fn exists(&self, k: TileKey) -> Result<bool, MapsError>` (`TileExists`).
    - `pub fn get(&self, k: TileKey) -> Result<Option<Vec<u8>>, MapsError>` — `TooLarge` above `MAX_TILE_BYTES`, checked with `length(Tile)` before the blob is read; NULL blob or missing `TilesData` row is `Ok(None)`.
    - `pub fn get_by_id(&self, id: i64) -> Result<Option<Vec<u8>>, MapsError>` (same rules).
    - `pub fn insert(&self, k: TileKey, cache_time: chrono::NaiveDateTime, data: &[u8]) -> Result<bool, MapsError>` — `false` (nothing written) if the key exists; `TooLarge` above the cap; one transaction; `id` by SQLite (`INTEGER PRIMARY KEY`), equivalent to `MAX(id)+1`.
    - `pub fn list(&self) -> Result<Vec<TileInfo>, MapsError>` (`TilesGetAll`, ordered by `id`); `pub fn count(&self) -> Result<u64, MapsError>`; `pub fn types(&self) -> Result<Vec<(i32, u64)>, MapsError>` (distinct `Type` with counts).
  - `pub const MAX_TILE_BYTES: usize = 4 * 1024 * 1024;`
  - `CacheTime` text: written as `%Y-%m-%d %H:%M:%S%.7f` in local time (System.Data.SQLite's default for `DateTime.Now`); read accepting that, without the fraction, with `T` instead of the space, and an integer/real (Julian day) — anything else and NULL give `None`.
  - `pub const DBID_OSM: i32`, `DBID_OPENTOPOMAP: i32`, `DBID_OCM_LANDSCAPE: i32` (values from the golden); `pub fn db_id(provider_name: &str) -> Option<i32>` for the names `crates/airscout/src/tiles.rs` uses (`"OpenStreetMap"`, `"OpenTopoMap"`, `"OCM Landscape"`).
  - `pub fn long2tilex(lon: f64, z: u8) -> i64`, `pub fn lat2tiley(lat: f64, z: u8) -> i64` (C# formulas; a NaN result is `i64::from(i32::MIN)` as the C# cast gives).
  - `pub fn preload_tiles(area: &GRect, zmin: u8, zmax: u8) -> Vec<(u8, u32, u32)>` — `(z, x, y)` in the C# order (z ascending, then x, then y), the C# bounds clamped to `0..=2^z-1` (Review Focus 3); empty when `zmax < zmin`.
  - `pub const MIRROR_URL: &str = "http://www.airscout.eu/downloads/ScoutBase/1/MapData/OpenStreetMaps/";`

- [ ] **Step 1: Write the failing tests**
  - `tiles.rs`: `db_ids_match_golden` (the three consts equal the golden); `ranges_match_golden_where_valid` (for every golden range whose bounds are already inside `0..2^z-1`, `long2tilex`/`lat2tiley` give the golden bounds and `preload_tiles` gives `total` entries); `preload_tiles_clamped_at_poles` (area `{80,90,-180,180}` and `{-90,-80,-10,10}` at z = 3: every `y` in `0..8`, every `x` in `0..8`, no duplicate); `preload_order_is_z_x_y` (area `{50,51,10,11}`, z 5..=6: sorted by `(z, x, y)` equals the list); `empty_when_max_below_min`.
  - `store.rs`: `open_creates_gmap_schema` (the `sqlite_master` SQL of both tables equals the C# DDL text); `insert_get_roundtrip` (insert, `exists`, `get` returns the bytes, second insert returns `false`); `list_and_types` (two types, `types()` counts); `open_refuses_foreign_sqlite` (a SQLite file with a table `foo`: `open` and `open_read_only` give `NotAGmdb`, the file is unchanged); `open_read_only_never_creates` (a missing path is an error and no file appears); `get_refuses_oversized_blob` (a row written with raw SQL with a 4 MiB + 1 blob gives `TooLarge`); `tiles_row_without_data_is_none`; `cache_time_formats` (rows written with raw SQL: `2019-12-14 10:11:12.1234567`, `2019-12-14 10:11:12`, `2019-12-14T10:11:12`, a Julian real, `garbage`, NULL → the expected `Option<NaiveDateTime>`); `opens_a_csharp_written_db` (a db built with the exact C# DDL and one row inserted with the C# column order and text time).
- [ ] **Step 2:** Run `cargo test -p as-maps`: FAIL (crate missing).
- [ ] **Step 3:** Create the crate (workspace member through `crates/*`; deps `rusqlite.workspace`, `chrono.workspace`, `thiserror.workspace`, `as-geo`; dev `tempfile`, `serde_json`, `png`). Implement the interfaces above. Run `cargo build` once without `--locked`, commit `Cargo.lock`.
- [ ] **Step 4:** Run `cargo test -p as-maps`: PASS. Run the gate.
- [ ] **Step 5:** Commit `feat(as-maps): GMap tile store, provider DbIds and the preloader tile list`.

### Task 3: `maptile-gen`

**Files:**
- Create: `crates/maptile-gen/Cargo.toml`, `crates/maptile-gen/src/main.rs`, `crates/maptile-gen/src/export.rs`, `crates/maptile-gen/src/import.rs`, `crates/maptile-gen/src/combine.rs`
- Test: `crates/maptile-gen/tests/cli.rs`

**Interfaces:**
- Consumes: `as_maps::{TileStore, TileKey, TileInfo, MAX_TILE_BYTES, db_id}` (Task 2).
- Produces: the binary `maptile-gen` with subcommands:
  - `export --db <GMDB> --out <DIR> [--type <DBID|PROVIDER>]` — `btn_Start_Click`: for every tile, `<DIR>/<zoom>/<x>/<y>.png` with the blob; file mtime set to `CacheTime` (`File::set_modified`, local time). A file that exists with the same mtime is skipped (the intended condition; S17 precedence bug not reproduced). A tile without data is skipped with a message (S17 null reference not reproduced). Opens the db read-only. If the db holds more than one `Type` and `--type` is not given: error listing the types with counts (C# writes all types into the same tree, so tiles of different providers overwrite each other; deviation). Prints `Saving [i of n] <file>` per tile and `Ready.` at the end (C# texts).
  - `import --db <GMDB> <SOURCE> [--type <DBID|PROVIDER>]` — `SOURCE` is a `.gmdb` (copies every tile of the given type, default all types, keeping `CacheTime`) or a directory `<z>/<x>/<y>.png` (`--type` default `OpenStreetMap`; cache time = file mtime). Existing keys are kept (`insert` returns `false`), counted as "kept". Files above `MAX_TILE_BYTES`, names that are not numbers, zoom > 30 and x/y ≥ 2^z are skipped with a message. Symlinks are not followed. Prints `imported N, kept K, skipped S`.
  - `combine <DIR> --out <PNG> [--overwrite]` — `CombineImages`: every `*.png` of `DIR` (not recursive), sorted by file name (ordinal, as `Array.Sort`), placed side by side from x = 0, height = the tallest, background `SystemColors.AppWorkspace` = RGB `(171, 171, 171)`; output written via `.part`; refuses an existing `--out` without `--overwrite` (the C# fixed path under the exe name is S17, not reproduced). Each input decoded with `png` with `Limits` of 64 MiB; total width capped at 65,535 px (error above).
  - `--type` accepts a number or one of the provider names of `as_maps::db_id`.

- [ ] **Step 1: Write the failing tests** (`assert_cmd`, temp dirs, tiny PNGs built in the test):
  - `export_writes_zxy_tree_with_cache_time` (2 tiles, one type: files at `11/1100/700.png` etc. with the blob bytes and mtime = cache time to the second);
  - `export_skips_same_time_and_rewrites_other_time` (pre-create one file with the right mtime and one with another: the first untouched, the second rewritten);
  - `export_skips_tile_without_data`; `export_refuses_mixed_types_without_flag` (error text lists both DbIds with counts) and `export_type_filter`;
  - `import_gmdb_then_export_roundtrip` (bytes and cache time survive);
  - `import_dir_tree` (default type is `DBID_OSM`; a second run reports all "kept"; a 4 MiB + 1 file and `a/b/c.png` are skipped);
  - `combine_side_by_side` (two PNGs 2×3 and 4×5 named `b.png`, `a.png`: output 6×5, `a.png` first, the empty pixels `(171,171,171)`); `combine_refuses_existing_out`;
  - `no_network` (the crate has no HTTP dependency: `cargo tree -p maptile-gen -e normal` has no `reqwest`; run it in the test via `env!("CARGO")`, or as a documented gate command in the report if the test cannot call cargo).
- [ ] **Step 2:** Run `cargo test -p maptile-gen`: FAIL.
- [ ] **Step 3:** Create the crate (deps `as-maps`, `clap.workspace`, `png.workspace`, `chrono.workspace`; dev `assert_cmd`, `tempfile`, `rusqlite`). Implement. `cargo build` once without `--locked`, commit `Cargo.lock`.
- [ ] **Step 4:** Run `cargo test -p maptile-gen`: PASS. Run the gate.
- [ ] **Step 5:** Commit `feat(maptile-gen): export, import and combine over the as-maps store`.

### Task 4: Store-first tile source in the app

**Files:**
- Modify: `crates/airscout/src/tiles.rs` (add `StoreFirst`, `tile_layer`, `store_path`)
- Modify: `crates/airscout/src/ui/map.rs:60,101-115,163-166`, `crates/airscout/src/ui/options/coverage.rs:38,82`, `crates/airscout/src/ui/dialogs/map_station.rs:125,195`, `crates/airscout/src/ui/dialogs/traffic.rs` (its `HttpTiles` field and constructor)
- Modify: where the app builds `Paths` / opens its databases at startup (`crates/airscout/src/services.rs`), to open the store once
- Modify: `crates/airscout/Cargo.toml` (dep `as-maps`)
- Test: unit tests in `crates/airscout/src/tiles.rs`

**Interfaces:**
- Consumes: `as_maps::{TileStore, TileKey, db_id}` (Task 2); `walkers::{TiledSource, TileData, SourceData, HttpSource, StyledSources, Style, TileId, Attribution, Stats}`.
- Produces:
  - `pub fn store_path(data_dir: &Path) -> PathBuf` = `data_dir.join("maptiles.gmdb")`.
  - `pub struct StoreFirst<F: TiledSource> { … }` with `pub fn new(store: Option<Arc<TileStore>>, db_id: Option<i32>, fallback: F) -> Self`; `impl<F: TiledSource> TiledSource for StoreFirst<F>`: `data(id)` returns `TileData::Ready(Arc::new(SourceData::Encoded(bytes)))` when the store has `TileKey { x, y, zoom, db_id }`, else `fallback.data(id)`; a store error is logged once (`log::warn!`, no tile bytes in the text) and that lookup falls back. Results of store lookups (hit or miss) are memoised in a map of at most 1024 entries (cleared when full), so a frame does not query SQLite again for a tile already looked up. `attribution`, `tile_size`, `max_zoom`, `stats` come from `fallback`.
  - `pub fn tile_layer(p: &TileProvider, store: Option<Arc<TileStore>>, cache_dir: &Path, ctx: &egui::Context) -> StyledSources` = `StyledSources::new(Style::default(), ctx.clone()).with_tiled_source("", StoreFirst::new(store, as_maps::db_id(p.name), HttpSource::new(ProviderSource(p.clone()), http_options(cache_dir), ctx.clone())))`.
  - `AppState` (or the `Paths` the UI holds) gets `pub map_store: Option<Arc<TileStore>>`, opened with `TileStore::open(&store_path(data_dir))` at startup; an error is shown once in the status bar and gives `None` (the map then works as before).
  - The four UI sites hold `Option<StyledSources>` instead of `Option<HttpTiles>` and draw it with `map.with_layer(t, opacity)` (`&mut StyledSources` implements `walkers::Layer`). The attribution drawing stays as it is (from `TileProvider`).

- [ ] **Step 1: Write the failing tests** (a fake fallback `struct Fake { calls: Vec<TileId> }` implementing `TiledSource` and returning `TileData::Missing`):
  - `store_hit_is_ready_and_skips_fallback` (store with one OSM tile: `data` is `Ready` with those bytes; `Fake.calls` is empty);
  - `store_miss_asks_fallback`; `no_store_asks_fallback`; `unknown_provider_asks_fallback` (`db_id` `None`);
  - `store_error_falls_back` (store whose file was replaced by a non-SQLite file after open, or a closed/locked connection: `data` returns the fallback's answer, no panic);
  - `lookup_is_memoised` (two `data` calls for the same id: one SQL query — count with a `TileStore` test hook or by deleting the row between the calls and still getting `Ready`);
  - `type_must_match` (an OpenTopoMap tile is not served to the OSM layer).
- [ ] **Step 2:** Run `cargo test -p airscout tiles::`: FAIL.
- [ ] **Step 3:** Implement `StoreFirst`, `tile_layer`, `store_path`; open the store at startup; switch the four UI sites. Existing UI tests (`egui_kittest`) must stay green unchanged.
- [ ] **Step 4:** Run the tests: PASS. Run the gate.
- [ ] **Step 5:** Commit `feat(airscout): map layers read the as-maps store first, then HTTP`.

### Task 5: Mirror preloader

**Files:**
- Create: `crates/airscout/src/map_preloader.rs`
- Modify: `crates/airscout/src/background.rs` (spawn the job), `crates/airscout/src/lib.rs` (module)
- Modify: `crates/as-settings/src/gui.rs` (fields), `crates/as-settings/src/lib.rs` (`Background::map_mirror_url`)
- Modify: `crates/airscout/src/ui/options/map.rs` (checkbox, spinner, estimate line)
- Modify: `Cargo.toml` (workspace `fastrand = "2"`), `crates/airscout/Cargo.toml`
- Test: unit tests in `map_preloader.rs`; `crates/as-settings` defaults test

**Interfaces:**
- Consumes: `as_maps::{TileStore, TileKey, preload_tiles, DBID_OSM, MIRROR_URL, MAX_TILE_BYTES}`; `crate::background::{sleep_cancellable, Schedule, BackgroundService::spawn_job}` (private today; use them from inside `background.rs` or widen to `pub(crate)`); a preloader client built like `crate::services::client()` (same `as_net::user_agent()`) but with a 30 s timeout, so a stalled tile does not hold the run for the 300 s of the updaters' client.
- Produces:
  - Settings: `GuiSettings::map_preloader_enabled: bool` (default `false`), `GuiSettings::map_preloader_max_zoom: u8` (default `11`, clamped to `0..=11` on load); `Background::map_mirror_url: String` (default `as_maps::MIRROR_URL`, not shown in the UI, like `stations_url`). Serde defaults so old settings files load.
  - `pub struct PreloadOutcome { pub total: usize, pub missing: usize, pub stored: usize, pub failed: usize, pub left: usize }`.
  - `pub fn preload_run(store: &TileStore, client: &reqwest::blocking::Client, base_url: &str, list: Vec<(u8, u32, u32)>, delay: Duration, cancel: &AtomicBool, progress: &mut dyn FnMut(String)) -> Option<PreloadOutcome>` — `None` when cancelled. Steps, 1:1 with `OnDoWork` (`MapPreloader.cs:113-260`) except where noted:
    1. `progress("MapPreloader getting tiles from database.")`; drop every tile that `store.exists(TileKey{…, db_id: DBID_OSM})` (**S18 fixed**: C# checks another database than it writes, so it downloads everything every run; logged as a port deviation).
    2. Shuffle the rest with Fisher–Yates as C# (`n` from `len` down to 2, `k = rng(0..=n-1)`), using `fastrand`.
    3. For each tile: `progress(format!("Preloading /{z}/{x}/{y}.png"))`; GET `format!("{base_url}{z}/{x}/{y}.png")`; store only if the status is 200, the body is at most `MAX_TILE_BYTES` (read through `take(MAX_TILE_BYTES + 1)`, so a big body is never buffered whole) and starts with the PNG or JPEG signature; `insert(key, Local::now().naive_local(), &body)`; any other answer or error counts as `failed` and the run goes on; then `sleep_cancellable(delay, cancel)`.
    4. Return the counts; the job then reports `format!("MapPreloader finished, total  {total} tile(s), {left} left.")` (C# text, two spaces after "total"; `left = missing - processed`).
  - The job in `BackgroundService::start`, started under the same `sched.run` / `sched.periodic` as the other updaters (`MapDlg.cs:2288`, `:2393`), only when `gui.map_preloader_enabled` **and** one of `elevation.globe`, `elevation.srtm3`, `elevation.srtm1` is on (C# `:122-130`; ported 1:1, logged) **and** `gui.map_provider == "OpenStreetMap"`. With another provider it sends one status text `MapPreloader: no mirror for provider <name>, nothing preloaded.` and ends (C# pulls from the provider's own server, `:202`; not ported, owner Q3). It opens its own `TileStore` connection on `store_path(data_dir)`; `delay` = 100 ms; list = `preload_tiles(area, 5, max_zoom)` (C# hard-codes `zmax = 11` and ignores `Map_Preloader_MaxZoom`; the port uses the setting, default 11, logged). No LED (C# has none). Progress texts go to the calculations status line as the C# `SayCalculations` (`MapDlg.cs:9184-9187`).
  - Options › Map: checkbox `Enable Map Preloading`; spinner `down to Zoom Level:` `0..=11`, enabled only with the checkbox; under them a label `Preloads about N tiles from www.airscout.eu (OpenStreetMap only); off by default.` where `N = preload_tiles(area, 5, max_zoom).len()`, computed when the area or the zoom changes.

- [ ] **Step 1: Write the failing tests** (mockito server, `delay = Duration::ZERO`, a temp store, the tile list given directly):
  - `preload_downloads_missing_tiles_only` (list of 3, one already in the store: exactly 2 GETs (`expect(1)` per path, `expect(0)` for the stored one); both stored with type `DBID_OSM`);
  - `preload_sends_honest_user_agent` (mock matches `User-Agent` = `as_net::user_agent()`);
  - `preload_skips_bad_answers` (404, 200 with `<html>`, 500: nothing stored, `failed == 3`, the run reaches the end);
  - `preload_caps_body` (200 with `MAX_TILE_BYTES + 1` bytes starting with the PNG signature: not stored);
  - `preload_stops_on_cancel` (cancel set by the progress callback after the first tile: returns `None`, at most 1 request);
  - `preload_order_is_shuffled_but_complete` (20 tiles: every path requested once);
  - job gating, through a function `fn preloader_wanted(s: &Settings) -> Result<(), &'static str>` or similar: `disabled_by_default`, `needs_osm_provider`, `needs_an_elevation_model` (C# coupling), `max_zoom_clamped`;
  - `as-settings`: `map_preloader_defaults` (`false`, `11`, the mirror URL) and an old settings file without the fields still loads.
- [ ] **Step 2:** Run the tests: FAIL.
- [ ] **Step 3:** Implement; wire the job; add the Options controls. Add `fastrand` to the workspace and `crates/airscout`. `cargo build` once without `--locked`, commit `Cargo.lock` (no new `+name`).
- [ ] **Step 4:** Run the tests: PASS. Run the gate.
- [ ] **Step 5:** Commit `feat(airscout): map preloader for the airscout.eu mirror, off by default`.

### Task 6: Docs, findings, gates

**Files:**
- Modify: `README.md` (sections `maptile-gen` and "Map preloader"; the sockets / outbound-hosts table; the offline-maps paragraph)
- Modify: `docs/logic-findings.md` (rows from #306, after the last row, before `## Data sources`)
- Check: `docs/superpowers/specs/2026-11-23-airscout-rs-design.md:74,85,149-150` (no change expected; if a line disagrees with this plan, change the spec and say so in the report)

**Interfaces:** none (docs).

- [ ] **Step 1: README.**
  - `maptile-gen`: the three subcommands with one example each (`maptile-gen import --db maptiles.gmdb ~/old/Data.gmdb`; `maptile-gen export --db maptiles.gmdb --out tiles/ --type OpenStreetMap`; `maptile-gen combine tiles/11/1100 --out row.png`); where the app's store lives (`<data>/maptiles.gmdb`); that a C# GMap cache (`%LOCALAPPDATA%\GMap.NET\TileDBv5\en\Data.gmdb`) can be imported; no network.
  - "Map preloader": off by default; only with the OpenStreetMap provider and an enabled GLOBE/SRTM3/SRTM1 model (as upstream); downloads every missing tile of zoom 5 to the chosen level for the whole area from `www.airscout.eu` over plain HTTP, one tile every 100 ms — for the default area and level 11 about N tiles (take N from Task 1's golden) and several hours per run; never from `tile.openstreetmap.org` or other providers; tiles are OSM-derived (ODbL attribution applies; their age is unknown).
  - Sockets table: `maptile-gen` none; app preloader outbound `www.airscout.eu` only when enabled.
- [ ] **Step 2: Findings** (each with C# `file:line`, what happens, and the port's handling; check every cite):
  1. S17a `MapTileGenerator/MainDlg.cs:39`: the database path is stored in `Database_UpdateURL` — Not reproduced (CLI args).
  2. S17b `:69`: operator precedence `d != null && !File.Exists(f) || time != CacheTime` — Not reproduced (`export_skips_tile_without_data`).
  3. S17c `:101`: `Path.Combine(Application.ExecutablePath, "zoom.png")` — Not reproduced (`--out`).
  4. Export writes all provider types into one tree, so tiles of different providers overwrite each other — Port deviation (`--type`, error on mixed types).
  5. S18a `AirScout/MapPreloader.cs:159` vs `:92`: the existence check reads `MapData.Database`, `LoadOSM` writes into the GMap cache, so every run downloads all tiles again — Port deviation (checks and writes the same store; `preload_downloads_missing_tiles_only`).
  6. S18b `:122-130`: the preloader runs only when an elevation model is enabled — Ported 1:1.
  7. S18c `:81`: the temp file is named after `y` only — Not reproduced (no temp file; body in memory, capped).
  8. S18d `:202`: other providers are bulk-downloaded from their own servers — Not ported (owner Q3).
  9. `:132`: the OSM-mirror check `GetType() == typeof(OpenStreetMapProvider)` vs the Rust provider name — New design note.
  10. `:113` area: `Map_Preloader_MaxZoom` (Options spinner, default 11) is ignored, `zmax = 11` is hard-coded (`:137`) — Port deviation (the setting is used).
  11. `:56-64`, `:140-150`: tile bounds are not clamped, so an area near the poles or across ±180° requests tiles that do not exist — Port deviation (clamped; `preload_tiles_clamped_at_poles`).
  12. `:117`, `:124`: `GMapProvider.UserAgent = "AirScout"` — Not reproduced (honest `as_net::user_agent()`).
  13. `:85-95`: any answer the `WebClient` accepts is cached (no size or type check) — Port deviation (200, ≤ 4 MiB, PNG/JPEG only).
  14. `MapDatabase.cs` `TileInsert`: `MAX(id)+1` outside a transaction (two writers can pick the same id) — Not reproduced (SQLite rowid, one transaction).
  15. `MapPreloader.cs:85` / `LoadOSM`: plain `http://` mirror — Ported 1:1 (README says so).
  Correct the numbers, wording and cites against the code; merge or split rows if the code shows otherwise, and say so in the report.
- [ ] **Step 3: Gates:** macOS gate; hamlib clippy pair (`cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings` and `cargo clippy -p as-cat --all-targets --locked -- -D warnings`); Linux docker (`LINUX_OK`); Windows mingw (`WINGNU_OK`); then `rm -rf target/linux target/wingnu` and `docker system prune -f` (keep the rust and dotnet images).
- [ ] **Step 4:** Commit `docs: stage 5c README, maps sockets and logic findings`.
