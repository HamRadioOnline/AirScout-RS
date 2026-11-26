# Stage 5b — Elevation Tools (`elevation-tile-gen`, `airscout-cli elevation-coverage`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stage 5b of the spec, "Suite tools", as split in the survey: the ElevationTileGenerator as the separate binary `elevation-tile-gen` (owner decision Q6) and the ElevationCoverageMapper as the `airscout-cli elevation-coverage` subcommand (Q7). The generator builds, from local SRTM and GLOBE files, the same `.loc` / `{AB}/{ABCD}.zip` / `files.cat` / `files.zip` tree that airscout.eu serves, so users can host their own.

**Architecture:**
- **`.loc` writer in `as-elevation`** (`crates/as-elevation/src/loc_json.rs`): `ElevationTile::to_loc_json`, the inverse of `from_loc_json`, in the byte layout of the served tiles: Newtonsoft indented, `\r\n`, .NET Framework `"R"` numbers, no `Bounds` member. The `files.cat` parser moves out of `Catalogue::fetch` into `read_files_cat` / `read_files_zip`, so the coverage tool reads local catalogues with the app's parser.
- **`elevation-tile-gen`** (new crate `crates/elevation-tile-gen`, a library plus the binary): `tiles` ports `ProcessTile` 1:1 (tile content, names, void rules); `zip`, `catalogue`, `compare`, `downsample` and `preview` port the other buttons with a safe tool layer. The library lets the golden and round-trip tests call the tile functions directly.
- **Golden vectors** (`tools/golden-harness/UpstreamTileGen.cs`, `TileGenCases.cs`): verbatim copies of the WinForms methods run on synthetic sources in the docker harness. They record per tile the C# member list, the numbers (doubles as IEEE bits) and an FNV-1a digest of `Elv`.
- **`airscout-cli elevation-coverage`** (`crates/as-cli/src/elevation_coverage.rs`): reads a local `files.zip`/`files.cat`, or downloads the one catalogue only with `--download`. It counts the tiles, optionally against the local store, and optionally writes GeoJSON.

**Tech Stack:** Rust 1.95+, clap 4, serde_json, chrono, zip 2 (deflate), png 0.18 (already in `Cargo.lock` through eframe; new direct edge in Task 5), reqwest blocking (coverage download, and the round-trip test against mockito), assert_cmd 2, tempfile, mockito (dev). Golden harness: .NET 8 SDK in docker `mcr.microsoft.com/dotnet/sdk:8.0` (no dotnet on the host).

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`:
- stage 5 "Suite tools" (`:113`);
- the binary rows `maptile-gen`, `elevation-tile-gen` (bins) (`:85`) and the `airscout-cli` subcommands row with `elevation-coverage` (`:87`), already changed by owner decision Q6 in 5a;
- "The Rust port must not depend solely on airscout.eu: support raw public sources (GLOBE, SRTM …) as fallback" (`:48-49`), and "If airscout.eu is gone, Rust reads raw GLOBE + SRTM3/SRTM1" (`:138-140`).

**Inputs:**
- Survey `docs/superpowers/surveys/2026-11-24-suite-tools-survey.md`: §5.1, §5.2, §10, §11 (S11 to S16), §12 (5b) and "Decisions (project owner, 2026-11-24)" (Q6, Q7). Corrections to the survey are in the closing section; the plan follows the corrected facts.
- Ledger `docs/superpowers/ledgers/2026-11-24-stage5a-dev-tools.md`: pre-flight C1 (a test that passes trivially), C5 (the hamlib gate with `-p catcheck` and `cargo clippy -p as-cat` alone), C6 (plan-mandated tests that re-implement the code under test), and the `GetLocsFromRect` gap (#289).
- Upstream `target/upstream/AirScout/` @ `91b02cb`, read-only. C# references are relative to it and start with the project directory (`ElevationTileGenerator/`, `ElevationCoverageMapper/`, `ScoutBase/…`, `SQLiteDatabase/…`).
- Served tiles already in the repo: `tests/fixtures/elevation/GLOBE/JO/JO50.zip` … `JO61.zip` and `tests/fixtures/elevation/SRTM3/JO/JO50AA.loc` (written by the C# tool on .NET Framework).

**Stage split:**
- **5b (this plan):** `elevation-tile-gen` (`tiles`, `zip`, `catalogue`, `compare`, `downsample`, `preview`), `airscout-cli elevation-coverage`, README, spec check and findings.
- **Not in 5b:** `as-maps`, `maptile-gen` and the mirror preloader (5c), `airscout-planeserver` (5d), Copernicus GLO-30 input (a later input format, survey §5.2), ASTER tiles (the C# tool cannot make them either).

## Global Constraints

Everything in the 5a Global Constraints still applies where it concerns the touched crates. Changes and additions:

- **Port policy:**
  - Tile content is ported 1:1: sample positions, void rules, min/max, tile names, which tiles are written. It is what the app's prediction reads. Every defect is logged in `docs/logic-findings.md` ("Ported 1:1"). Logic fixes are stage 6.
  - Tool-layer bugs (dialogs, file handling, error handling, statistics, timestamps) are not reproduced. Each gets a finding: "Not reproduced", "Port deviation" or "New design".
  - The next finding number is **#291**; check the last row first. Task 7 assigns the numbers in its list order.
  - Every C# `file:line` in code comments and findings is checked against `target/upstream`.
- **Owner decisions (binding):** Q6: `elevation-tile-gen` is a separate binary; `elevation-coverage` is an `airscout-cli` subcommand. Q7: `elevation-coverage` comes in 5b.
- **Network:**
  - `elevation-tile-gen` makes no network request. It reads local files only.
  - `elevation-coverage` makes at most one HTTP GET per run (`<base>/<MODEL>/files.zip`), and only with `--download`. Without `--download` and without `--catalogue` it is an error before any socket. The request uses `as_net::download_if_newer`, which sends `as_net::user_agent()`. No other User-Agent anywhere.
  - Never write the upstream author's private LAN address anywhere. Examples use `localhost` or `127.0.0.1`.
- **Data:**
  - No DEM data is committed. Tests and the harness build tiny synthetic sources in a temp dir (`synth` below). The golden file holds digests and numbers only.
  - The served fixture tiles already in `tests/fixtures/elevation` may be read; no new tile file is added.
  - No real call sign, position or e-mail in tests.
- **Synthetic source (shared by harness and Rust tests):** sample `(r, c)` (row from the top of the file, column from the west):
  - in a `voids` rectangle `[r0, r1, c0, c1, v]` (half-open), the value `v`;
  - else in a `fill` rectangle `[r0, r1, c0, c1]`, `synth(r, c) = ((r * 31 + c * 17) % 5200) - 700` (as `i32`, then `i16`);
  - else 0.
  - SRTM files are big-endian, GLOBE files little-endian. The `source` cases of `tests/golden/elevation_tilegen.json` carry the rectangles; Rust tests read them from there.
- **Untrusted input:** source files, `.loc` files, zips and catalogues never panic. A bad file is an error naming the file, never a partial silent write. No allocation follows a size read from a file except the source file itself (at most 6000 × 10800 × 2 bytes, the C# whole-file read).
- **Files:** every file the tools write goes to `<target>.part` first and is then renamed, so an interrupted run leaves no truncated `.loc`, zip or catalogue. No tool deletes a file except `zip --remove-loc` after verifying the zip (Task 4).
- **Platform:** macOS, Linux and Windows; no `cfg` code in the new crate. `#![forbid(unsafe_code)]` in `elevation-tile-gen` and `as-cli`.
- **Cargo:**
  - Every cargo command carries `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
  - Disk: total under 120 GB. Check `du -sh target` before each task. Above 40 GB run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clean --profile dev`; add `--target-dir target/linux` or `--target-dir target/wingnu` for the docker dirs. **Never** run plain `cargo clean`: it deletes `target/upstream`.
  - Tasks 3 and 5 change dependency edges. Each runs one `cargo build` without `--locked` and commits `Cargo.lock`. Check: `git diff Cargo.lock | grep '^+name'` prints only `+name = "elevation-tile-gen"` in Task 3 and nothing in Task 5.
  - New entries: workspace `png = "0.18"` (Task 5); `crates/elevation-tile-gen` (workspace member through `crates/*`).
- **Gate:**
  - macOS: `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo fmt --all --check && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy --workspace --all-targets --locked -- -D warnings && CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace --locked`.
  - hamlib feature check (5a C5): `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings`.
  - `as-cat` alone (without the `settings` feature, 5a C5): `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy -p as-cat --all-targets --locked -- -D warnings`.
  - Linux docker: `docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target/linux -e CARGO_BUILD_JOBS=2 -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 rust:1.95 bash -c 'apt-get update && apt-get install -y libasound2-dev pkg-config && rustup component add rustfmt clippy && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked && cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings && cargo clippy -p as-cat --all-targets --locked -- -D warnings'`.
  - Windows check: `docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target/wingnu -e CARGO_BUILD_JOBS=2 -e CARGO_INCREMENTAL=0 -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 rust:1.95 bash -c 'apt-get update && apt-get install -y gcc-mingw-w64-x86-64 && rustup target add x86_64-pc-windows-gnu && rustup component add clippy && cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings && cargo build --workspace --all-targets --locked --target x86_64-pc-windows-gnu'`. Tasks 3, 5 and 7 run it. `target/wingnu` counts toward the disk budget.
  - The hosted Windows CI job runs after the owner-approved push; the stage is not done until it is green.
- **Tests:** no network beyond 127.0.0.1 (mockito). Waits bounded at 10 s. Big synthetic files only in a tempdir; a GLOBE file is made sparse with `File::set_len` and only the filled rows are written.
- **File size:** new files stay under about 500 lines. `crates/as-cli/src/main.rs` (613) grows only by the `mod` line, the `Cmd` variant and its match arm (≤ 620).
- **Docs and commits:**
  - README and findings use plain English, short sentences and one idea per sentence (`docs/logic-findings.md` table format: `| # | Location | What looks wrong | What the port does |`).
  - Commits are signed: `export SSH_AUTH_SOCK=~/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock; git commit -S -m "…"`.
  - Linear history: no merge commits; rebase onto `main` if it moved.

## Review Focus

1. **A file that is not a source** (`N5.hgt`, `N50E011X.hgt` of SRTM3 size, `x.hgt`, a 100-byte `N50E011.hgt`, `C10G.bin`, `ABC`, `C1XG`, a directory). Expected: no panic (the C# `IsGLOBEFile` indexes before its length check, S12). Each is skipped with `not an SRTM1, SRTM3 or GLOBE file: <name>` on stderr; a run with no source exits 1 with `no source file`. Tests: Task 3 `identify_accepts_and_rejects`, `no_source_is_error`.
2. **Sources of two models in one run** (an SRTM3 and an SRTM1 file, or SRTM3 and GLOBE) into one `--out`. Expected: exit 1 with `inputs mix SRTM3 and SRTM1: run once per model with its own --out`, before any `.loc` is written. A served tree holds one model per directory. Test: Task 3 `mixed_models_rejected`.
3. **A second run into an existing tree, or an interrupted one.** Expected: existing `.loc` files are kept byte for byte (C# `File.Exists` skip) unless `--overwrite`; no `.part` file stays behind. `zip --remove-loc` deletes no `.loc` when a tile of the square does not parse, and the old zip stays. Tests: Task 3 `existing_tiles_kept_unless_overwrite`; Task 4 `zip_bad_tile_removes_nothing`.
4. **A catalogue with junk** (a line without `;`, a bad date, a lower-case name, a name that is not a locator, a duplicate, non-ASCII). Expected: such lines are skipped or counted as `invalid`, never a panic. `elevation-coverage` never opens a socket without `--download` (mockito `expect(0)`). Tests: Task 1 `files_cat_skips_junk`; Task 6 `needs_catalogue_or_download`, `counts_found_and_missing_against_store`.
5. **A self-hosted tree read by the app's updater.** Expected: `Catalogue::fetch` and `update_square` against the generated tree served over HTTP store every generated tile, and `ElevationStore::elevation` gives the generator's values, voids (`-32768`) included. A square the generator skipped stays missing (`-32767`). Test: Task 4 `self_hosted_tree_feeds_the_updater`.

---

### Task 1: `as-elevation`: `.loc` writer, catalogue reader, palette lookup

**Files:**
- Create: `crates/as-elevation/src/loc_json.rs`
- Modify: `crates/as-elevation/src/lib.rs` (module, re-exports), `crates/as-elevation/src/catalogue.rs:58-107` (`fetch` uses the new readers), `crates/as-elevation/src/palette.rs:39-50`
- Test: `crates/as-elevation/tests/loc_writer.rs`, `crates/as-elevation/tests/common/mod.rs` (add `fixture_texts`)

**Interfaces (`ScoutBase/ScoutBase.Elevation/ElevationTile.cs:56-124`; `SQLiteDatabase/SQLiteDatabase/SQLiteEntry.cs:64`, `:190-199`; `ScoutBase/ScoutBase.Elevation/ElevationCatalogue.cs:45`; `ScoutBase/ScoutBase.Elevation/ElevationDatabase.cs:2079-2090`):**
- Consumes: `ElevationTile` (pub fields `index`, `min_elv`, `min_lat`, `min_lon`, `max_elv`, `max_lat`, `max_lon`, `rows`, `columns`, `elv` column-major, `last_updated`), `ElevationTile::from_loc_json`, `ElevationError::{Corrupt, Io}`.
- Produces (`loc_json.rs`):
  - `pub fn framework_number_text(x: f64) -> String`: .NET Framework `double.ToString("R", Invariant)` plus Newtonsoft `EnsureDecimalPlace`. Take 15 significant digits (`format!("{:.14e}")`); if that text does not parse back to `x`, take 17 (`{:.16e}`). Strip trailing mantissa zeros. With `e` the decimal exponent of the first digit and `p` the digit count used (15 or 17): plain decimal when `-5 < e < p`, else `d[.ddd]E+XX` / `E-XX` (at least two exponent digits). Append `.0` when the text has no `.` and no `E`. `-0.0` gives `0.0` (Framework drops the sign). Only finite input occurs (tile coordinates).
  - `pub fn loc_date_text(t: &DateTime<Utc>) -> String`: `yyyy-MM-ddTHH:mm:ss`, then `.` and up to 7 fraction digits (100 ns ticks, truncated) without trailing zeros (none when zero), then `Z`.
  - `impl ElevationTile { pub fn to_loc_json(&self) -> String }`, the served byte layout: `{`, then the members `TileIndex`, `MinElv`, `MinLat`, `MinLon`, `MaxElv`, `MaxLat`, `MaxLon`, `Rows`, `Columns`, `Elv`, `LastUpdated` at two-space indent as `"Name": value`. `Elv` is one array per column, each element on its own line (four and six spaces). Every line ends with `\r\n`, and so does the closing `}` (C# `StreamWriter.WriteLine`, `ElevationTileGenerator/MainDlg.cs:285`). No BOM, no `Bounds` member (finding 5).
- Produces (`catalogue.rs`):
  - `pub fn read_files_cat(r: impl BufRead) -> Result<BTreeMap<String, DateTime<Utc>>, ElevationError>`: the loop of today's `fetch` (`:81-97`) without the square filter. Key: the name without `.loc`, upper-cased. First occurrence wins; malformed lines are skipped.
  - `pub fn read_files_zip(path: &Path) -> Result<BTreeMap<String, DateTime<Utc>>, ElevationError>`: entry `files.cat` of the zip, else `Corrupt("{path}: …")`.
  - `Catalogue::fetch` keeps its signature and behaviour: download, `read_files_zip`, then keep keys whose `get(..4)` is in the `get_locs_from_rect(rect, 2)` set.
- Produces (`palette.rs`): `pub fn palette_color(percentage: f64) -> [u8; 3]` = C# `GetColor` (`:2079-2090`): `i = (percentage / 100.0 * 1000.0) as i32`; outside `0..=1000` (and NaN) black `[0, 0, 0]`, else `colour_from_base(i / 1000 * 100)`. `elevation_color(value)` becomes `palette_color` of its clamped `value / 100` (no behaviour change).
- `lib.rs` re-exports `framework_number_text`, `loc_date_text`, `read_files_cat`, `read_files_zip`, `palette_color`.

- [ ] **Step 1: Write the failing tests** in `loc_writer.rs` (`common::fixture_texts() -> Vec<(String, String)>`: every `.loc` text in the fixture zips and the loose `.loc`, as raw strings):
  - `served_tiles_round_trip_byte_exact`: for every fixture text, `ElevationTile::from_loc_json(&text)?.to_loc_json() == text`. Assert that the count is at least 2000 (four GLOBE squares and one SRTM3 tile), so an empty loop cannot pass.
  - `number_text_framework_r`: `10.008333333333333` → `"10.008333333333333"`; `50.025` → `"50.025"`; `10.0` → `"10.0"`; `0.0` and `-0.0` → `"0.0"`; `-12.5` → `"-12.5"`; `0.1 + 0.2` → `"0.30000000000000004"`; `1.0 / 3.0` → `"0.33333333333333331"`; `1e-6` → `"1E-06"`; `1e15` → `"1E+15"`; `0.0001` → `"0.0001"`.
  - `date_text_trims_fraction`: `2018-01-24T20:37:30.7992685Z` round-trips; `2026-11-24T12:00:00Z` → `"2026-11-24T12:00:00Z"`; `…T12:00:00.5Z` → `"2026-11-24T12:00:00.5Z"`; 123 456 789 ns → `".1234567Z"`.
  - `files_cat_skips_junk` (Review Focus 4): text `"// h\r\n\r\nJO50AA.loc;2018-02-27 08:26:32Z\r\njo50ab.loc;2018-02-27 08:26:33Z\r\nbroken line\r\nJO50AC.loc;not a date\r\nJO50AA.loc;2019-01-01 00:00:00Z\r\nÄÖ.loc;2018-02-27 08:26:34Z\r\n"`. The map is exactly `{"JO50AA": 2018-02-27 08:26:32Z, "JO50AB": 2018-02-27 08:26:33Z, "ÄÖ": …}`. Junk is not validated here; the coverage tool counts non-locators.
  - `files_zip_without_cat_is_corrupt`: a zip with only `x.txt` gives `Err(ElevationError::Corrupt(_))`.
  - `palette_color_lookup`: `palette_color(0.0) == [0, 97, 71]`; `palette_color(100.0) == [255, 255, 255]`; `palette_color(50.0) == [126, 75, 75]`; `palette_color(-1.0) == [0, 0, 0]`; `palette_color(100.5) == [0, 0, 0]`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-elevation --test loc_writer`. Expected: FAIL (functions not defined).
- [ ] **Step 3: Implement** `loc_json.rs`, the reader split and `palette_color`. Write `to_loc_json` by hand with `format!`/`push_str`: the shape is fixed, and no serde formatter writes `\r\n`.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-elevation -p airscout`. Expected: PASS, including the existing `updater` and `coverage` tests (unchanged `fetch` and `elevation_color`). If `served_tiles_round_trip_byte_exact` fails on numbers only, report the first differing value: it decides the number rule and must not be skipped.
- [ ] **Step 5: Commit** `feat(as-elevation): .loc writer in the served layout; files.cat reader; palette lookup`.

### Task 2: Golden vectors from the C# tile generator

**Files:**
- Create: `tools/golden-harness/UpstreamTileGen.cs`, `tools/golden-harness/TileGenCases.cs`, `tests/golden/elevation_tilegen.json` (generated)
- Modify: `tools/golden-harness/Program.cs:35` (one `Write` line), `tests/golden/README.md` (one line)

**Interfaces (`ElevationTileGenerator/MainDlg.cs:72-196`, `:199-522`, `:1092-1150`, `:1252-1301`; `ScoutBase/ScoutBase.Elevation/ElevationTile.cs:32-124`; `SQLiteDatabase/SQLiteDatabase/SQLiteEntry.cs:190-199`):**
- `UpstreamTileGen.cs` starts with the header style of `UpstreamTile.cs:1-5`: VERBATIM COPY @ 91b02cb7, the source lines, every edit listed, "Do not fix anything here". It holds:
  - `UpstreamSQLiteEntry` with only `LastUpdated` (`SQLiteEntry.cs:64`) and `ToJSON` (`:190-199`, settings verbatim);
  - `UpstreamTileDesignator : UpstreamSQLiteEntry`: `ElevationTile.cs:39-124` verbatim, including the public `Bounds` field and the constructor defaults (`MinElv = short.MaxValue`, `MaxElv = short.MinValue`), without the `DataRow`/`IDataRecord` constructors and `ToBitmap`;
  - `IsGLOBEFile`, `IsSRTM3File`, `IsSRTM1File` (`MainDlg.cs:72-196`), verbatim;
  - `ProcessTile(string tilename, string destDir)` (`:199-522`). Edits: no `tsl_Status`, `ss_Main`, `Application.DoEvents` or `Stopwatch` lines; `Properties.Settings.Default.DestDir` → `destDir`; `DateTime.UtcNow` → `FakeTileClock.UtcNow`; `ElevationData.Database.ElvMissingFlag` → `(short)-32768`; `ElevationTileDesignator` → `UpstreamTileDesignator`. The GLOBE loops over `i0`, `j0`, `i1`, `j1` run only over the one value given by the parameters `oi0, oj0, oi1, oj1`; `i2`, `j2` and the body stay verbatim. A full GLOBE file is 1.15 million tiles.
  - `GetSRTM1Elevation`, `ConvertSRTM1ToSRTM3File` (`:1252-1301`), and `CompareFilesHGT` (`:1092-1150`) with the `MessageBox`/`Say` lines dropped and the output path as a parameter.
  - `FakeTileClock.UtcNow = new DateTime(2026, 11, 24, 12, 0, 0, DateTimeKind.Utc).AddTicks(1234567)`.
- `TileGenCases.cs` writes `elevation_tilegen.json` with these cases:
  - `source`: args `{"name", "kind": "SRTM3"|"SRTM1"|"GLOBE", "rows", "cols", "fill": [[r0,r1,c0,c1]], "voids": [[r0,r1,c0,c1,v]]}`, result `null`. The sources:
    - `N50E011.hgt` (SRTM3; fill all; voids `[1150,1200,0,100,-32768]` and `[1150,1200,100,200,-32768]`: the void tiles `JO50MA`, first, and `JO50NA`, after non-void tiles);
    - `S01W002.hgt` (SRTM3, fill all: negative coordinates, start letter `A`);
    - `N50E010.hgt` (SRTM1, fill all);
    - `C10G` (GLOBE 4800 × 10800; fill `[4680,4800,1200,1440]` = square JO50; void `[4795,4800,1200,1210,-600]` = tile JO50AA, all `<= -500`), run with `oi0=0, oj0=0, oi1=5, oj1=0`;
    - `G10G` (GLOBE 6000 × 10800; fill `[0,120,10560,10800]` = NN49 at the file's north-east corner), run with `oi0=4, oj0=4, oi1=4, oj1=9`;
    - `dsrc.hgt` (SRTM1 for `downsample`: fill all, but with `((r * 7 + c * 3) % 3000) - 100`, so no 3 × 3 sum overflows; flagged by `"formula": "downsample"` in args);
    - `cmp-b.hgt` (SRTM3: the `N50E011.hgt` data, plus voids `[0,10,0,1201,-32768]` and `[600,601,0,1201,1000]`).
  - `srtm_tile` / `globe_tile`: one case per tile position in C# loop order, args `{"source", "tile"}`. Result `{"written": false}`, or `{"written": true, "members": [<top-level JSON member names in file order>], "min_elv", "max_elv", "rows", "columns", "min_lat", "min_lat_bits", "min_lon", "min_lon_bits", "max_lat", "max_lat_bits", "max_lon", "max_lon_bits", "elv_len", "elv_fnv", "last_updated"}`. These are read back from the written `.loc` file with Newtonsoft (`JObject`, `DateParseHandling.None`). `*_bits` is `BitConverter.DoubleToInt64Bits(x).ToString("x16")`. `elv_fnv` is FNV-1a 64 (offset `0xcbf29ce484222325`, prime `0x100000001b3`, hex, 16 digits) over each `Elv[k][l]` as two little-endian bytes, with `k` outer.
  - `downsample`: args `{"source": "dsrc.hgt"}`, result `{"len", "fnv"}` of the output file bytes.
  - `compare`: args `{"a": "N50E011.hgt", "b": "cmp-b.hgt"}`, result `{"len", "fnv"}` of the `.hgtdiff` bytes.
- The sources are written to `Path.GetTempPath()/tilegen` (GLOBE with `FileStream.SetLength`, then only the filled rows) and deleted afterwards.

- [ ] **Step 1: Write the two `.cs` files and the `Write` line.** Copy the C# by hand from `target/upstream/AirScout` @ `91b02cb`; diff each copied method against upstream (`diff <(sed -n 'A,Bp' upstream) <(sed -n 'C,Dp' copy)`). The only differences are the listed edits.
- [ ] **Step 2: Run** `tools/golden-harness/run.sh`. Expected: `elevation_tilegen.json: N cases`. `git status --short tests/golden` shows only `?? tests/golden/elevation_tilegen.json`; any change to another golden file means stop and report (non-determinism).
- [ ] **Step 3: Check the vectors.** `jq` on the file:
  - `N50E011.hgt` has 288 tile cases, `JO50MA` `written: false`, `JO50NA` `written: true` with `min_elv == 32767`;
  - `S01W002.hgt` tiles start with `II99A`;
  - `N50E010.hgt` has 288, `C10G` 576 with `JO50AA` `written: false`, `G10G` 576;
  - `last_updated` is `2026-11-24T12:00:00.1234567Z`.
  Report the `members` list of one tile and the file size (expected well under 1 MB).
- [ ] **Step 4: Commit** `test(golden): C# tile generator vectors (tiles, downsample, compare)`.

### Task 3: `elevation-tile-gen` crate and `tiles`

**Files:**
- Create:
  - `crates/elevation-tile-gen/Cargo.toml` (`[lib] name = "elevation_tile_gen"`, `[[bin]] name = "elevation-tile-gen"`; deps `as-elevation`, `as-geo`, `chrono`, `clap`, `serde_json`, `zip`; dev `as-golden`, `assert_cmd`, `tempfile`, `mockito`, `reqwest`);
  - `crates/elevation-tile-gen/src/lib.rs`, `src/main.rs`, `src/source.rs`, `src/tiles.rs`.
- Modify: `Cargo.lock`
- Test: `crates/elevation-tile-gen/tests/common/mod.rs` (synthetic sources from the golden `source` cases, `fnv`), `tests/tilegen_golden.rs`, `tests/tiles.rs`

**Interfaces (`ElevationTileGenerator/MainDlg.cs:72-196`, `:199-522`, `:524-542`; `ElevationTileGenerator/Properties/Settings.settings:11-13`):**
- Consumes: `ElevationTile::new(index, min_elv, min_lat, min_lon, max_elv, max_lat, max_lon, rows, columns, elv, last_updated) -> Result<ElevationTile, ElevationError>` (`crates/as-elevation/src/tile.rs:62`), `ElevationTile::{base_lat, base_lon, to_loc_json}`, `ElevationModel`, `as_geo::maidenhead::loc_from_lat_lon(lat, lon, false, 3, false) -> Result<Option<String>, LocatorError>`.
- Produces (`source.rs`):
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum SourceKind { Srtm3, Srtm1, Globe }` with `pub fn model(self) -> ElevationModel` and `pub fn name(self) -> &'static str` (`"SRTM3"`, `"SRTM1"`, `"GLOBE"`).
  - `#[derive(Clone, Debug, PartialEq)] pub struct Source { pub path: PathBuf, pub kind: SourceKind, pub lat_base: i32, pub lon_base: i32, pub rows: usize, pub cols: usize }`.
  - `pub fn identify(path: &Path) -> io::Result<Option<Source>>`. The rules of `:72-196`, with every length checked before any index:
    - SRTM: extension `.hgt` in any case; stem of exactly 7 characters `[NS]dd[EW]ddd` in any case (C# accepts longer stems, finding 2); size 1201² × 2 (SRTM3) or 3601² × 2 (SRTM1); `lat_base`/`lon_base` signed as `:204-205`.
    - GLOBE: no extension; stem of 4 characters `[A-P]dd[GB]` in any case; size from the letter table (`:102-124`), and `lat_base`, `lon_base`, `rows`, `cols` from `:310-414`.
    - Anything else is `None`; a missing file or a directory is `None` too.
- Produces (`tiles.rs`):
  - `#[derive(Debug)] pub enum Outcome { Tile(ElevationTile), Void(String), Exists(String) }`, with `pub fn index(&self) -> &str`.
  - `pub fn srtm_tiles(src: &Source, data: &[u8], now: DateTime<Utc>, exists: &dyn Fn(&str) -> bool) -> Result<Vec<Outcome>, ElevationError>`: the SRTM branch `:201-302` 1:1, 288 outcomes in C# order (`i` 0..12 outer, `j` 0..24 inner).
    - Tile base: the first 4 characters of `loc_from_lat_lon(lat_base, lon_base, false, 3)` (`:206`). The first letter is `A` for an even `lon_base`, else `M` (`:230`; `-1 % 2 != 0`).
    - `Rows = rows / 24`, `Columns = cols / 12`.
    - Sample `(k, l)`, big-endian `i16`, at byte `((rows - 2 - j*Rows - l) * cols + i*Columns + k) * 2` (`:251`).
    - Min and max skip `-32768` and keep the first extreme; the positions are `base + k * step` (`:256-267`).
    - An existing tile (`exists`) is `Exists` and is not read. `all_void` starts `true` once per **file** (`:228`) and is never reset: only all-void tiles before the first non-void tile are `Void` (finding 3).
    - `data.len()` must be `rows * cols * 2`, else `Corrupt`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct GlobeIndex { pub i0: usize, pub j0: usize, pub i1: usize, pub j1: usize, pub i2: usize, pub j2: usize }`.
  - `pub fn globe_indices(src: &Source) -> Vec<GlobeIndex>`: the loops `:424-437` in C# order (`j0max` 4 for 6000 rows else 3; `i1max` 9 for `i0 < 4` else 4).
  - `pub fn globe_tile_index(src: &Source, g: GlobeIndex) -> String`: `loc_from_lat_lon` of the midpoint `:441-444`.
  - `pub fn globe_tile(src: &Source, data: &[u8], g: GlobeIndex, now: DateTime<Utc>) -> Result<Option<ElevationTile>, ElevationError>`: `:450-482` 1:1.
    - `Rows = 5`, `Columns = 10` (`:453-454`).
    - Little-endian `i16` at byte `((rows - 1 - j0*10*24*Rows - j1*24*Rows - j2*Rows - l) * cols + i0*10*24*Columns + i1*24*Columns + i2*Columns + k) * 2` (`:460`).
    - `<= -500` becomes `-32768` (`:462-463`).
    - `None` when all samples are void (per tile, `:450`).
  - `pub fn loc_path(out: &Path, index: &str) -> PathBuf`: `<out>/<index[0..2]>/<index[2..4]>/<index>.loc` (`:209`, `:447`).
  - `pub fn write_loc(out: &Path, tile: &ElevationTile) -> io::Result<PathBuf>`: create the dirs, write `to_loc_json()` to `.part`, `File::set_modified(last_updated)`, rename.
- Produces (`main.rs`, `#![forbid(unsafe_code)]`): clap `Parser` with subcommand `tiles <SOURCE>... --out <DIR> [--overwrite] [--squares <LIST>] [--time <RFC3339>]`.
  - A directory argument expands to its entries, sorted by name. Non-sources give the stderr line and are skipped.
  - No source: `no source file`. Two kinds: `inputs mix {A} and {B}: run once per model with its own --out` (first two kinds in input order). Both errors happen before any write.
  - `--time` (default now, UTC) is `LastUpdated` of every tile. `--overwrite` makes `exists` always false. Without it, `exists` is `loc_path(..).exists()`.
  - `--squares JO50,jo61`: upper-cased 4-character prefixes. SRTM: filter the outcomes after `srtm_tiles` (so `all_void` follows the whole file). GLOBE: skip an index before reading; a whole `(i0, j0, i1, j1)` block may be skipped when the square of its midpoint is not listed.
  - Each source file is read whole into memory (`:222-225`).
  - stderr per source: `{file}: {w} written, {v} void, {e} kept`. stdout (pretty JSON): `{"model": kind name, "out": path, "sources": n, "written": w, "void": v, "kept": e}`. Errors: `error: {e}`, exit 1.

- [ ] **Step 1: Write the failing tests:**
  - `tilegen_golden.rs` (`as_golden::load("elevation_tilegen.json")`; data built by `common::source_bytes(&case)` from a `source` case; `now` = the golden `last_updated`):
    - `srtm_tiles_match_csharp`: for each SRTM source, the outcome indices equal the golden `tile` order (288 entries). `written` ↔ `Outcome::Tile`. For a tile: every `*_bits` equals `to_bits()` of the field; `min_elv`, `max_elv`, `rows`, `columns`, `elv_len` and `fnv(&tile.elv)` are equal. The members of `to_loc_json` (lines matching `^  "(\w+)": `) equal the golden `members` without `"Bounds"`. Assert that both `written` and not-written cases occur, so the void path is exercised.
    - `globe_tiles_match_csharp`: the same for `C10G` and `G10G`. The indices of one block of `globe_indices` (`i0, j0, i1, j1` fixed) map through `globe_tile_index` to the golden `tile` order.
  - `tiles.rs`:
    - `identify_accepts_and_rejects` (Review Focus 1): in a tempdir, `N50E011.hgt` (1201² × 2 B) → `Srtm3, 50, 11`; `s01w002.HGT` → `Srtm3, -1, -2`; `N50E010.hgt` (3601² × 2) → `Srtm1`; `C10G` (sparse 4800 × 10800 × 2) → `Globe, 50, 0, 4800, 10800`. `None` for `N5.hgt`, `N50E011X.hgt` (SRTM3 size), `x.hgt` (SRTM3 size), a 100-byte `N50E011.hgt` in a subdir, `C10G.bin`, `ABC`, `Q10G`, `C1XG` and a directory `D10G`.
    - `cli_writes_srtm3_tree`: `tiles <tmp>/N50E011.hgt --out <out> --time 2026-11-24T12:00:00Z` (the golden `N50E011.hgt` data). Exit 0, `v["written"] == 287`, `v["void"] == 1`. `<out>/JO/50/JO50MB.loc` starts with `"{\r\n  \"TileIndex\": \"JO50MB\",\r\n"` and ends with `"  \"LastUpdated\": \"2026-11-24T12:00:00Z\"\r\n}\r\n"`. Its mtime is within 2 s of that time. No `.part` file exists under `<out>`.
    - `round_trip_through_as_elevation`: the tree of `cli_writes_srtm3_tree`. Every `.loc` parses with `ElevationTile::from_loc_json` and is inserted into an `ElevationStore` (`Srtm3`). For every 7th cell of every tile, `store.elevation(Srtm3, lat, lon, false)` at the cell centre equals the in-memory tile of `srtm_tiles` at that point (`tile.get_elevation`). Fixed values from the formula:
      - `JO50MB` cell `(0, 0)` (row 1149, column 0) gives `3719`;
      - a point in `JO50MA` gives `-32767` (not written);
      - a point in `JO50NA` gives `-32768` (written all-void, finding 3).
    - `existing_tiles_kept_unless_overwrite` (Review Focus 3): run twice, `--time` `T1` then `T2`. The second run reports `kept == 287`, `written == 0`, and `JO50MB.loc` still has `T1`. A third run with `--overwrite` gives `written == 287` and `T2`.
    - `mixed_models_rejected` (Review Focus 2): `tiles N50E011.hgt N50E010.hgt --out <out>` exits 1 with `inputs mix SRTM3 and SRTM1`; `<out>` holds no `.loc`.
    - `no_source_is_error`: `tiles <tmp>/x.hgt` exits 1; stderr has `not an SRTM1, SRTM3 or GLOBE file:` and `no source file`.
    - `squares_filter_globe`: sparse `C10G` with the fill of square JO50 only. `tiles C10G --squares jo50 --out <out>` gives exactly 576 `.loc` files, all under `<out>/JO/50`, and `v["written"] == 576`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p elevation-tile-gen`. Expected: FAIL (crate empty).
- [ ] **Step 3: Implement.** Run one `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build -p elevation-tile-gen` without `--locked`, then the `Cargo.lock` check. If a golden case fails, do not change the expectation: report the tile and the field (DONE_WITH_CONCERNS).
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p elevation-tile-gen`, then the Windows check. Expected: PASS. Report `wc -l` of the new files.
- [ ] **Step 5: Commit** `feat(elevation-tile-gen): tiles from SRTM and GLOBE files, 1:1 with the C# generator`.

### Task 4: `zip` and `catalogue`; the self-hosted tree

**Files:**
- Create: `crates/elevation-tile-gen/src/pack.rs`
- Modify: `crates/elevation-tile-gen/src/lib.rs`, `src/main.rs` (two subcommands)
- Test: `crates/elevation-tile-gen/tests/pack.rs`

**Interfaces (`ElevationTileGenerator/MainDlg.cs:544-589`, `:677-789`; `ScoutBase/ScoutBase.Elevation/ElevationCatalogue.cs:45`):**
- Consumes: `loc_path`, `write_loc` (Task 3); `ElevationTile::from_loc_json`; `read_files_zip` (Task 1); in tests `Catalogue::fetch(&Client, base_url, model, rect, cache_dir)`, `update_square(&Client, base_url, model, square4, &store, None, &mut |_| {})` (`crates/as-elevation/src/updater.rs:18`) and `ElevationStore::{open, elevation}`.
- Produces (`pack.rs`):
  - `#[derive(Debug, Default, PartialEq)] pub struct ZipReport { pub squares: usize, pub tiles: usize, pub removed: usize }`.
  - `pub fn zip_squares(out: &Path, remove_loc: bool) -> Result<ZipReport, ElevationError>`:
    - For `AB` in `AA..=RR` and `CD` in `00..=99` (C# order, `:549-556`): if `<out>/AB/CD` holds `*.loc` files, write `<out>/AB/ABCD.zip`.
    - Entries: flat names (`:571`), sorted, Deflated. Each entry's `last_modified` is the tile's `LastUpdated` as a zip date in UTC fields. Every `.loc` must parse, else `Corrupt("{file}: …")` and no zip for that square.
    - `.zip.part`, then rename over an old zip.
    - With `remove_loc`: reopen the zip, compare every entry's bytes with its `.loc` file, then delete the `.loc` files and `remove_dir` the folder (only if empty). On any mismatch nothing is deleted and the run fails.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)] pub enum CatFrom { Zip, Loc }`.
  - `pub fn scan_catalogue(out: &Path, from: CatFrom) -> Result<BTreeMap<String, DateTime<Utc>>, ElevationError>`:
    - Key: the entry or file name (`JO50AA.loc`, `:691`, `:724`). Value: the tile's `LastUpdated` from its JSON, not a file or entry time (finding 8).
    - Recursive under `out`, symlinks not followed.
    - `Zip` reads every `*.zip` except `<out>/files.zip` and only its `.loc` entries; `Loc` reads every `*.loc`.
    - A name seen twice is `Corrupt("tile {name} is in {a} and {b}")`.
  - `pub fn catalogue_text(files: &BTreeMap<String, DateTime<Utc>>, created: DateTime<Utc>) -> String`. Lines end with `\r\n`. Exactly:
    ```
    // (D)igital (E)levation (Model) tile catalogue
    // created {created:%Y-%m-%d} with elevation-tile-gen (AirScout-RS)
    // GLOBE data homepage: https://www.ngdc.noaa.gov/mgg/topo/globe.html
    // SRTM3 data homepage: https://lpdaac.usgs.gov/dataset_discovery/measures/measures_products_table/srtmgl3_v003
    // SRTM1 data homepage: https://lpdaac.usgs.gov/dataset_discovery/measures/measures_products_table/srtmgl1_v003

    {name};{time:%Y-%m-%d %H:%M:%SZ}
    ```
    The names are in `BTreeMap` order (ordinal, equal to the C# `SortedDictionary` for these names).
  - `pub fn write_catalogue(out: &Path, text: &str) -> io::Result<()>`: `<out>/files.cat`, and `<out>/files.zip` with the single entry `files.cat` (`:751-756`), both via `.part`.
- `main.rs`: `zip --out <DIR> [--remove-loc]` prints `{"squares", "tiles", "removed"}`. `catalogue --out <DIR> [--from zip|loc] (default zip) [--time <RFC3339>]` prints `{"tiles": n, "from": "zip", "files": ["files.cat", "files.zip"]}`.

- [ ] **Step 1: Write the failing tests** (`pack.rs`; trees made with the Task 3 binary from the golden `N50E011.hgt` data, `--time 2026-11-24T12:00:00Z`):
  - `zip_packs_square_flat_deflated`: `zip --out <out>` gives `v == {"squares": 1, "tiles": 287, "removed": 0}`. `<out>/JO/JO50.zip` has 287 entries, sorted, named `JO50MB.loc` … without a path, each `CompressionMethod::Deflated`, each with the bytes of its `.loc` file. The `.loc` files are still there.
  - `zip_remove_loc_verifies_first`: `zip --out <out> --remove-loc` gives `removed == 287`. `<out>/JO/50` no longer exists, and the zip still has 287 entries.
  - `zip_bad_tile_removes_nothing` (Review Focus 3): first `zip` (old zip, 287 entries), then append `x` to `JO50MB.loc`. `zip --remove-loc` exits 1 naming `JO50MB.loc`. All 287 `.loc` files remain, and the old zip is byte-identical.
  - `catalogue_lists_every_tile`: after `zip`, `catalogue --out <out> --time 2026-11-25T08:00:00Z`. `files.cat` equals `catalogue_text` with the header line `// created 2026-11-25 with elevation-tile-gen (AirScout-RS)` and 287 lines like `JO50MB.loc;2026-11-24 12:00:00Z`. `files.zip` holds only `files.cat` with the same bytes. `read_files_zip(<out>/files.zip)` has 287 keys starting `JO50`. A second `catalogue` run gives the same `files.cat` (the root `files.zip` is not scanned).
  - `catalogue_duplicate_tile_is_error`: copy `JO/JO50.zip` to `JO/JO50-copy.zip`. `catalogue` exits 1 with `tile JO50MB.loc is in`.
  - `self_hosted_tree_feeds_the_updater` (Review Focus 5):
    - Tree from `N50E011.hgt` and `S01W002.hgt` (both SRTM3), then `zip` and `catalogue`.
    - mockito serves `GET /SRTM3/files.zip`, `/SRTM3/JO/JO50.zip` and `/SRTM3/II/II99.zip` with the generated bytes.
    - `Catalogue::fetch(&client, &server.url(), Srtm3, GRect::new(-1.5, -2.5, 51.0, 12.0), cache)` lists every generated tile.
    - `update_square` for `JO50` and `II99` returns 287 and 288.
    - For every 7th cell centre of every generated tile, `store.elevation(Srtm3, lat, lon, false)` equals the in-memory tile from `srtm_tiles`. A point in `JO50MA` gives `-32767`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p elevation-tile-gen --test pack`. Expected: FAIL (no subcommand).
- [ ] **Step 3: Implement** `pack.rs` and the two subcommands.
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p elevation-tile-gen`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(elevation-tile-gen): zip and catalogue; a self-hosted tree feeds the app's updater`.

### Task 5: `compare`, `downsample`, `preview`

**Files:**
- Create: `crates/elevation-tile-gen/src/hgt.rs`, `crates/elevation-tile-gen/src/preview.rs`
- Modify: `Cargo.toml` (workspace `png = "0.18"`), `crates/elevation-tile-gen/Cargo.toml` (`png`), `src/lib.rs`, `src/main.rs`, `Cargo.lock`
- Test: `crates/elevation-tile-gen/tests/hgt.rs`

**Interfaces (`ElevationTileGenerator/MainDlg.cs:1018-1150`, `:1252-1301`, `:621-631`, `:811-854`; `ScoutBase/ScoutBase.Elevation/ElevationTile.cs:183-213`):**
- Consumes: `identify`, `SourceKind` (Task 3); `palette_color` (Task 1); `ElevationTile::from_loc_json`.
- Produces (`hgt.rs`):
  - `pub fn diff_bytes(a: &[u8], b: &[u8]) -> Result<Vec<u8>, ElevationError>`: `CompareFilesHGT` 1:1 (`:1116-1139`).
    - Different lengths give `InvalidArgument("cannot compare files with different length")`.
    - Per big-endian pair: `-32768` unless both are `>= -500`, else `e1.wrapping_sub(e2)`.
    - The result is written **little-endian** (`BinaryWriter`, `:1133`).
  - `#[derive(Debug, PartialEq, serde::Serialize)] pub struct DiffStats { pub samples: usize, pub width: usize, pub differing: usize, pub voids: usize, pub min: Option<i16>, pub max: Option<i16>, pub mean: Option<f64> }` and `pub fn diff_stats(diff: &[u8]) -> (DiffStats, BTreeMap<i16, u64>)`. Not the C# statistics (finding 10):
    - `width = floor(sqrt(samples))`; `voids` counts `-32768`; `differing` counts non-void `!= 0`;
    - min, max, mean (`f64`) and the histogram are over non-void samples only (`None` when there is none).
  - `pub fn histogram_csv(h: &BTreeMap<i16, u64>, min: i16, max: i16) -> String`: `Difference[m];Count\n`, then every value `min..=max` as `{v};{count}\n` (count 0 included, `:1075-1082`).
  - `pub fn downsample(srtm1: &[u8]) -> Result<Vec<u8>, ElevationError>`: `ConvertSRTM1ToSRTM3File` (`:1263-1301`). Input length 3601² × 2, output 1201² × 2, big-endian.
    - Layout 1:1: point `(i, j)` for `i, j < 1200` averages the 3 × 3 block at rows `3i..3i+2`, columns `3j..3j+2`. Column 1200 repeats the last value of the row (`:1288-1290`). Row 1200 is SRTM1 row 3600 at columns `3j` (`:1292-1297`).
    - Per controller ruling (owner question 1): the sum is `i32` (no `short` overflow); a block with any `-32768` gives `-32768`; else `sum / 9` truncated toward zero.
- Produces (`preview.rs`): `pub fn square_rgba(tiles: &[ElevationTile]) -> Result<(u32, u32, Vec<u8>), ElevationError>`: `PictureFromTiles` + `ToBitmap` (`:811-844`; `ElevationTile.cs:183-213`).
  - All tiles must share one 4-character square and one size, else `InvalidArgument`. Size: `Columns * 24` × `Rows * 24`.
  - Tile cell `(k, l)` of tile `index` goes to `x = (index[4] - 'A') * Columns + k`, `y = height - (index[5] - 'A') * Rows - 1 - l`.
  - Colour `palette_color(clamp(e / 3000 * 100, 0, 100))`; a void cell is opaque black; a missing tile is transparent (alpha 0).
- `main.rs`:
  - `compare <A> <B> --out <DIFF> [--histogram <CSV>]` prints `DiffStats`;
  - `downsample <SRTM1.hgt> --out <SRTM3.hgt> [--overwrite]`: the input must `identify` as `Srtm1`; an existing `--out` without `--overwrite` is an error;
  - `preview <ZIP or DIR> --out <PNG>`: `.loc` entries of a zip, or `*.loc` of a dir; writes a PNG (RGBA 8) via `.part`; prints `{"width", "height", "tiles"}`.

- [ ] **Step 1: Write the failing tests** (`hgt.rs`):
  - `downsample_matches_csharp`: the golden `dsrc.hgt` data. `len` and `fnv` of `downsample` equal the golden `downsample` case.
  - `downsample_has_no_short_overflow`: an SRTM1 of all `4000` gives all `4000` (the C# `short` sum gives `-3281`). One `-32768` in the block of point `(0, 0)` gives `-32768` at `(0, 0)` and `4000` at `(0, 1)`.
  - `diff_matches_csharp`: the golden `N50E011.hgt` and `cmp-b.hgt` data. `len` and `fnv` of `diff_bytes` equal the golden `compare` case.
  - `diff_stats_count_voids`: a 4-sample pair `a = [10, -32768, 5, 7]`, `b = [10, 3, 2, -600]` (big-endian) gives the diff `[0, -32768, 3, -32768]` and stats `{samples: 4, width: 2, differing: 1, voids: 2, min: Some(0), max: Some(3), mean: Some(1.5)}`. `histogram_csv` gives `"Difference[m];Count\n0;1\n1;0\n2;0\n3;1\n"`.
  - `compare_lengths_differ_is_error`: CLI with a 4-byte and a 6-byte file exits 1 with `different length`.
  - `preview_places_tiles`: tiles from `srtm_tiles` over the golden `N50E011.hgt` data. `square_rgba` gives `2400 × 1200`. The pixel of `JO50MB` cell `(0, 0)` (`x = 1200`, `y = 1200 - 50 - 1`) is `palette_color(3719 / 3000 * 100 clamped)` with alpha 255. `JO50MA` pixels have alpha 0. CLI `preview` on the Task 4 zip writes a PNG that the `png` crate decodes to the same size.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p elevation-tile-gen --test hgt`. Expected: FAIL.
- [ ] **Step 3: Implement.** Run one `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build -p elevation-tile-gen` without `--locked`, then the `Cargo.lock` check (no `+name`; `png` resolves to the locked 0.18.1).
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p elevation-tile-gen`, then the Windows check. Expected: PASS.
- [ ] **Step 5: Commit** `feat(elevation-tile-gen): compare, downsample and preview`.

### Task 6: `airscout-cli elevation-coverage`

**Files:**
- Create: `crates/as-cli/src/elevation_coverage.rs`
- Modify: `crates/as-cli/src/main.rs` (`mod elevation_coverage;`, `Cmd::ElevationCoverage(elevation_coverage::ElevationCoverageArgs)`, match arm)
- Test: `crates/as-cli/tests/elevation_coverage.rs`

**Interfaces (`ElevationCoverageMapper/MainDlg.cs:22`, `:87-157`, `:159-214`):**
- Consumes: `read_files_cat`, `read_files_zip` (Task 1); `as_net::download_if_newer(&Client, url, path) -> Result<bool, NetError>` (`crates/as-net/src/lib.rs:26`, sends `user_agent()`); `ElevationStore::{open, tile_exists}`; `maidenhead::{check, lat_lon_from_loc, bounds_from_loc}`; `crate::{parse_model, parse_area, Res}`.
- Produces (`elevation_coverage.rs`):
  - `#[derive(clap::Args)] pub struct ElevationCoverageArgs`:
    - `--model` = `"globe"` (C# fixed `GLOBE`, `:22`);
    - `--catalogue` `Option<PathBuf>`: a local `files.zip` (by extension, any case) or `files.cat`;
    - `--download` (flag); `--base-url` = `as_elevation::DEFAULT_BASE_URL`;
    - `--data` `Option<PathBuf>`: compare with the local store;
    - `--area` `Option<String>` (`minlat,minlon,maxlat,maxlon`, default the world);
    - `--geojson` `Option<PathBuf>`; `--level` `square|tile` (clap `ValueEnum`, default `square`).
  - `pub fn tile_cell(index6: &str) -> Option<GRect>`: bottom-left plus `1/24°` × `2/24°` (`:172-177`). `None` unless `index6` is 6 characters and passes `maidenhead::check`.
  - `pub fn run(a: &ElevationCoverageArgs) -> crate::Res<serde_json::Value>`:
    - Both or neither of `--catalogue`/`--download` give `give --catalogue <files.zip|files.cat> or --download` (both: `… not both`), before any socket.
    - The area must have `min < max`, lat in `-90..=90`, lon in `-180..=180`, else `bad area …`.
    - `--download`: one `download_if_newer` of `{base}/{MODEL}/files.zip` into `std::env::temp_dir()/airscout-cli-coverage-{pid}`. The client is built with `user_agent(as_net::user_agent())`. The dir is removed afterwards, also on error. stderr: `{MODEL}: Downloading elevation tile catalogue from {url}` (`:93`).
    - A key that is not a locator counts as `invalid`. A tile is in the area when its cell overlaps it (strict inequalities on both axes). The filter does not use `get_locs_from_rect`, so the #289 gap does not apply (finding 13).
    - `--data`: when `<data>/<model>.db3` is missing, every tile is missing and nothing is created; else `ElevationStore::open(data)` and `tile_exists`. stderr `{MODEL}: {f} tile(s) found, {m} more tile(s) available and missing.` (`:156`).
    - Result: `{"model": dir name, "catalogue": url or path, "area": [4], "tiles": n, "squares": distinct 4-char prefixes, "invalid": i, "found": f or null, "missing": m or null, "geojson": path or null}`.
    - GeoJSON `FeatureCollection` via `.part`. `square`: one `Polygon` per 4-character square (`bounds_from_loc`) with properties `{"square", "tiles"}` and, with `--data`, `"found"`, `"missing"`. `tile`: one per tile with `{"tile", "state": "available" | "found" | "missing"}`. Ring `[[min_lon, min_lat], [max_lon, min_lat], [max_lon, max_lat], [min_lon, max_lat], [min_lon, min_lat]]`.

- [ ] **Step 1: Write the failing tests** (`elevation_coverage.rs`; catalogue `files.cat` written in the test with lines `JO50AA.loc`, `jo50ab.loc`, `JO61UF.loc`, `JN58TD.loc`, `XX99ZZ.loc` (all `;2018-02-27 08:26:32Z`), `broken line`, `JO50AC.loc;bad`; `common::fixture_data_dir()` for the store):
  - `needs_catalogue_or_download` (Review Focus 4): a mockito server with a catch-all mock `.expect(0)`. Without both flags (`--base-url <server>`), the run exits 1 with `give --catalogue`. With both, it exits 1 with `not both`. `mock.assert()`.
  - `download_once_with_user_agent`: `GET /GLOBE/files.zip` mock with `.match_header("user-agent", Matcher::Regex("^AirScout-RS/".into()))`, `.expect(1)`, body = a zip with the test `files.cat`. `--download --base-url <server>` exits 0 with `tiles == 4`, `invalid == 1`, `squares == 3`. No `airscout-cli-coverage-*` dir is left in the temp dir.
  - `counts_found_and_missing_against_store`: `--catalogue files.cat --data <fixture>` gives `found == 3`, `missing == 1`, `invalid == 1`. stderr has `GLOBE: 3 tile(s) found, 1 more tile(s) available and missing.`
  - `no_store_creates_nothing`: `--data <empty tempdir>` gives `found == 0`, `missing == 4`. The dir stays empty.
  - `area_filter_crosses_square_corner` (#289): a catalogue with `JO50XX`, `JO51XA`, `JO60AX`, `JO61AA` and `JO50AA`. `--area 50.99,11.99,51.01,12.01` gives `tiles == 4` (all four corner tiles, not `JO50AA`). `--area 51,12,50,13` exits 1 with `bad area`.
  - `geojson_square_and_tile_levels`: `--geojson g.json` (square level, with `--data`). The file has 3 features, and the `JO50` feature has properties `{"square": "JO50", "tiles": 2, "found": 2, "missing": 0}` and ring start `[10.0, 50.0]` equal to its end. `--level tile` without `--data` gives 4 features, each with `state == "available"`.
- [ ] **Step 2: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cli --test elevation_coverage`. Expected: FAIL (no subcommand).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p as-cli`. Expected: PASS. Report `wc -l crates/as-cli/src/main.rs` (≤ 620).
- [ ] **Step 5: Commit** `feat(as-cli): elevation-coverage subcommand over a local or downloaded catalogue`.

### Task 7: Docs, spec check, findings, gates

**Files:**
- Modify: `README.md`, `docs/logic-findings.md`; `docs/superpowers/specs/2026-11-23-airscout-rs-design.md` only if Step 2 finds a difference.

**Steps:**
- [ ] **Step 1: README:**
  - Status row: `5b | Elevation tools: elevation-tile-gen, airscout-cli elevation-coverage | done; not tested with real SRTM or GLOBE files`.
  - Build: the binaries `airscout`, `airscout-cli`, `catcheck` and `elevation-tile-gen`.
  - New section "Elevation tools (stage 5b)" with one example each: `elevation-tile-gen tiles ./srtm3/*.hgt --out ./www/ElevationData/SRTM3`, then `zip --out …`, then `catalogue --out …`; `airscout-cli elevation-coverage --download --model srtm3 --data ./data --geojson coverage.json`. Then:
    - "Host your own": serve `./www` with any static web server, then set `gui.elevation_base_url` in `settings.toml` (or `--base-url` of `airscout-cli`) to `http://<host>/ElevationData`. The model directory names are `GLOBE`, `SRTM3`, `SRTM1`.
    - Inputs: SRTM `.hgt` files (1201² or 3601² samples) and GLOBE tiles `A10G` … `P10G`, one model per run. The licences of the source data apply; the repository contains no elevation data.
    - Tiles match the original generator 1:1, quirks included (#293, #294). `zip` keeps the `.loc` files unless `--remove-loc`. `catalogue` takes the times from the tiles.
    - `compare`, `downsample` and `preview`, one line each.
  - "Network sockets", developer tools table:
    - the sentence becomes "Only `elevation-coverage --download` makes an HTTP request.";
    - new rows `elevation-coverage --download | <--base-url>/<MODEL>/files.zip (default http://www.airscout.eu/downloads/ScoutBase/1/ElevationData), HTTP GET, User-Agent AirScout-RS/<version> | outbound | only with --download, one request per run` and `elevation-tile-gen | none | - | never`.
  - Privacy: the catalogue request carries no call, locator or area.
  - Findings range: "Stage 5b's are #291 to #N".
- [ ] **Step 2: Spec check:** `:85` lists `elevation-tile-gen` as a bin and `:87` lists `elevation-coverage` as an `airscout-cli` subcommand (Q6, done in 5a). Expected: no change. If either differs, correct it to Q6 and say so in the commit body.
- [ ] **Step 3:** Add findings from **#291** in this order. Each cites C# file:line and says Ported 1:1 / Not reproduced / Port deviation / New design.
  1. S11: `ElevationCoverageMapper/MainDlg.cs:22`, `:87-103`, `:145-156`, `:193-208`. The model is fixed to GLOBE. `files.zip` and `files.cat` are deleted and written in the working directory, and the host is `airscout.eu` without `www`. `found` and `missing` are never counted; the red "missing" branch is never sent. It downloads on every start. **New design** (Q7): `elevation-coverage` with `--model`, a local `--catalogue` or an explicit `--download` (one request, honest User-Agent, temp dir removed), counts against the local store and GeoJSON.
  2. S12: `ElevationTileGenerator/MainDlg.cs:87-91` reads `upperfilename[3]` before the length check, so a short name throws. `IsSRTM3File` and `IsSRTM1File` (`:143-156`, `:177-190`) index up to `[6]` without a length check, and they accept stems longer than 7 characters. **Not reproduced**: lengths are checked first, and an SRTM stem must have exactly 7 characters.
  3. `ElevationTileGenerator/MainDlg.cs:228`: in the SRTM branch, `all_void` is set once per file, not per tile (the GLOBE branch resets it, `:450`). After the first non-void tile, all-void tiles are written too, with `MinElv` 32767 and `MaxElv` -32768. **Ported 1:1** (tests `srtm_tiles_match_csharp`, `round_trip_through_as_elevation`).
  4. S15: `ElevationTileGenerator/MainDlg.cs:251` against `:460`. An SRTM sample of tile cell `(k, l)` comes from row `rows - 2 - j*Rows - l`, the cell's north edge, while its column is the cell's west edge. GLOBE uses `rows - 1`. SRTM tiles are therefore one sample (about 90 m or 30 m) north of the cell they describe. **Ported 1:1**: the tiles must match what airscout.eu serves. Not checked against a served tile, because the repository has no raw SRTM file.
  5. `ScoutBase/ScoutBase.Elevation/ElevationTile.cs:56-68`, `:111`, `:114-124`; `ElevationTileGenerator/MainDlg.cs:241-242`. The current `ToJSON` writes the public `Bounds` field, and the generator never sets it (the `TileIndex` setter does not), so it is all zeros. The served tiles have no `Bounds` member. **Port deviation**: the served layout, checked byte for byte against the fixture tiles: no `Bounds`, `\r\n`, .NET Framework `"R"` numbers.
  6. Tool layer of `ProcessTile`/`Scan` (`ElevationTileGenerator/MainDlg.cs:239`, `:273`, `:448`, `:482`, `:487-495`, `:524-542`; `Properties/Settings.settings:11-13`):
     - files come from a file dialog;
     - existing tiles are skipped, and the `Overwrite` setting is never read;
     - `LastUpdated` is the clock of each tile;
     - a failed `CreateDirectory` is swallowed;
     - several models can be mixed into one folder.
     **New design**: `tiles <SOURCE>…` with `--overwrite`, `--time`, `--squares`, one model per run, and writes through a `.part` file.
  7. S16: `ElevationTileGenerator/MainDlg.cs:544-589`. `ZIP` deletes the `.loc` files and the folder once `Save` returns, without checking the archive, and also deletes empty folders. The comment says "sleep 5s"; the code sleeps 1 s (`:573-574`). **Port deviation**: the `.loc` files are kept unless `--remove-loc`. Even then, every entry is compared with its file first.
  8. Catalogue: `ElevationTileGenerator/MainDlg.cs:690`, `:718-725`, `:738-745`.
     - The time is the `.loc` creation time or the zip entry time; a copied tree gets other times.
     - `CATFromZIP` also scans the `files.zip` of an earlier run and lists `files.cat` as a tile.
     - A duplicate name throws.
     - The header credits DL2ALF and claims ASTER void filling for any tree.
     **Port deviation**: the time comes from the tile's `LastUpdated`. The root `files.zip` and non-`.loc` entries are skipped. A duplicate is an error naming both places. The header names the tool and keeps the GLOBE and SRTM home pages.
  9. S13: `ElevationTileGenerator/MainDlg.cs:1270-1290`. The 3 × 3 sum is a `short`, which overflows above about 3640 m, and `-32768` voids are averaged into their neighbours. **Not reproduced** (owner question 1): `i32` sum; a block with a void is void. The layout is 1:1 (a block starts at the sample, column 1201 repeats the last value, row 1201 is sampled).
  10. `ElevationTileGenerator/MainDlg.cs:1018-1150`. The difference file goes to the program directory, and `ElevationDifference.csv`/`.tif` go to the working directory. The diff is written little-endian, although the sources are big-endian. The statistics are wrong:
      - voids are counted as `-512` (never true);
      - min and max start at 0 and include the `-32768` voids;
      - the mean is an integer division (`sum / length`, `:1088`).
      **Port deviation**: the diff bytes are 1:1 (little-endian, `-500` threshold) to the `--out` path. The statistics count `-32768` as voids and leave voids out of min, max, mean and the histogram.
  11. Pictures: `ElevationTileGenerator/MainDlg.cs:621-631` draws a fixed area into `Elevation.tif` in the working directory. "Pic fm Loc-Tiles" lists files and does nothing (`:846-854`). **New design**: `preview` draws one square from a zip or a folder into a PNG with the `ToBitmap` colours (`ElevationTile.cs:183-213`).
  12. S14: `ElevationTileGenerator/MainDlg.cs:856-890`. `ScanDirDelete` recurses through `ScanDirZIP` (`:862`), and `Delete` empties a folder without confirmation. **Not reproduced**: no delete command.
  13. #289 and coverage: the app's coverage job and `Catalogue::fetch` select squares with `GetLocsFromRect` (1:1, #289). **Port deviation**: `elevation-coverage --area` keeps every tile whose cell overlaps the area, so a narrow area across a square corner lists all four squares (test `area_filter_crosses_square_corner`).
- [ ] **Step 4:** Run the full gate on macOS, the hamlib feature check, `cargo clippy -p as-cat` alone, the Linux docker gate and the Windows check (Global Constraints). Expected: green. Report the test counts. The hosted Windows CI job runs after the owner-approved push.
- [ ] **Step 5: Commit** `docs: stage 5b README, tool sockets, self-hosting and logic findings`.

## Self-review against the spec

- **Spec stage 5, 5b share:** `elevation-tile-gen` (Tasks 1-5) and `elevation-coverage` (Task 6). Binary layout per Q6 (spec `:85`, `:87`, checked in Task 7). Fallback "if airscout.eu is gone" (`:48-49`, `:138-140`): Task 4 `self_hosted_tree_feeds_the_updater` and the README "Host your own".
- **Owner decisions:** Q6 (separate binary, subcommand), Q7 (subcommand in 5b).
- **Survey §5.1, §5.2:** every recommended subcommand is covered: `tiles`, `zip` (no delete without `--remove-loc`), `catalogue`, `compare`, `downsample` (S13 per owner question 1) and `preview`; `Delete` and the fixed picture area are dropped. Golden vectors from the C# harness: Task 2.
- **Survey §10:** the coverage tool's one download is explicit (`--download`), with `as_net::user_agent()` via `download_if_newer`. The generator is offline. README rows: Task 7.
- **Survey §11:** S11 (finding 1), S12 (2), S13 (9), S14 (12), S15 (4), S16 (7), plus the new findings 3, 5, 6, 8, 10, 11, 13.
- **Project rules:** no secrets (no credential is read); honest UA; bulk downloads none; synthetic data only; no LAN address; three OS; disk rules; signed commits; linear history.
- **Type consistency:** `ElevationTile::to_loc_json` (T1) is used by `write_loc` (T3) and the zip check (T4). `read_files_zip` (T1) is used by T4 tests and T6. `Source`, `SourceKind`, `identify`, `srtm_tiles`, `globe_tile`, `Outcome` (T3) are used by T4 and T5 tests and by `downsample`. `palette_color` (T1) is used by `square_rgba` (T5). `crate::{parse_model, parse_area, Res}` come from `as-cli/src/main.rs`.
- **Lessons of 5a:**
  - C1: the golden and round-trip tests assert counts and both branches (written and void), so an empty or always-void run fails.
  - C6: the round trip compares the store against the generator's own in-memory tiles and against fixed numbers (`3719`, `-32767`, `-32768`), not against a copy of the index formula. The index formula itself is pinned only by the C# vectors.
  - #289: Task 6 does not use `get_locs_from_rect` for its area.
- **Proportion:** 7 tasks; the only code-like block is the catalogue header, which is exact copy.

## Open questions (owner) and controller rulings

Owner questions (recommendation first):

1. **S13, `downsample` (SRTM1 to SRTM3).** Recommendation: fix the two data bugs (an `i32` sum instead of the overflowing `short`; a block with a void gives a void) and keep the C# layout 1:1. Logged as "Not reproduced" (finding 9). The alternative is 1:1 with the overflow, so mountains above about 3640 m get wrapped values in every SRTM3 file made this way. The survey left this open ("with S13 decided"). Until the owner answers, Task 5 implements the recommendation. A 1:1 change would touch one function and one test.

Controller rulings (binding for the implementers; each with its cost if wrong):

- Ruling: tile content is 1:1, quirks included (S15 row offset, SRTM `all_void` per file). The served airscout.eu tiles were made by this code, and a self-hosted tree must give the app the same elevations — cost if wrong: self-hosted SRTM data keeps the one-sample shift until stage 6.
- Ruling: `.loc` files use the served layout (no `Bounds`, `\r\n`, .NET Framework `"R"` numbers), not what the current C# `ToJSON` would write. The served fixtures are the only byte-exact reference, and C# clients read tiles without `Bounds` today — cost if wrong: none for readers; the bytes differ from a hypothetical .NET 8 build.
- Ruling: the golden vectors are value-exact (IEEE bits, `Elv` digests, member names), not text-exact. The harness runs .NET 8, whose `"R"` differs from the .NET Framework text in the served files; Task 1 pins the text against those files — cost if wrong: none; both checks exist.
- Ruling: the GLOBE loops in the harness copy run over one 1° × 2° block per source (edit listed in the header). A full GLOBE file is 1.15 million tiles — cost if wrong: an index bug outside the two blocks; the two blocks include a file corner.
- Ruling: `elevation-tile-gen` is a library plus a binary. The golden and round-trip tests need the tile functions; 5a C6 showed what a bin-only crate costs — cost if wrong: none.
- Ruling: `elevation-coverage` downloads only with `--download`, and with neither `--download` nor `--catalogue` it is an error. The task brief requires an explicit action for the one download — cost if wrong: one extra flag compared with the C# start-and-download.
- Ruling: catalogue times come from each tile's `LastUpdated` — file creation times cannot be set portably, and zip entry times are DOS local times with 2 s resolution — cost if wrong: a tile edited by hand without changing `LastUpdated` keeps its old time.
- Ruling: the catalogue header names the tool and drops "(c) DL2ALF" and the ASTER claim. A user's own tree is not DL2ALF's data — cost if wrong: four comment lines differ from the served header; readers skip `//` lines.
- Ruling: `preview` uses the `png` crate. It is already in `Cargo.lock` through eframe, so the edge is new but no download is — cost if wrong: one direct dependency more.
- Ruling: `compare` keeps the little-endian diff file of the C# (1:1, golden) but fixes the statistics (finding 10) — cost if wrong: a diff read as big-endian elsewhere is wrong, as upstream.
- Ruling: `elevation-coverage --data` does not open the store when `<model>.db3` is missing. `ElevationStore::open` creates all five database files (`crates/as-elevation/src/store.rs:81-98`) — cost if wrong: none.

Survey corrections found while planning (the plan follows these):

- §5.2: the first subsquare letter is set at `MainDlg.cs:230`, not `:236`. The GLOBE `<= -500` rule is at `:462-463`.
- §5.2 says "the stage 1 harness has ScoutBase.Elevation". The harness links no ScoutBase.Elevation file. It uses verbatim copies (`UpstreamTile.cs`), and the generator code lives in the WinForms exe, so Task 2 copies it.
- §5.2 "golden `.loc` JSON is compared": the harness runs .NET 8, whose number text differs from the served (.NET Framework) files. The golden vectors are value-exact, and the text is pinned against the served fixtures (Task 1).
- §11 S15 says "suspected one-sample north shift". The index arithmetic confirms it for latitude only (north edge in latitude, west edge in longitude, finding 4).
- §11 misses the SRTM `all_void` carried across tiles (`:228`), the zero `Bounds` member, the `files.zip` self-listing of `CATFromZIP` (`:718-725`), the unused `Overwrite` setting, the SRTM name check without a length check, and the statistics bugs of the diff view (`:1020-1021`, `:1042-1043`, `:1088`).
