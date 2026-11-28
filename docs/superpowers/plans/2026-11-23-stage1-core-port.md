# AirScout-RS Stage 1 — Core Port + Golden Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the AirScout prediction core (geodesy, Maidenhead, propagation formulas, elevation model, propagation path/horizon, stations, settings) to Rust, with results proven equal to the original C# code by golden tests.

**Architecture:** Cargo workspace at the repo root. Pure-math crate `as-geo`; I/O crates `as-db`, `as-elevation`, `as-propagation`, `as-stations`, `as-settings` on top. A small .NET 8 console harness in `tools/golden-harness/` compiles the original C# source files (linked, not copied) and writes JSON fixtures to `tests/golden/`; Rust tests load these fixtures.

**Tech Stack:** Rust stable (edition 2021), rusqlite (bundled), serde/serde_json, toml, directories, reqwest (blocking, rustls), zip, thiserror, approx (tests), mockito (tests). .NET 8 SDK for the harness only.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`

**Upstream source:** https://github.com/dl2alf/AirScout at commit `91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`. Harness and porters read it from `$AIRSCOUT_UPSTREAM` (a local clone at that commit). `tools/fetch-upstream.sh` creates it under `target/upstream/AirScout`.

## Global Constraints

- Port 1:1. Do **not** fix suspected bugs in Stage 1. Reproduce them, and add each one to `docs/logic-findings.md` (file, line, what looks wrong, why). Stage 6 fixes them.
- Earth radius `6371.0` km (`LatLon.Earth.Radius`). Equivalent radius = `6371.0 * k_factor`.
- Elevation flags: `ELV_MISSING: i16 = -32768`, `TILE_MISSING: i16 = -32767`.
- Units match the original: distances in km, heights in m, step widths in m, frequency in GHz for `f1_radius`, angles in radians for epsilon, degrees for lat/lon/bearing.
- Library crates: no `unwrap()`/`expect()` outside tests. Errors via `thiserror`. `#![forbid(unsafe_code)]` in every crate.
- Golden tolerance: absolute `1e-9` for pure math (`as-geo`). Elevation integers must match exactly. Propagation epsilons: absolute `1e-12` rad.
- CI must pass on `macos-latest`, `ubuntu-latest`, `windows-latest`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`.
- Golden fixtures are committed. The harness is run only when regenerating them; CI does **not** need .NET.
- Data source for tiles: `http://www.airscout.eu/downloads/ScoutBase/1/ElevationData/{GLOBE,SRTM3,SRTM1,ASTER3,ASTER1}/` (verified 2026-11-23: `GLOBE/JO/JO50.zip` → 200, 273 106 bytes; it contains 577 `JO50xx.loc` JSON files).

## Review Focus

1. **Points near the antimeridian and poles** (e.g. lon 179.99 → −179.99, lat ±89.9). Expect: distance/bearing/destination finite and equal to the C# output. Pinned in Task 2 goldens.
2. **Lowercase, mixed-case, 4/6/8/10-char and invalid locators** (`"jo50aa"`, `"JO50"`, `"JO50AA12"`, `"ZZ99"`, `""`). Expect: same accept/reject decision and coordinates as C#. Pinned in Task 3.
3. **A path that crosses a tile that is not in the store** (sea or not downloaded). Expect: elevation 0 at those points, and `valid` follows the database-complete rule exactly as in C#. Pinned in Task 6.
4. **Identical start and end point (distance 0) and very short paths (< step width).** Expect: no panic, count = 1, and the same epsilons as C# (including `-inf`/NaN propagation if C# produces them). Pinned in Task 7.
5. **Interrupted or corrupt download** (truncated zip, HTTP 404, malformed `.loc` JSON). Expect: error returned, store unchanged, no partial tile written. Pinned in Task 8.

---

### Task 1: Golden harness + upstream fetch

**Files:**
- Create: `tools/fetch-upstream.sh`
- Create: `tools/golden-harness/GoldenHarness.csproj`
- Create: `tools/golden-harness/Program.cs`
- Create: `tools/golden-harness/Shims.cs` (only if the linked sources need missing types, e.g. `System.Windows`)
- Create: `tools/golden-harness/Cases.cs`
- Create: `tests/golden/README.md`
- Create: `docs/logic-findings.md`

**Interfaces:**
- Produces: JSON fixtures in `tests/golden/*.json`. Each file is `{"upstream_commit": "<sha>", "cases": [ {"fn": "<name>", "args": {...}, "result": ...}, ... ]}`. Doubles are written with `"R"` round-trip formatting. NaN/±Inf are written as the strings `"NaN"`, `"Infinity"`, `"-Infinity"`.
- Fixture files written by this task: `latlon.json`, `maidenhead.json`, `propagation_formulas.json`. Later tasks add cases to the harness and regenerate (`elevation_tile.json`, `elevation_path.json`, `propagation_path.json`, `propagation_horizon.json`).

- [ ] **Step 1:** Write `tools/fetch-upstream.sh`. It runs `git clone https://github.com/dl2alf/AirScout target/upstream/AirScout` (when the directory is missing), then `git -C … checkout 91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`. Run it. Expected: `git -C target/upstream/AirScout rev-parse HEAD` prints that SHA.
- [ ] **Step 2:** Install the .NET 8 SDK if `dotnet --version` fails (macOS: `brew install --cask dotnet-sdk`). **Ask the user before installing.**
- [ ] **Step 3:** Create a `net8.0` console project. Link (`<Compile Include="$(AIRSCOUT_UPSTREAM)/ScoutBase/ScoutBase.Core/X.cs" Link="Upstream/X.cs" />`) these files: `LatLon.cs`, `MaidenheadLocator.cs`, `GeographicalPoint.cs`, `Propagation.cs`. Add `Newtonsoft.Json`. Build with `dotnet build tools/golden-harness`. Add stubs to `Shims.cs` only for compile errors, and keep them minimal.
- [ ] **Step 4:** In `Cases.cs`, define a fixed case set:
  - Station pairs (lat1, lon1, lat2, lon2): JO50AA↔JO61UF centres, DL↔G (51.5,10)→(51.5,−0.1), across the antimeridian (10,179.99)→(10,−179.99), near the pole (89.9,0)→(89.9,180), identical points (50,10)→(50,10), southern hemisphere (−33.9,18.4)→(−34.6,−58.4).
  - Locators: `JO50AA`, `jo50aa`, `JO50`, `JO50AA12`, `JO50AA12AB`, `AA00AA`, `RR99XX`, `ZZ99`, `""`, `"JO5"`. Use every `PositionInRectangle` value.
  - Formula inputs: h ∈ {0, 10, 500, 2000} m; dist ∈ {0, 0.05, 1, 100, 400, 800} km; H ∈ {0, 5000, 12000} m; radius ∈ {6371, 6371×1.33}; eps ∈ {−0.01, 0, 0.005, 0.05} rad; qrg ∈ {0.144, 0.432, 1.296, 10.368} GHz.
- [ ] **Step 5:** In `Program.cs`, evaluate every public static function of `LatLon`, `MaidenheadLocator` (`Check`, `IsPrecise`, `Convert`, `LocFromLatLon` for precision 3–5 and both `smallLettersForSubsquares`, `LatFromLoc`, `LonFromLoc`, `BoundsFromLoc`, `GetSubGrids`) and `ScoutBase.Core.Propagation` on the cases. Catch exceptions and record them as `{"error": "<ExceptionTypeName>"}`. Write the three JSON files to `tests/golden/`.
- [ ] **Step 6:** Run `AIRSCOUT_UPSTREAM=$PWD/target/upstream/AirScout dotnet run --project tools/golden-harness -- tests/golden`. Expected: the three files exist, and each `cases` array is non-empty.
- [ ] **Step 7:** Create `docs/logic-findings.md` with this seed list, found while planning (port them as-is):
  1. `PropagationPath.cs` `GetInfoPoint`: `F1Radius(QRG, Distance, Distance - dist)`. The first distance argument should probably be `dist`.
  2. `PropagationPath.cs` `GetIntersectionPoint`: `(maxdistance > 0) && (qrb < maxdistance)` returns null. This looks inverted: planes *closer* than maxdistance get rejected.
  3. `PropagationDatabase.cs` `PropagationHorizonCreate`: `F1Radius(qrg, d, dist)` uses the full distance as the second leg instead of `dist - d`.
  4. `LocalObstruction.cs` constructor: when `height == null`, the loop resets `Distance[i]` instead of `Height[i]`.
  5. `ElevationDatabase.cs` `GetElevationTile`: when the cache is full, it removes the oldest entry but never adds the new tile.
  6. `PropagationDatabase.cs` `PropagationHorizonUpdate` SQL: `@StepWidth = @StepWidth` in the SET clause (it should be `StepWidth = @StepWidth`).
  7. `LatLon` uses a spherical earth (R = 6371 km) everywhere. Compare with WGS84 in Stage 6.
  8. `LocalObstruction.GetObstruction`: `(int)bearing` truncates. With a bearing of 359.6 it uses index 359, never rounds to 0, and divides by zero when the distance is 0.
- [ ] **Step 8:** Commit: `git add tools tests/golden docs/logic-findings.md && git commit -m "test: add C# golden harness and core math fixtures"`

### Task 2: Workspace + `as-geo::latlon`

**Files:**
- Create: `Cargo.toml` (workspace, `resolver = "2"`, shared `[workspace.dependencies]`)
- Create: `rust-toolchain.toml` (`channel = "stable"`, components rustfmt, clippy)
- Create: `.gitignore` (`/target`)
- Create: `crates/as-geo/Cargo.toml`, `crates/as-geo/src/lib.rs`, `crates/as-geo/src/latlon.rs`
- Create: `crates/as-golden/` (dev-only helper crate: `load(name) -> Golden`, `assert_close(a, b, tol)`. It also parses `"NaN"`/`"Infinity"` strings and treats NaN == NaN as equal)
- Test: `crates/as-geo/tests/latlon_golden.rs`
- Create: `.github/workflows/ci.yml` (3-OS matrix, see Global Constraints)

**Interfaces:**
- Produces in `as_geo::latlon`:
  - `pub const EARTH_RADIUS_KM: f64 = 6371.0;`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct GPoint { pub lat: f64, pub lon: f64 }`
  - `pub struct GRect { pub min_lat: f64, pub min_lon: f64, pub max_lat: f64, pub max_lon: f64 }`
  - `pub fn distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64` (km)
  - `pub fn bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64` (deg, 0..360)
  - `pub fn midpoint(lat1, lon1, lat2, lon2) -> GPoint`
  - `pub fn destination_point(lat: f64, lon: f64, bearing: f64, distance_km: f64) -> GPoint`
  - `pub fn intersection_point(lat1, lon1, brg1, lat2, lon2, brg2) -> Option<GPoint>` (C# `null` → `None`)
  - plus every other public static in `LatLon.cs`, named in snake_case.
  - `as_golden::{load, Golden, Case, assert_close}`.

- [ ] **Step 1:** Write `latlon_golden.rs`. It loads `latlon.json`, dispatches each case by `fn`, and asserts `assert_close(rust, golden, 1e-9)`. For `intersection_point`, a C# `null` must map to `None`.
- [ ] **Step 2:** Run `cargo test -p as-geo`. Expected: compile failure (functions missing).
- [ ] **Step 3:** Port `LatLon.cs` to `latlon.rs` with the same operation order (`x / 180 * PI`, not `to_radians()`, so the floating-point results match).
- [ ] **Step 4:** Run `cargo test -p as-geo`. Expected: PASS.
- [ ] **Step 5:** Add the CI workflow. Run `cargo fmt --check && cargo clippy --all-targets -- -D warnings` locally. Expected: clean.
- [ ] **Step 6:** Commit: `feat(as-geo): port LatLon with golden tests`.

### Task 3: `as-geo::maidenhead`

**Files:**
- Create: `crates/as-geo/src/maidenhead.rs`
- Test: `crates/as-geo/tests/maidenhead_golden.rs`

**Interfaces:**
- Consumes: `GPoint`, `GRect` from Task 2.
- Produces:
  - `pub enum PositionInRectangle { TopLeft, TopMiddle, TopRight, MiddleLeft, MiddleMiddle, MiddleRight, BottomLeft, BottomMiddle, BottomRight }` (variant set and order copied from `GeographicalPoint.cs:15`)
  - `pub fn check(loc: &str) -> bool`
  - `pub fn is_precise(lat: f64, lon: f64, precision: u8) -> bool`
  - `pub fn convert(loc: &str, small_letters_for_subsquares: bool) -> Result<String, LocatorError>`
  - `pub fn loc_from_lat_lon(lat: f64, lon: f64, small_letters: bool, precision: u8, autolength: bool) -> Result<String, LocatorError>`
  - `pub fn lat_from_loc(loc: &str, pos: PositionInRectangle) -> Result<f64, LocatorError>`, `lon_from_loc(...)`, `gpoint_from_loc(...) -> Result<GPoint, _>`, `bounds_from_loc(loc) -> Result<GRect, _>`
  - `pub enum SubGridsFilter` (copied), `pub fn get_sub_grids(loc, filter, small_letters, reverse) -> Result<Vec<String>, _>`
  - `#[derive(thiserror::Error, Debug)] pub enum LocatorError { Invalid(String), Precision(u8) }`. A golden `{"error": …}` case must map to `Err`.

- [ ] **Step 1:** Write `maidenhead_golden.rs` covering every `fn` in `maidenhead.json`. Strings must match exactly, floats within 1e-9, and errors must map to `Err`.
- [ ] **Step 2:** Run `cargo test -p as-geo maidenhead`. Expected: FAIL (missing).
- [ ] **Step 3:** Port `MaidenheadLocator.cs` (plus the `PositionInRectangle` offsets from `GeographicalPoint.cs`).
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(as-geo): port Maidenhead locator`.

### Task 4: `as-geo::propagation` formulas + `Band`

**Files:**
- Create: `crates/as-geo/src/propagation.rs`, `crates/as-geo/src/band.rs`
- Test: `crates/as-geo/tests/propagation_golden.rs`, `crates/as-geo/tests/band.rs`

**Interfaces:**
- Produces in `as_geo::propagation` (all pure `fn(...) -> f64`, argument order and units exactly as in `ScoutBase.Core/Propagation.cs`): `alpha_from_distance(dist, re)`, `slant_range_from_heights(h, dist, big_h, radius)`, `slant_range_from_heights_latlon(h, lat1, lon1, lat2, lon2, big_h, radius)`, `epsilon_from_heights(h, dist, big_h, radius)`, `theta_from_heights(...)`, `height_from_epsilon(h, dist, eps, radius)`, `distance_from_epsilon(h, big_h, eps, radius)`, `f1_radius(qrg_ghz, dist1, dist2)`, `visibility_distance(h1, h2, re)`, `doppler_shift(qrg_hz, speed)`.
- Produces in `as_geo::band`: `#[repr(i32)] pub enum Band { None = 0, B50M = 50, B70M = 70, B144M = 144, B432M = 432, B1_2G = 1296, B2_3G = 2320, B3_4G = 3400, B5_7G = 5760, B10G = 10368, B24G = 24048, B47G = 47088, B76G = 76032, All = 999_999_999 }`, `Band::name(&self) -> &'static str` (the strings from `StringCustomAttribute` in `Band.cs`), `Band::parse(&str) -> Option<Band>` (case-insensitive, `"NONE"`/`"ALL"`), `Band::all_except_none_and_all() -> &'static [Band]`, `Band::qrg_ghz(&self) -> f64` (= value / 1000).

- [ ] **Step 1:** Write `propagation_golden.rs` against `propagation_formulas.json` (tolerance 1e-9; NaN must equal NaN).
- [ ] **Step 2:** Write `band.rs` tests: `Band::parse("144m")` equals the `B144M` name round-trip for every band, `Band::parse("all") == Some(Band::All)`, `all_except_none_and_all().len() == 12`.
- [ ] **Step 3:** Run `cargo test -p as-geo`. Expected: FAIL.
- [ ] **Step 4:** Port `Propagation.cs` and `Band.cs`.
- [ ] **Step 5:** Run the tests. Expected: PASS.
- [ ] **Step 6:** Commit: `feat(as-geo): port propagation formulas and bands`.

### Task 5: `as-elevation` tile model + `.loc` parsing + `as-db`

**Files:**
- Create: `crates/as-db/src/lib.rs` (rusqlite `bundled`; `open(path) -> Result<Connection>`, `user_version` get/set, `migrate(conn, &[&str])` that applies statements in order and bumps `user_version`)
- Create: `crates/as-elevation/src/{lib.rs, model.rs, tile.rs, error.rs}`
- Create: `tests/fixtures/elevation/GLOBE/JO/JO50.zip`, `JO51.zip`, `JO60.zip`, `JO61.zip` (downloaded once from the data source; total ≈ 1.1 MB), and `tests/fixtures/elevation/SRTM3/JO/JO50AA.loc` (one tile extracted from `SRTM3/JO/JO50.zip`)
- Extend the harness: add case `elevation_tile` (load `.loc` JSON into `ElevationTileDesignator` via Newtonsoft, then call `GetElevation` on a 7×7 grid per tile, including points exactly on the tile edges)
- Test: `crates/as-elevation/tests/tile_golden.rs`

**Interfaces:**
- Produces:
  - `pub enum ElevationModel { None = 0, Globe = 1, Srtm3 = 2, Srtm1 = 3, Aster3 = 4, Aster1 = 5 }` with `default_step_width_m() -> f64` (SRTM1/ASTER1 30, SRTM3/ASTER3 90, others 1000), `average_period() -> usize` (ASTER1/ASTER3 5, others 0), `dir_name() -> &'static str` ("GLOBE", "SRTM3", …).
  - `pub const ELV_MISSING: i16 = -32768; pub const TILE_MISSING: i16 = -32767;`
  - `pub struct ElevationTile { pub index: String, pub min_elv: i32, pub min_lat: f64, pub min_lon: f64, pub max_elv: i32, pub max_lat: f64, pub max_lon: f64, pub rows: usize, pub columns: usize, pub elv: Vec<i16> /* column-major: elv[col * rows + row], matching C# Elv[col,row] */, pub last_updated: chrono::DateTime<Utc>, base_lat: f64, base_lon: f64, step_lat: f64, step_lon: f64, bounds: GRect }`
  - `ElevationTile::from_loc_json(&str) -> Result<ElevationTile, ElevationError>`. The JSON shape is the `.loc` file (`"Elv"` is `Columns` arrays of `Rows` values).
  - `ElevationTile::get_elevation(&self, lat: f64, lon: f64) -> i16`. It uses `i = trunc((lat-base_lat)/step_lat)` and `j = trunc((lon-base_lon)/step_lon)`, with `step_lat = 1/24/rows` and `step_lon = 2/24/columns`. Out of range returns `ELV_MISSING`, like the C# `catch`.
  - `ElevationTile::to_blob(&self) -> Vec<u8>` / `from_blob(...)`: little-endian i16, column-major, with our own format (no BinaryFormatter compatibility).

- [ ] **Step 1:** Regenerate goldens with the new harness case. Expected: `tests/golden/elevation_tile.json` exists.
- [ ] **Step 2:** Write `tile_golden.rs`: for every fixture tile and grid point, `tile.get_elevation(lat, lon) == golden` (exact). Also add `blob_roundtrip` (`from_blob(to_blob(t)) == t`) and `malformed_json_is_err` (truncated `.loc` → `Err(ElevationError::Parse(_))`).
- [ ] **Step 3:** Run `cargo test -p as-elevation`. Expected: FAIL.
- [ ] **Step 4:** Implement `as-db`, `model.rs`, `tile.rs`.
- [ ] **Step 5:** Run the tests. Expected: PASS.
- [ ] **Step 6:** Commit: `feat(as-elevation): tile model, .loc parsing, as-db`.

### Task 6: `as-elevation` store + cached lookup

**Files:**
- Create: `crates/as-elevation/src/store.rs`, `crates/as-elevation/src/cache.rs`
- Test: `crates/as-elevation/tests/store.rs`

**Interfaces:**
- Consumes: `ElevationTile`, `ElevationModel`, `as_db`.
- Produces:
  - `pub struct ElevationStore` (one SQLite file per model: `globe.db3`, `srtm3.db3`, `srtm1.db3`, `aster3.db3`, `aster1.db3` in the given directory; table `Elevation(TileIndex TEXT PK, MinElv, MinLat, MinLon, MaxElv, MaxLat, MaxLon, Rows, Columns, Elv BLOB, LastUpdated INTEGER)`).
  - `ElevationStore::open(dir: &Path) -> Result<Self>`
  - `insert_or_update_if_newer(&self, model, &ElevationTile) -> Result<()>`, `find(&self, model, index: &str) -> Result<Option<ElevationTile>>`, `count(&self, model) -> Result<u64>`
  - DB status bits per model: `pub struct DbStatus(u32)` with `COMPLETE` and `ERROR` bits (values from `ScoutBase.Database` `DATABASESTATUS`), `get_status/set_status_bit/reset_status_bit`.
  - `bounds: GRect` (C# `MinLat..MaxLon` of the database), settable.
  - `ElevationStore::elevation(&self, model, lat, lon, set_invalid_to_zero: bool) -> Result<i16, ElevationError>`. Bounds check: lat ∉ [−90,90] or lon ∉ [−180,180] → `Err(InvalidCoordinate)`. Tile lookup key = `loc_from_lat_lon(lat, lon, false, 3)`. Missing tile → `TILE_MISSING`. If `set_invalid_to_zero` and e ≤ `TILE_MISSING` → 0.
  - Tile cache: last-tile fast path plus an ordered cache with capacity from `CacheSetBounds`. **Reproduce finding #5** (when full, remove the oldest and do not insert), behind a `const LEGACY_CACHE_BEHAVIOUR: bool = true` with a comment referencing `logic-findings.md` #5.
  - `Send + Sync` (use `Mutex` around the connection and cache).

- [ ] **Step 1:** Write tests: `store_roundtrip` (insert a fixture tile, `find` returns an equal tile), `insert_older_is_ignored`, `elevation_matches_tile_golden` (the store answer equals the Task 5 golden for every point), `missing_tile_returns_tile_missing_or_zero` (both flag settings), `invalid_coordinate_is_err` (lat 90.0001).
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement `store.rs` and `cache.rs`.
- [ ] **Step 4:** Run them. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(as-elevation): SQLite tile store with cache`.

### Task 7: Elevation path, horizon, local obstruction

**Files:**
- Create: `crates/as-elevation/src/{path.rs, horizon.rs, obstruction.rs}`
- Extend the harness: case `elevation_path`. Copy the bodies of `ElevationPathCreateFromLatLon` and `MovingAverage` **verbatim** into `tools/golden-harness/UpstreamLoops.cs`, replacing only the DB access with an in-memory store loaded from the same fixture `.loc` files, and the `BackgroundWorker` with `null`. Record the copied line ranges in a header comment. Path inputs: the 6 station pairs from Task 1 that lie in JO50/JO51/JO60/JO61, plus distance 0, plus a path leaving the fixture area (missing tiles), each for step widths 1000 and 90, average periods 0 and 5, and complete = true/false.
- Test: `crates/as-elevation/tests/path_golden.rs`, `crates/as-elevation/tests/obstruction.rs`

**Interfaces:**
- Consumes: `ElevationStore::elevation`, `as_geo::latlon::{distance, bearing, destination_point}`.
- Produces:
  - `pub struct ElevationPath { pub lat1, lon1, lat2, lon2: f64, pub step_width_m: f64, pub count: usize, pub path: Vec<i16>, pub valid: bool, pub last_updated: DateTime<Utc> }` with `distance()`, `bearing12()`, `bearing21()`, `info_points() -> Result<Vec<i16>, ElevationError>` (`Err` when `step_width_m > 1000`).
  - `count = trunc(distance_km * 1000 / step_width_m) + 1`.
  - `ElevationStore::create_path(&self, model, lat1, lon1, lat2, lon2, step_width_m, cancel: Option<&AtomicBool>) -> Result<Option<ElevationPath>>` (`None` = cancelled). It reproduces the C# logic including the moving average (`average_period`), the start offset `-(avperiod/2) * step` (integer division), the tile-missing → validity rule using `bounds` and DB status.
  - `ElevationStore::find_or_create_path(...)` with an SQLite cache table `ElevationPath` (key: lat1, lon1, lat2, lon2, step_width).
  - `pub struct ElevationHorizon { lat, lon, distance_km, step_width_m, count, paths: [Vec<i16>; 360], valid }` and `pub struct LocalObstruction { lat, lon, distance: [f64; 360], height: [f64; 360] }` with `obstruction(my_height: f64, bearing: f64) -> f64` (atan((h − my)/dist) with `(int)bearing` truncation, findings #4 and #8 reproduced).

- [ ] **Step 1:** Regenerate goldens. Expected: `elevation_path.json` exists.
- [ ] **Step 2:** Write `path_golden.rs`: for each case, `count`, `path` (exact `Vec<i16>`) and `valid` equal the golden. Add `zero_distance_path_has_count_1` and `cancel_returns_none`.
- [ ] **Step 3:** Write `obstruction.rs`: `obstruction(10.0, 359.6)` uses index 359, and constructing with `height = None` leaves `height` all 0 (C# behaviour: `Height` is initialised by `this()`).
- [ ] **Step 4:** Run `cargo test -p as-elevation`. Expected: FAIL.
- [ ] **Step 5:** Implement `path.rs`, `horizon.rs`, `obstruction.rs`.
- [ ] **Step 6:** Run the tests. Expected: PASS.
- [ ] **Step 7:** Commit: `feat(as-elevation): elevation path, horizon, local obstruction`.

### Task 8: Elevation catalogue + downloader

**Files:**
- Create: `crates/as-elevation/src/{catalogue.rs, updater.rs}`
- Test: `crates/as-elevation/tests/updater.rs` (mockito server)

**Interfaces:**
- Consumes: `ElevationStore`, `ElevationTile::from_loc_json`, `as_geo::maidenhead`.
- Produces:
  - `pub const DEFAULT_BASE_URL: &str = "http://www.airscout.eu/downloads/ScoutBase/1/ElevationData";`
  - `pub struct Catalogue { pub base_url: String, pub files: BTreeMap<String, DateTime<Utc>>, pub last_modified: Option<DateTime<Utc>> }`
  - `Catalogue::fetch(client: &reqwest::blocking::Client, base_url: &str, model, rect: GRect, cache_dir: &Path) -> Result<Catalogue>`. It downloads `{base}/{MODEL}/files.zip` (only if newer: `If-Modified-Since`), unzips `files.cat`, and parses lines `"<LOC6>;yyyy-MM-dd HH:mm:ssZ"`, skipping empty lines and lines starting with `/`. It keeps only entries whose 4-char square lies in `rect`. The squares come from `get_locs_from_rect(rect, precision 2)`, which is ported from `ElevationDatabase.GetLocsFromRect`.
  - `pub fn update_square(client, base_url, model, square4: &str, store: &ElevationStore, cancel: Option<&AtomicBool>, progress: &dyn Fn(UpdateProgress)) -> Result<usize /* tiles written */>`. It downloads `{base}/{MODEL}/{AB}/{ABCD}.zip`, parses every `*.loc` and inserts each with `insert_or_update_if_newer` inside **one transaction** (all or nothing). It extracts in memory and never writes temp `.loc` files.
  - `pub enum UpdateProgress { Catalogue { done: usize, total: usize }, Square { square: String, tiles: usize } }`
  - Set `User-Agent: AirScout-RS/<version>`.

- [ ] **Step 1:** Write tests with mockito: `catalogue_filters_by_rect`, `update_square_writes_all_tiles` (served fixture `JO50.zip` → `store.count(Globe) == 577`), `http_404_is_err_and_store_unchanged`, `truncated_zip_is_err_and_store_unchanged`, `malformed_loc_rolls_back_whole_square`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them. Expected: PASS.
- [ ] **Step 5:** Add the `#[ignore]` test `live_download_globe_jo50` against the real server. Run it once with `cargo test -p as-elevation -- --ignored live_`. Expected: PASS (577 tiles).
- [ ] **Step 6:** Commit: `feat(as-elevation): catalogue and tile downloader`.

### Task 9: `as-propagation` path + intersection

**Files:**
- Create: `crates/as-propagation/src/{lib.rs, path.rs, point.rs}`
- Extend the harness: case `propagation_path`. Copy `PropagationPathCreateFromLatLon` verbatim into `UpstreamLoops.cs` (same rules as Task 7) and link `PropagationPath.cs` math (`GetInfoPoints`, `GetInfoPoint`, `GetIntersectionPoint`) via a copied class without the SQLite base. Inputs: the Task 7 paths × h1,h2 ∈ {10, 50} m × qrg ∈ {0.144, 1.296, 10.368} × k ∈ {1.33} × f1_clearance ∈ {0, 0.6} × near-field suppression ∈ {0, 500} m × local obstruction ∈ {−∞ (double.MinValue), 0.01}. Intersection probes: 12 plane positions/bearings around each path, maxdistance ∈ {−1, 0, 50}.
- Test: `crates/as-propagation/tests/path_golden.rs`

**Interfaces:**
- Consumes: `ElevationStore::find_or_create_path`, `as_geo::propagation::*`, `as_geo::latlon::*`.
- Produces:
  - `pub struct PropagationPoint { pub lat: f64, pub lon: f64, pub h1: f64, pub h2: f64, pub f1: f64 }`
  - `pub struct IntersectionPoint { pub lat: f64, pub lon: f64, pub qrb: f64, pub min_h: f64, pub dist1: f64, pub dist2: f64 }`
  - `pub struct PathParams { pub lat1: f64, pub lon1: f64, pub h1: f64, pub lat2: f64, pub lon2: f64, pub h2: f64, pub qrg_ghz: f64, pub radius_km: f64, pub f1_clearance: f64, pub step_width_m: f64, pub local_obstruction: f64 }`
  - `pub struct PropagationPath { pub params: PathParams, pub eps1_min: f64, pub eps2_min: f64, pub valid: bool, pub local_obstructed: bool, pub last_updated: DateTime<Utc> }` with `distance()`, `bearing12()`, `bearing21()`, `info_points() -> Option<Vec<PropagationPoint>>` (length `trunc(distance)+2`), `info_point(dist_km) -> Option<PropagationPoint>`, `mid_point() -> GPoint`, `intersection_point(lat, lon, bearing, max_distance) -> Option<IntersectionPoint>` (findings #1 and #2 reproduced).
  - `pub struct PropagationEngine { elevation: Arc<ElevationStore>, conn: Mutex<Connection>, pub near_field_suppression_m: f64 }`
  - `PropagationEngine::create_path(&self, model, p: &PathParams, cancel) -> Result<Option<PropagationPath>>` and `find_or_create_path(...)` with the SQLite cache table `PropagationPath` (key columns as in C#).

- [ ] **Step 1:** Regenerate goldens. Expected: `propagation_path.json` exists.
- [ ] **Step 2:** Write `path_golden.rs`: `eps1_min`, `eps2_min` within 1e-12, `valid` and `local_obstructed` equal, every `info_points()` field within 1e-9, intersection `None`/`Some` equal and fields within 1e-9.
- [ ] **Step 3:** Run it. Expected: FAIL.
- [ ] **Step 4:** Implement.
- [ ] **Step 5:** Run it. Expected: PASS.
- [ ] **Step 6:** Commit: `feat(as-propagation): propagation path and intersection`.

### Task 10: `as-propagation` horizon

**Files:**
- Create: `crates/as-propagation/src/horizon.rs`
- Extend the harness: case `propagation_horizon`. Copy `PropagationHorizonCreate` and `EpsilonMaxFromPath` verbatim (same rules). Inputs: 2 station positions in the fixture area, dist ∈ {50, 200} km, step 1000 m, qrg 0.144 and 10.368, f1 0.6, near-field 0 and 500.
- Test: `crates/as-propagation/tests/horizon_golden.rs`

**Interfaces:**
- Consumes: `ElevationStore::find_or_create_path` (bearing variant: `create_path_from_bearing(model, lat, lon, bearing_deg, dist_km, step, cancel)`, added to `as-elevation` in this task), `LocalObstruction`.
- Produces:
  - `pub struct HorizonPoint { pub dist: f64, pub eps_min: f64, pub elv: i16 }` (defaults `f64::MIN`, `f64::MIN`, `i16::MIN`, like C#)
  - `pub struct PropagationHorizon { lat, lon, h, dist, step_width_m, qrg_ghz, f1_clearance, radius_km, pub horizon: [HorizonPoint; 360], pub valid: bool, pub local_obstruction: Option<LocalObstruction> }`
  - `PropagationEngine::create_horizon(&self, model, lat, lon, h, dist, qrg, radius, f1, step, obstruction: Option<&LocalObstruction>, cancel, progress: &dyn Fn(usize /*azimuth*/, &HorizonPoint)) -> Result<Option<PropagationHorizon>>` and `find_or_create_horizon(...)`. It uses rayon over the 360 azimuths, and the result order must still be by azimuth.
  - `pub fn epsilon_max_from_path(h1, elv: &mut Vec<i16>, dist, step_width_m, freq, f1_clearance, max_elv, re) -> HorizonPoint`
  - Store `Horizon` BLOB with our own format. **Do not reproduce** finding #6 (the SQL typo). It is a DB-layer bug with no effect on computed results. Note this in `logic-findings.md` as "not reproduced, reason".

- [ ] **Step 1:** Regenerate goldens. Expected: `propagation_horizon.json` exists.
- [ ] **Step 2:** Write `horizon_golden.rs` (all 360 points: dist and eps_min within 1e-12, elv exact; `epsilon_max_from_path` result and the truncated `elv` length equal).
- [ ] **Step 3:** Run it. Expected: FAIL.
- [ ] **Step 4:** Implement.
- [ ] **Step 5:** Run it. Expected: PASS.
- [ ] **Step 6:** Commit: `feat(as-propagation): horizon`.

### Task 11: `as-stations`

**Files:**
- Create: `crates/as-stations/src/{lib.rs, location.rs, qrv.rs, callsign.rs, updater.rs}`
- Test: `crates/as-stations/tests/{db.rs, callsign.rs, updater.rs}`

**Interfaces:**
- Consumes: `as_db`, `as_geo::{maidenhead, band::Band}`.
- Produces:
  - `pub enum GeoSource` (values copied from `ScoutBase.Stations`/`ScoutBase.Core` `GEOSOURCE`)
  - `pub struct Location { pub call: String, pub loc: String, pub lat: f64, pub lon: f64, pub source: GeoSource, pub hits: i32, pub last_updated: DateTime<Utc> }`
  - `pub struct Qrv { pub call: String, pub loc: String, pub band: Band, pub antenna_height: f64, pub antenna_gain: f64, pub power: f64, pub last_updated: DateTime<Utc> }`
  - `pub struct StationDb` with tables `Location(Call, Loc, Lat, Lon, Source, Hits, LastUpdated, PK(Call,Loc))`, `QRV(Call, Loc, Band, AntennaHeight, AntennaGain, Power, LastUpdated, PK(Call,Loc,Band))`. Methods: `open`, `location_insert_or_update_if_newer`, `location_find(call, loc: Option<&str>)` (with `loc = None`: the entry with the highest `hits`, as in C# `LocationFind(call)`; verify in `StationDatabase.cs`), `locations_by_call`, `qrv_insert_or_update_if_newer`, `qrv_find(call, loc, band)`, `qrv_get_all(call, loc)`.
  - `callsign.rs`: port the pure functions of `ScoutBase.Core/Callsign.cs` and `WCCheck.cs` (check, base call, prefix extraction) with tests from the doc examples in those files.
  - `updater.rs`: port `StationDatabaseUpdater` download/parse. The URL comes from the original (`grep -n http ScoutBase/ScoutBase.Stations/StationDatabaseUpdater.cs`). Verify it is reachable and record the status in `logic-findings.md` ("data sources" section).

- [ ] **Step 1:** Write tests: insert/find roundtrip, `insert_older_is_ignored`, `qrv_per_band`, callsign cases (`DL2ALF` valid, `DL2ALF/P` base `DL2ALF`, `""` invalid, `PA/DL2ALF/M` base `DL2ALF`), `updater_parses_fixture` (mockito with a small recorded sample).
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(as-stations): station and QRV database`.

### Task 12: `as-settings`

**Files:**
- Create: `crates/as-settings/src/{lib.rs, band_settings.rs}`
- Test: `crates/as-settings/tests/settings.rs`

**Interfaces:**
- Consumes: `as_geo::band::Band`, `as_elevation::ElevationModel`.
- Produces:
  - `pub struct BandSettings { pub k_factor: f64, pub f1_clearance: f64, pub max_distance: f64, pub max_elevation: f64, pub min_altitude: f64, pub max_altitude: f64, ... }`. Fields and **default values copied from `AirScout.Core/BandSettings.cs`** (every field, snake_case).
  - `pub struct Settings { pub my_call: String, pub my_loc: String, pub my_lat: f64, pub my_lon: f64, pub my_height: f64, pub dx_call: String, pub dx_loc: String, pub band: Band, pub elevation_model: ElevationModel, pub path_near_field_suppression_m: f64, pub band_settings: BTreeMap<Band, BandSettings>, ... }`. Stage 1 only needs the fields the core uses. Defaults come from `AirScout/Properties/Settings.settings` (`DefaultSettingValue`). Later stages add fields with `#[serde(default)]`.
  - `Settings::load_or_default(path: &Path) -> Result<Settings>`, `Settings::save(&self, path) -> Result<()>` (atomic: write to temp, then rename), `Settings::default_path() -> PathBuf` (via `directories::ProjectDirs::from("eu", "airscout", "AirScout-RS")` + `settings.toml`).
  - `Settings::radius_km(&self) -> f64` = `6371.0 * band_settings[band].k_factor`.

- [ ] **Step 1:** Write tests: `defaults_match_original` (spot-check `k_factor` and `f1_clearance` per band against the values in `BandSettings.cs`), `roundtrip_toml`, `unknown_fields_ignored`, `missing_file_gives_default`, `corrupt_file_is_err_not_overwritten`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run them. Expected: PASS.
- [ ] **Step 5:** Commit: `feat(as-settings): settings and band settings`.

### Task 13: Stage-1 CLI smoke binary + CI green on 3 OS

**Files:**
- Create: `crates/as-cli/src/main.rs` (bin `airscout-cli`, clap)
- Test: `crates/as-cli/tests/cli.rs` (assert_cmd)

**Interfaces:**
- Consumes: everything above.
- Produces: `airscout-cli path --from JO50AA --to JO61UF --band 144 --h1 10 --h2 10 --model globe --data <dir> [--download]`. It prints JSON: `{distance_km, bearing12, bearing21, eps1_min_deg, eps2_min_deg, valid, info_points:[...]}`. The command `airscout-cli horizon --loc JO50AA --h 10 --dist 100 ...` prints 360 points.

- [ ] **Step 1:** Write `cli.rs`: run `path` against the fixture data dir (pre-loaded from the fixture zips via a test helper). Assert exit code 0, `valid == true`, and `distance_km` equals `as_geo::latlon::distance` of the locator centres within 1e-9.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test --workspace`. Expected: PASS. If a GitHub remote exists (ask the user before creating one), push the branch and check that CI is green on all 3 OS. Otherwise run the workflow locally on this OS only and report that the other two are unverified.
- [ ] **Step 5:** Commit: `feat(as-cli): stage-1 smoke CLI`.
