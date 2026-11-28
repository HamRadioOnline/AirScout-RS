# AirScout-RS Stage 2 — Aircraft + Feeds Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the aircraft side of AirScout to Rust. That covers the aircraft/type/airline/airport database, the position history, the plane cache with extrapolation, the crossing prediction ("which plane is usable and when"), the ADS-B decoder, all 11 plane feeds and the signal-level database. Golden tests prove the results equal the original C# code.

**Architecture:** Four new library crates on top of stage 1:
- `as-aircraft`: PlaneInfo, DBs, cache, checker, crossing prediction. Depends on `as-geo`, `as-db`, `as-net` and `as-propagation`.
- `as-adsb`: pure decoder, no I/O.
- `as-feeds`: `PlaneFeed` trait, one module per feed, and the feed worker. Depends on `as-aircraft` and `as-adsb`.
- `as-signals`.

Feeds are **blocking**: reqwest blocking or `std::net`, with the worker on its own `std::thread`. That matches the original's sequential poll loop and the stage-1 crates. The stage-3 GUI hosts the worker thread. The golden harness gets the relevant C# files linked, or extracted verbatim like `UpstreamHorizon.cs`.

**Tech Stack:**
- Stage 1 stack.
- `zstd` (binCraft).
- `aes` + `cbc` + `md-5` (OpenSSLDecrypt, only if a task needs it; see Task 13).
- `base64` for test fixtures.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md` (stage 2 and "Calls made by Claude").

**Upstream:** `target/upstream/AirScout` @ `91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`. The survey of all stage-2 modules, with file:line references, is the input for the logic findings. Tasks cite upstream lines directly.

## Global Constraints

Everything from the stage-1 plan's Global Constraints still holds:
- port 1:1;
- `docs/logic-findings.md` for every suspected bug;
- no `unwrap`/`expect` in library code;
- `thiserror`;
- `#![forbid(unsafe_code)]`;
- CI on all three OS;
- fixtures committed, so CI never needs .NET.

New for stage 2:
- **Units as in the original.** `PlaneInfo` stores altitude in **ft** and speed in **kts**. `alt_m()` = `ft / 3.28084` and `speed_kmh()` = `kts * 1.852` are derived methods. Feed args use metres for MinAlt/MaxAlt.
- **Times** are `chrono::DateTime<Utc>`. C# `DateTime.MinValue` / unset becomes `Option::None`. DB columns follow the source schema: `LastUpdated` is **unix seconds (i64)** for aircraft/positions/signals tables, because the original's primary keys depend on second resolution (the positions PK).
- **No secrets in the repo.** Passwords, OAuth secrets and API keys come only from user settings. Never log them. Do not port `getkey.php`/SessionKey retrieval (it needs a registration call that sends call and position to airscout.eu). Settings that the original filled via getkey become user-entered fields.
- **Honest HTTP client.** Always use `as_net::user_agent()`. No browser fingerprint spoofing (the original PublicBinCraft and AirScoutServer send Chrome/Firefox headers). This is a deliberate deviation; ledger it as a ruling in Task 12.
- **Default-off feeds.** A feed that needs credentials, or whose source does not offer the endpoint as a public API, has `enabled_by_default() == false` and a `disclaimer()`. That applies to ADSBSupport, PublicBinCraft and BinCraft (hard-coded AMPRNet IP). OpenSky anonymous, ADSB.one, Dump1090, Readsb, RTL1090, VRS, AirScoutServer and FlexJSON are on by default (still selectable, one feed active at a time like the original).
- **SQL always uses bound parameters**, never string concatenation (the original concatenates).
- Golden tolerance:
  - plane extrapolation and crossing: absolute `1e-9`;
  - ADS-B lat/lon: `1e-9`;
  - integer fields: exact.
- Logic findings continue numbering at **#16**. Each task adds the findings it reproduces. The survey list (crossing #1–#4 → new numbers, cache, checker, plugins, LibADSB) is the source.

## Review Focus

1. **A feed response that is broken or slow.** Cases: HTTP 500, timeout, truncated JSON, a 0-byte binCraft body, wrong zstd magic. Expect: `get_planes` returns `Err`, the worker logs it, sleeps the interval and tries again. No panic, and the cache is untouched. Pinned in Task 10 (worker) and in each feed task (one error test each).
2. **A Beast TCP stream** with `0x1A 0x1A` escapes, frames split across reads, garbage between frames, and the server closing the connection. Expect: no panic and no out-of-bounds. Frames split across reads still decode. The reconnect follows the original 10 s back-off. The C# escape bug is reproduced for frames that contain an escape (#30 analogue). Pinned in Task 14.
3. **Track wraparound and stale data in the cache.** Cases: old track 350° → new 10°, a plane older than the TTL, a time in the future. Expect: the same result as C# (golden), including the wraparound quirk. Pinned in Task 4.
4. **Concurrent cache use.** The feed thread inserts while a reader calls `get_all`. Expect: no data race, and readers get copies. The C# aliasing bug (#7) is **not** reproduced, because it is a memory-safety artefact, not logic; ledger the ruling. Pinned in Task 4 with a two-thread test.
5. **Area and altitude filter edges.** Cases: `max_lat <= min_lat` (worker reports an error and stops, as in C#), a plane exactly on the bbox border, altitude exactly `MinAlt`/`MaxAlt`. Expect: the same accept/skip decision as `PlaneFeed.cs`. Pinned in Task 10.

---

### Task 1: as-aircraft scaffold: PlaneInfo, categories, JSON shapes

**Files:**
- Create: `crates/as-aircraft/{Cargo.toml,src/lib.rs,src/plane.rs,src/category.rs,src/json.rs,src/units.rs}`
- Modify: root `Cargo.toml` (workspace member + `as-aircraft` dep)
- Test: `crates/as-aircraft/tests/plane.rs`

**Interfaces:**
- Produces:
  - `units::{ft_m(f64)->f64, m_ft(f64)->f64, kts_kmh(f64)->f64, kmh_kts(f64)->f64, km_mi(f64)->f64}`. `km_mi` keeps the `* 1.609` bug: log it as a finding with `SupportFunctions.cs:471`.
  - `PlaneCategory { None=0, Light, Medium, Heavy, Superheavy }`, with `from_i32`, `parse(&str)` (unknown → `None`), `as_str()` ("None".."Superheavy") and `short()` (first letter).
  - `PlaneInfo`. Fields: `time: DateTime<Utc>, call, reg, hex: String, lat, lon, alt_ft, track, speed_kts: f64, type_code, manufacturer, model: String, category: PlaneCategory, from, to: String, vspeed: i32`.
    - Result fields: `int_point: Option<GPoint>, int_qrb: f64 (MAX), alt_diff: f64, potential: i32, eps1, eps2, theta1, theta2, angle, squint: f64, signal_strength: f64 (MIN), ambiguous: bool, comment: String`.
    - History fields: `old_time: Option<DateTime<Utc>>, old_lat, old_lon, old_alt_ft, old_track, old_speed_kts: f64`.
    - Methods: `alt_m()`, `speed_kmh()`, and `PlaneInfo::new(time, call, reg, hex, lat, lon, track, alt_ft, speed_kts, type_code, manufacturer, model, category)` (the full C# constructor).
  - `PlaneJson` (serde, PascalCase, every field `Option`, `Time` unix seconds) and `HistJson` (field names exactly as in `HistSON.cs`).
  - `enums::{PathMode, LifeMode, PlayMode, TrackMode}` with the C# discriminants.

- [ ] **Step 1:** Write tests:
  - `units_match_original`: `ft_m(3280.84) == 1000.0` (within 1e-9), `kts_kmh(100.0) == 185.2`, `km_mi(10.0) == 16.09` (the bug, pinned).
  - `category_parse_and_short`: `parse("Heavy") == Heavy`, `parse("xyz") == None`, `Superheavy.short() == "S"`.
  - `plane_json_roundtrip_pascal_case`: deserialize `{"Hex":"3C6444","Call":"DLH1A","Lat":50.1,"Lon":8.2,"Alt":36000,"Track":90,"Speed":450,"Type":"A320","Category":2,"Manufacturer":"Airbus","Model":"A320","Reg":"D-AIZZ","Time":1700000000}`, then serialize and compare as `serde_json::Value`.
  - `new_sets_old_time_none`.
- [ ] **Step 2:** Run `cargo test -p as-aircraft`. Expected: FAIL (crate missing).
- [ ] **Step 3:** Implement. Constants: `ft/3.28084`, `kts*1.852`, `km*1.609`.
- [ ] **Step 4:** Run `cargo test -p as-aircraft`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(as-aircraft): PlaneInfo, categories, JSON shapes`.

### Task 2: AircraftDb (5 tables) + updater

**Files:**
- Create: `crates/as-aircraft/src/db.rs`, `src/updater.rs`
- Create: `tests/fixtures/aircraft/{Aircrafts,AircraftTypes,Airlines,Airports,AircraftRegistrations}.json`. Each holds ≤ 30 real rows cut from the airscout.eu files (2019 data, unencrypted zips), plus the rows the tests name.
- Test: `crates/as-aircraft/tests/db.rs`, `tests/updater.rs`

**Interfaces:**
- Consumes: `as_db::migrate`, `as_net::download_if_newer`.
- Produces:
  - `AircraftDb::{open(dir: &Path), open_in_memory()}`. File `aircrafts.db3`.
  - Row structs: `Aircraft{hex, call, reg, type_code, last_updated}`, `AircraftType{icao, iata, manufacturer, model, category: PlaneCategory, last_updated}`, `Airline{icao, iata, airline, country, ..}`, `Airport{icao, iata, lat, lon, alt, airport, country, ..}`, `AircraftRegistration{prefix, country, remarks, ..}`. `last_updated: DateTime<Utc>`, stored as unix seconds.
  - Per table: `*_insert_or_update_if_newer(&row) -> Result<bool>`, `*_bulk_insert_or_update_if_newer(&[row])` (one transaction), `*_count()`.
  - Finders:
    - `aircraft_find_by_hex/reg/call`: reg and call return the newest row.
    - `aircraft_type_find_by_icao`: newest row.
    - `airline_find_by_icao/iata`.
    - `airport_find_by_icao`.
    - `registration_find_by_reg(&str) -> Option<AircraftRegistration>`: algorithm of `AircraftDatabase.cs:1986-2013`.
  - `registration_min_len()` and `type_icao_min_len()` (cached at open, used by Task 3).
  - `plane_info_bulk_upsert(&[PlaneInfo])`: `AircraftDatabase.cs:2352/2392`. It writes `Aircrafts(hex, call, reg, type)` **and** `AircraftTypes("", type, …)` with `LastUpdated = now`. Reproduce it and log the IATA="" duplicate as a finding.
  - `updater::update_aircraft_db(client, base_url, tmp_dir, db, cancel) -> Result<UpdateStats>`.
    - `DEFAULT_UPDATE_URL = "http://www.airscout.eu/downloads/AirScout/1/AircraftData/"`.
    - Files: `AircraftTypes.zip`, `Airports.zip`, `Aircrafts.zip`, `AircraftRegistrations.zip`, `Airlines.zip`, each containing `<Name>.json`.
    - Empty Call/Reg/TypeCode becomes `"[unknown]"`.
    - Bulk insert when the table is empty, else insert-if-newer.
    - Import only when the zip mtime is newer than the stored table timestamp (same pattern as `as-stations`).

- [ ] **Step 1:** Write tests:
  - `import_fixtures_counts`.
  - `insert_if_newer_keeps_newer`.
  - `find_by_call_returns_newest`.
  - `registration_find`: `"D-AIZZ"` → prefix `"D"`; `"N12345"` → `"N"`, via the `N-` rewrite; `"HB-JZA"` → `"HB"`.
  - `registration_without_dash_is_none_not_panic`: C# throws `ArgumentOutOfRangeException`. Rust returns `None`. Ledger the ruling and log the finding.
  - `sql_injection_inert`: `aircraft_find_by_call("x' OR '1'='1")` is `None`.
  - `updater_imports_zip_from_mock`: mockito serves zipped fixtures, so counts > 0.
  - `updater_skips_when_not_newer`.
  - `live_update` (`#[ignore]`).
- [ ] **Step 2:** Run `cargo test -p as-aircraft`. Expected: FAIL.
- [ ] **Step 3:** Implement. Schema: primary keys as in the survey (`Aircrafts(Hex)`, `AircraftTypes(ICAO, IATA)`, `Airlines(ICAO, IATA)`, `Airports(ICAO, IATA)`, `AircraftRegistrations(Prefix)`).
- [ ] **Step 4:** Run `cargo test -p as-aircraft`. Expected: PASS. Also run `cargo test -p as-aircraft -- --ignored live_update` once. Record the result in the ledger.
- [ ] **Step 5:** Commit `feat(as-aircraft): aircraft database and updater`.

### Task 3: PlaneInfoChecker + PlaneInfoConverter (golden)

**Files:**
- Create: `crates/as-aircraft/src/check.rs`, `src/convert.rs`
- Create: `tools/golden-harness/UpstreamChecker.cs`. It is a verbatim extract of `PlaneInfoChecker.cs` and `PlaneInfoConverter.cs`, with the DB lookups replaced by a fixed in-memory table: airlines ICAO `DLH, BAW, KLM`, IATA `LH, BA, KL`, reg prefixes `D, G, HB, N, PH`, airports `EDDF, EGLL`, type `A320`.
- Modify: `tools/golden-harness/{Program.cs,Cases.cs}`. Write `tests/golden/plane_checker.json`.
- Test: `crates/as-aircraft/tests/check_golden.rs`

**Interfaces:**
- Produces:
  - `trait Lookup { fn airline_icao(&self, &str)->bool; fn airline_iata(&self, &str)->bool; fn reg_prefix(&self, &str)->bool; fn airport_icao(&self, &str)->bool; fn type_icao(&self, &str)->bool; fn reg_prefix_min_len(&self)->usize; fn type_icao_min_len(&self)->usize; }`, implemented for `AircraftDb` and for a test `StaticLookup`.
  - `check::{hex, call, reg, lat, lon, alt, track, speed, type_code}(…, &dyn Lookup) -> bool` and `check::plane(&PlaneInfo) -> bool`.
  - `convert::{to_utc, to_hex, to_reg, to_flight, to_type, to_radar, to_route, to_call, to_squawk, to_lat, to_lon, to_alt, to_track, to_speed}`. Each returns `Option<…>` where C# returns null / `MinValue`. Lat/lon parsing is culture-invariant: harness runs with `InvariantGlobalization`, so `.` is the separator. Log the culture dependence (#15 of the survey) as a finding.

- [ ] **Step 1:** Harness cases.
  - Hex: `"3c6444"`, `"\"3C6444\""`, `"3C644"`, `"GGGGGG"`, `"1000000"`.
  - Call: `"DLH1A"`, `"LH123"` (Type-B, always false: finding), `"D-AIZZ"`, `"[unknown]"`, `"AB"`, `""`.
  - Reg: `"D-AIZZ"`, `"N123AB"`, `"HB-JZA"`, `"X"`.
  - Numeric bounds, each exactly at, inside and outside the limits: alt 0/100000/100001, track 0/359.9/360, speed 0/800/801, `Check(PlaneInfo)` speed 5000.
  - Converter strings: `"1700000000"`, `"2023-11-14 22:13:20"`, route `"EDDF-EGLL"` (finding #14: last char dropped), squawk `"7700"`/`"7800"`.

  Write `plane_checker.json`. Run `tools/golden-harness/run.sh`. Expected: the file exists with > 60 cases.
- [ ] **Step 2:** Write `check_golden.rs`: for every case, the Rust result equals the C# result. Run it. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p as-aircraft --test check_golden`. Expected: PASS. Add findings (Type-B calls, `H` reg mismatch, route substring, culture) to `docs/logic-findings.md`.
- [ ] **Step 5:** Commit `feat(as-aircraft): plane info checker and converter (golden)`.

### Task 4: PlaneInfoCache with extrapolation (golden)

**Files:**
- Create: `crates/as-aircraft/src/cache.rs`
- Modify: `tools/golden-harness/GoldenHarness.csproj`. Link `AirScout.Core/PlaneInfo.cs`, `PlaneCategory.cs`, `AirScout.Aircrafts/PlaneInfoCache.cs`, and the ScoutBase.Core file that holds `UnitConverter` (`SupportFunctions.cs`). If a linked file pulls in WinForms types, use a verbatim extract instead, the way `UpstreamHorizon.cs` does.
- Modify: `Program.cs`, `Cases.cs`. Write `tests/golden/plane_cache.json`.
- Test: `crates/as-aircraft/tests/cache_golden.rs`, `tests/cache.rs`

**Interfaces:**
- Produces:
  - `PlaneInfoCache` (internally `Mutex<BTreeMap<String, PlaneInfo>>`; every method takes `&self`; `Send + Sync`).
  - Methods: `insert_or_update_if_newer(&PlaneInfo) -> bool`, `bulk_insert_or_update_if_newer(&[PlaneInfo]) -> usize`, `delete(&str)`, `len()`, `get(hex, at, ttl_min: i64) -> Option<PlaneInfo>`, `get_all(at, ttl_min) -> Vec<PlaneInfo>`.
  - The cache always stores and returns **copies**.
  - Algorithm: `PlaneInfoCache.cs:14-72` (insert, moves current values into `old_*`), `:122-211` (`Get`, inclusive bounds `0<x<=max`, 45° check without wraparound) and `:213-330` (`GetAll`, strict bounds, always dead-reckons).

- [ ] **Step 1:** Harness sequences, each a list of inserts followed by `Get`/`GetAll` at given times:
  - straight flight (2 reports 60 s apart, query +30 s);
  - turning flight;
  - track wrap 350° → 10°;
  - wrap 10° → 350°;
  - climb;
  - only 1 report (no old_*);
  - report older than TTL;
  - `at` before `Time`;
  - speed 0;
  - alt > 50000.

  Record the full returned PlaneInfo (lat, lon, alt, track, speed, time) or null. Run the harness.
- [ ] **Step 2:** Write the tests:
  - `cache_golden` (every case; lat/lon/alt/track/speed within 1e-9; null equals `None`).
  - `cache_returns_copies` (mutating a returned plane does not change the cache).
  - `cache_concurrent_insert_and_get_all`: two threads, 10 000 operations each, no panic, `len()` consistent.

  Run them. Expected: FAIL.
- [ ] **Step 3:** Implement. Ledger the ruling: "Aliasing (#survey 7) not reproduced: copies instead of shared mutable references."
- [ ] **Step 4:** Run `cargo test -p as-aircraft`. Expected: PASS. Log findings (wraparound, missing locks, aliasing).
- [ ] **Step 5:** Commit `feat(as-aircraft): plane info cache with extrapolation (golden)`.

### Task 5: Crossing prediction `get_nearest_planes` (golden)

**Files:**
- Create: `crates/as-aircraft/src/crossing.rs`
- Create: `tools/golden-harness/UpstreamNearestPlanes.cs`: a verbatim extract of `AircraftDatabase.cs:2477-2650` using the stage-1 `UpstreamPropagationPath`.
- Modify: `Program.cs`, `Cases.cs`. Write `tests/golden/nearest_planes.json`.
- Test: `crates/as-aircraft/tests/crossing_golden.rs`

**Interfaces:**
- Consumes: `as_propagation::PropagationPath::{intersection_point, distance, bearing12, mid_point, params, eps1_min, eps2_min}` and `as_geo::propagation::{epsilon_from_heights, theta_from_heights}`.
- Produces:
  - `crossing::get_nearest_planes(at: DateTime<Utc>, path: &PropagationPath, planes: &mut [PlaneInfo], max_radius_km: f64, max_dist_km: f64, max_alt_m: f64, max_delta_m: f64) -> Vec<PlaneInfo>`.
    - `planes` is `&mut` on purpose. The original dead-reckons the caller's objects after cloning (`AircraftDatabase.cs:2520-2531`), and in MULTI mode the next path sees the mutated positions. Reproduce that exactly and doc-comment it with the finding number.
  - `crossing::nearest_plane(&[PlaneInfo], max_dist_km) -> Option<&PlaneInfo>`: highest potential, then the smallest `int_qrb` among `int_qrb < max_dist` (`MapDlg` logic).
  - `crossing::minutes_to_crossing(&PlaneInfo) -> f64`: `int_qrb / kts_kmh(speed) * 60`.
  - `crossing::merge_active(active: &mut BTreeMap<String, PlaneInfo>, found: Vec<PlaneInfo>)`: the `MapDlg.UpdatePlanes` merge rule.

- [ ] **Step 1:** Harness cases. Take 3 paths from the stage-1 path fixtures (JO50AA↔JO61UF 144 MHz, a long path at ~800 km, one with local obstruction). For each, use these planes:
  - on the path, heading across;
  - parallel to the path;
  - beyond `max_radius`;
  - above `Min_H` (potential 100);
  - below `Min_H` by 500 m;
  - below by 1500 m;
  - far from the path, heading towards it (75/50/0);
  - `track` exactly along `bearing12`;
  - 2 paths in sequence, sharing one plane list (MULTI mutation).

  Use `max_radius ∈ {-1, 0, 50}`, `max_dist = 10`, `max_alt = 12200`, `max_delta = 1000`. Record all result fields, and the caller's plane positions after the call. Run the harness.
- [ ] **Step 2:** Write `crossing_golden`, which compares every result field within 1e-9, plus `potential` and `int_point` presence exactly, plus the post-call caller positions. Also write `nearest_plane_picks_potential_then_qrb` and `merge_active_rules`. Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p as-aircraft --test crossing_golden`. Expected: PASS. Log findings: extrapolation without effect / MULTI mutation, unreachable potential 50, squint from `imin`, theta not set, mismatched alt filters, `MaxSquint`/`MaxElevation` unused.
- [ ] **Step 5:** Commit `feat(as-aircraft): crossing prediction (golden)`.

### Task 6: Aircraft position history DB + maintainer

**Files:**
- Create: `crates/as-aircraft/src/positions.rs`
- Test: `crates/as-aircraft/tests/positions.rs`

**Interfaces:**
- Produces:
  - `PositionDb::{open(dir), open_in_memory()}`. File `aircraftpositions.db3`. Table `AircraftPositions(Hex, Call, Lat, Lon, Alt, Track, Speed, LastUpdated)`. PK `(Hex, LastUpdated)`. Indexes on `Hex` and on `Call`.
  - `AircraftPosition{hex, call, lat, lon, alt_ft, track, speed_kts, last_updated}`.
  - Methods:
    - `insert_or_update_if_newer`;
    - `bulk_insert_plane_infos(&[PlaneInfo], &dyn Lookup) -> Result<usize>` (only rows with valid hex and call);
    - `get_all(range)`, `get_by_hex`, `get_by_call`;
    - `get_all_latest(at, ttl_min)` (`GROUP BY Hex`, max LastUpdated);
    - `oldest()`, `youngest()`;
    - `delete_older_than(DateTime)`, `delete_first(n)` (rowid ASC);
    - `count()`, `file_size_mb()`.
  - `maintain(&PositionDb, &MaintainerConfig{max_count: 10_000_000, max_size_mb: 1500, max_days: 7}, last_max_size: &mut f64) -> Result<MaintainStats>`: the 3 rules of `AircraftPositionDatabaseMaintainer.cs`, including "delete 1000 rows" and the never-decreasing max size.

- [ ] **Step 1:** Write tests:
  - `latest_per_hex`.
  - `same_second_is_one_row`.
  - `maintain_deletes_older_than_7_days`.
  - `maintain_over_count_deletes_1000_oldest` (use `max_count` 10 with 1500 rows; the result is 500).
  - `bulk_insert_skips_invalid_hex`.

  Run them. Expected: FAIL.
- [ ] **Step 2:** Implement. Return a real row count, not the C# constant 0. Return an error with rollback instead of -1 inside an open transaction. This deviation affects only error reporting; ledger it as a ruling. Log the C# behaviour as a finding.
- [ ] **Step 3:** Run `cargo test -p as-aircraft`. Expected: PASS.
- [ ] **Step 4:** Commit `feat(as-aircraft): position history and maintainer`.

### Task 7: as-adsb messages: CRC, Mode-S, extended squitter types (golden)

**Files:**
- Create: `crates/as-adsb/{Cargo.toml,src/lib.rs,src/crc.rs,src/modes.rs,src/msg.rs,src/cpr.rs}`
- Modify: `GoldenHarness.csproj`: link all `LibADSB/*.cs` files except `ADSBDecoder.cs` (Task 8). Modify `Program.cs`/`Cases.cs`. Write `tests/golden/adsb_msgs.json`.
- Test: `crates/as-adsb/tests/msgs_golden.rs`

**Interfaces:**
- Produces:
  - `crc::parity(&[u8]) -> u32` (polynomial `0xFFF409`).
  - `ModeSReply::from_hex(&str) -> Result<ModeSReply, AdsbError>`: 14 or 28 hex characters; fields `df, ca, icao24: u32, payload, parity`; `check_parity()`.
  - `msg::decode(hex) -> Result<Msg, AdsbError>`, where `Msg` is one of:
    - `Identification{icao24, emitter_category, call}` (call not trimmed, as in C#);
    - `AirbornePosition{icao24, tc, odd: bool, cpr_lat: u32, cpr_lon: u32, alt_ft: Option<i32>, …}`;
    - `SurfacePosition{…}`;
    - `VelocityOverGround{icao24, speed_kts, heading_deg: i32, vrate_ms}`;
    - `AirspeedHeading{icao24, airspeed, heading_deg, vrate_ms}`;
    - `Other{df, tc}`.

    Dispatch follows `Decoder.cs`: DF17/18 only; TC 1-4, 5-8, 9-18 and 20-22, 19 subtypes 1/2 and 3/4.
  - `cpr::{nl(lat) -> i32, global(even: &AirbornePosition, odd: &AirbornePosition, newer_is_odd: bool) -> Result<(f64, f64), AdsbError>, local(msg, ref_lat, ref_lon) -> (f64, f64)}`. `local` keeps the C# `(float)` cast in the longitude term; log it as a finding.

- [ ] **Step 1:** Harness cases. Known vectors:
  - `8D4840D6202CC371C32CE0576098` (ident KLM1023);
  - `8D40621D58C382D690C8AC2863A7` + `8D40621D58C386435CC412692AD6` (even/odd pair, expected around 52.2572 / 3.9194, 38000 ft);
  - `8D485020994409940838175B284F` (velocity);
  - `8DA05F219B06B6AF189400CBC33F` (airspeed);

  plus a bad-CRC copy of each, a DF11, a 13-character string, a Gillham altitude, and NL at lat 0 / 86.99 / 87 / 87.01 / -87. Record each decoded field, or the error type. Run the harness.
- [ ] **Step 2:** Write `msgs_golden`. Run it. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p as-adsb`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(as-adsb): Mode-S/ADS-B message decoding (golden)`.

### Task 8: as-adsb stateful decoder + Beast/AVR framing

**Files:**
- Create: `crates/as-adsb/src/decoder.rs`, `src/framing.rs`
- Modify: harness. Link `ADSBDecoder.cs` and replace `DateTime.UtcNow` via a shim clock only if needed. Write `tests/golden/adsb_decoder.json`.
- Test: `crates/as-adsb/tests/decoder_golden.rs`, `tests/framing.rs`

**Interfaces:**
- Produces:
  - `AdsbInfo{timestamp: DateTime<Utc>, icao24: String, call: String, lat: f64, lon: f64, baro_alt_ft: f64, geo_minus_baro_ft: f64, heading: f64, speed_kts: f64}`.
  - `Decoder::new(ttl_min: i64)` and `Decoder::with_clock(ttl_min, Box<dyn Fn() -> DateTime<Utc> + Send>)`.
  - `decode_message(&mut self, raw: &str, ts: DateTime<Utc>, use_geo_alt_only: bool) -> Result<(), AdsbError>`: strips the first and last character, `*…;` framing.
  - `planes(&mut self) -> Vec<AdsbInfo>`: the `GetPlanes` semantics, with the `lastsend` filter.
  - Reproduce the pairing via the clock (≤ 10 s, survey #26), ttl unused (#24), the NaN check bug (#23) and the first-position delay (#27). Each gets a finding.
  - `framing::BeastReader::push(&mut self, bytes: &[u8]) -> Vec<String>`: emits `*HEX;` for types `0x32` (7 bytes) and `0x33` (14 bytes) and skips `0x31`. It buffers partial frames across pushes. Escapes: the C# code does not unescape `0x1A 0x1A`. Reproduce that (frame content as C# would read it) and log it as finding #30.
  - `framing::avr_line(&str) -> Option<String>`: accepts `*…;` and `@<12 hex>…;`.

- [ ] **Step 1:** Harness: feed sequences into `ADSBDecoder` with a fixed fake clock:
  - ident + even + odd + velocity for 2 aircraft;
  - pair 11 s apart (no global decode);
  - local decode after a global one;
  - `GetPlanes` twice.

  Record the outputs. Run the harness.
- [ ] **Step 2:** Write tests:
  - `decoder_golden`.
  - `beast_split_across_pushes` (one frame split at every byte offset gives the same result).
  - `beast_garbage_between_frames`.
  - `beast_escape_reproduces_csharp`.
  - `avr_both_formats`.
  - `beast_truncated_stream_no_panic` (proptest-free: all prefixes of a 3-frame buffer).

  Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p as-adsb`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(as-adsb): stateful decoder and Beast/AVR framing`.

### Task 9: as-feeds core: trait, args, shared helpers, settings

**Files:**
- Create: `crates/as-feeds/{Cargo.toml,src/lib.rs,src/plugin.rs,src/vars.rs,src/wake.rs,src/registry.rs}`
- Modify: `crates/as-settings/src/lib.rs`. Add `feed: String` (default `"OpenSky"`, the key-free default), `planes_interval_s: u64` (60), `planes_position_ttl_min: i64` (5), `planes_keep_history: bool` (false), `planes_extended_plausibility: bool` (true), `planes_max_error_dist: f64` (30), `planes_max_delta_m: f64` (1000) and `feed_settings: toml::Table` (per-feed raw tables). `#[serde(default)]` keeps old files loading.
- Test: `crates/as-feeds/tests/core.rs`, `crates/as-settings/tests/` (extend)

**Interfaces:**
- Produces:
  - `FeedArgs{app_dir, data_dir, tmp_dir: PathBuf, min_lat, max_lat, min_lon, max_lon: f64, min_alt_m, max_alt_m: i32, my_lat, my_lon, dx_lat, dx_lon: f64, keep_history: bool}`.
  - `FeedPlane`. Defaults: `time: None`; numeric fields `f64::MIN`, which mirrors `PlaneFeedPluginPlaneInfo`. Fields: `hex, call, reg, lat, lon, alt_ft, track, speed_kts, type_code, manufacturer, model, country, category: i32, from, to, vspeed, ground: bool`.
  - `trait PlaneFeed: Send { fn name(&self) -> &'static str; fn info(&self) -> &'static str; fn disclaimer(&self) -> Option<&'static str>; fn enabled_by_default(&self) -> bool; fn settings(&self) -> toml::Table; fn load_settings(&mut self, &toml::Table) -> Result<(), FeedError>; fn start(&mut self, &FeedArgs) -> Result<(), FeedError>; fn get_planes(&mut self, &FeedArgs) -> Result<Vec<FeedPlane>, FeedError>; fn stop(&mut self, &FeedArgs) -> Result<(), FeedError>; }`.
  - `FeedError{Http, Timeout, Parse, Io, Config}` (thiserror). The `Display` of an error never contains secrets.
  - `vars::substitute(template: &str, args: &FeedArgs, now: DateTime<Utc>, last_dv: &str) -> String`. Variables: `%APPDIR% %DATADIR% %LOGDIR% %DATABASEDIR% %MINLAT% %MAXLAT% %MINLON% %MAXLON% %MINALTM% %MAXALTM% %MINALTFT% %MAXALTFT% %UNIXTIME% %MYLAT% %MYLON% %MINDISTKM% %MAXDISTKM% %LASTDV%`.
    - Doubles use invariant formatting identical to C# `ToString(CultureInfo.InvariantCulture)` (shortest round-trip).
    - `MINALTFT = (int)(m*3.28084)`.
    - `MAXDISTKM` = max distance from My to the 4 bbox corners. Read `MINDISTKM` from `ADSBOne.cs` VarConverter.
  - `wake::category(emitter: &str, type_code: &str) -> i32`: A1/A2 → 1, A3/A4 → 2, A5 → 3, else 0; `A388` → 4.
  - `registry::all_feeds() -> Vec<Box<dyn PlaneFeed>>` and `registry::feed_by_name(&str)`. Task 9 creates the registry empty; each feed task adds its entry.

- [ ] **Step 1:** Write tests:
  - `vars_substitute_all`: fixed args; compare the full string for the ADSBOne default URL `https://api.adsb.one/v2/point/%MYLAT%/%MYLON%/%MAXDISTKM%` and for the VRS URL.
  - `vars_invariant_decimal_point`.
  - `wake_mapping`.
  - `settings_old_file_still_loads` (a stage-1 TOML without the new keys).
  - `feed_settings_roundtrip`.

  Run them. Expected: FAIL.
- [ ] **Step 2:** Implement.
- [ ] **Step 3:** Run `cargo test -p as-feeds -p as-settings`. Expected: PASS.
- [ ] **Step 4:** Commit `feat(as-feeds): plugin trait, variables, wake categories, settings`.

### Task 10: Feed worker: validation/recovery pipeline + loop

**Files:**
- Create: `crates/as-feeds/src/worker.rs`, `src/pipeline.rs`
- Test: `crates/as-feeds/tests/pipeline.rs`, `tests/worker.rs`

**Interfaces:**
- Consumes: `PlaneInfoCache`, `AircraftDb` (+ `Lookup`), `PositionDb`, `check::*`, `FeedPlane`.
- Produces:
  - `pipeline::process(raw: Vec<FeedPlane>, args: &FeedArgs, cfg: &PipelineConfig, cache: &PlaneInfoCache, db: &AircraftDb) -> Processed{planes: Vec<PlaneInfo>, rejected: Vec<(FeedPlane, Reject)>}`.
    - Follows `PlaneFeed.cs:~200-470` step by step, in survey §5 order: ground skip, hex/reg recovery, lat/lon and bbox, alt recovery and alt window in metres, call/reg recovery to `"[unknown]"`, track, speed recovery, type/manufacturer/model/category from DB, manufacturer prefix strip from model, extended plausibility (track fold ≤ 45°, distance ≤ `max_error_dist * Δmin`, last plane ≤ 300 s, `cache.get(hex, time, 5)`).
    - Reproduce the `"[unkonwn]"` typo (`PlaneFeed.cs:336`) and the track fold (#8). Each gets a finding.
  - `PipelineConfig{extended_plausibility: bool, max_error_dist: f64, log_errors: bool}`.
  - `worker::FeedWorker::spawn(feed: Box<dyn PlaneFeed>, args: FeedArgs, cfg: WorkerConfig, cache: Arc<PlaneInfoCache>, db: Arc<AircraftDb>, positions: Option<Arc<PositionDb>>, events: std::sync::mpsc::Sender<FeedEvent>) -> FeedHandle`.
    - `FeedHandle::{stop(), join()}`; `stop` sets an `AtomicBool`. The sleep is checked once per second, like the C# loop.
    - `FeedEvent::{Started, Planes(Vec<PlaneInfo>), Error(String), Stopped}`.
    - After each cycle: `cache.bulk_insert_or_update_if_newer`, `db.plane_info_bulk_upsert`, positions when `keep_history`, then `Planes` event.
    - Invalid area (`max_lat <= min_lat || max_lon <= min_lon`): send `Error`, then `Stopped`.

- [ ] **Step 1:** Write tests (with a `FakeFeed` returning scripted results):
  - `ground_plane_skipped`.
  - `hex_invalid_recovered_from_reg`.
  - `outside_bbox_skipped_silently`, `on_bbox_border_kept` (inclusive or exclusive exactly as `PlaneFeed.cs`; read the comparison there).
  - `alt_window_edges` (`min_alt_m` exactly kept/skipped as in C#).
  - `missing_call_from_db_then_unknown`.
  - `plausibility_rejects_jump`.
  - `plausibility_track_fold_reproduced` (90° vs 270° passes).
  - `worker_error_then_recovers` (feed `Err` on cycle 1, `Ok` on cycle 2: events `Error`, `Planes`; interval 1 s).
  - `worker_invalid_area_stops`.
  - `worker_stop_within_1s`.

  Run them. Expected: FAIL.
- [ ] **Step 2:** Implement.
- [ ] **Step 3:** Run `cargo test -p as-feeds`. Expected: PASS.
- [ ] **Step 4:** Commit `feat(as-feeds): feed worker and validation pipeline`.

### Task 11: JSON feeds: ADSBOne, Readsb, Dump1090, AirScoutServer, VRS

**Files:**
- Create: `crates/as-feeds/src/feeds/{mod.rs,adsbone.rs,readsb.rs,dump1090.rs,airscoutserver.rs,vrs.rs}`
- Create: `tests/fixtures/feeds/`.
  - `adsbone.json`: record **one** real response from the ADSBOne default URL around JO50, cut to ≤ 25 aircraft. Keep at least one `"alt_baro":"ground"` entry; if none appears, add one by hand and note it in the README.
  - `readsb_aircraft.json` and `dump1090_aircraft.json`: synthetic, following the readsb/dump1090 JSON docs, each with a ground aircraft, an aircraft without position, and every category A1–A5.
  - `airscoutserver_planes.json`: `List<PlaneJson>`.
  - `airscoutserver_fr24.json`: the legacy dictionary branch.
  - `vrs_aircraftlist.json`.
  - `README.md`: source and date per file.
- Modify: harness. Add a verbatim extract of the JSON DTO classes of the Readsb and Dump1090 plugins, run `JsonConvert.DeserializeObject` on the two fixtures, and record the count or the error type in `tests/golden/feed_json.json`. This settles survey #33 with evidence.
- Test: `crates/as-feeds/tests/json_feeds.rs`

**Interfaces:**
- Produces one `PlaneFeed` per module, registered in `registry`. Settings structs carry the C# `[DefaultValue]`s:
  - **ADSBOne**: `url` default as in Task 9. `Time = now_ms - seen_pos`. Split `desc` "MANUF Model". Ground is not set (faithful).
  - **Readsb**: url `http://localhost/tar1090/data/aircraft.json`. Call/Category are not mapped (faithful). The `"ground"` behaviour follows the `feed_json.json` golden: if C# throws, return `Err(Parse)` for the whole response.
  - **Dump1090**: url `http://localhost:8080/data/aircraft.json`. The category chain bug (only A5 survives, #32) and the unmapped `flight` are reproduced.
  - **AirScoutServer**: url `http://localhost:9880/planes.json`. Both branches.
  - **VRS**: `url`, `url2`, `username`, `password`, `username2`, `password2`, `load_share` (true), `use_geo_alt` (false), `timeout_s` (30). Basic auth. Failover uses the URL1 credentials (#38, reproduced). WTC 0-4 maps 1:1. `lastDv` feeds `%LASTDV%`.
  - The settings URL persistence bug (#35, "only loaded if it contains `?`") is **not** reproduced. TOML settings replace the XML loader. Ledger it as a ruling and still log the finding.

- [ ] **Step 1:** Record `adsbone.json` once with `curl -A "AirScout-RS/<version>"`. Write the synthetic fixtures. Run the harness and confirm the result for `feed_json.json`.
- [ ] **Step 2:** Write tests per feed (mockito serving the fixture):
  - `<feed>_maps_fixture`: count, plus the full field check on one named aircraft.
  - `<feed>_http_500_is_err`.
  - `<feed>_truncated_json_is_err`.
  - `vrs_failover_to_url2`.
  - `vrs_basic_auth_header`.
  - `readsb_ground_matches_golden`.

  Add `#[ignore]` live tests for ADSBOne. Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p as-feeds`. Expected: PASS. Log findings #32, #33 (with golden evidence), #35, #36, #38 and #40.
- [ ] **Step 5:** Commit `feat(as-feeds): ADSBOne, Readsb, Dump1090, AirScoutServer, VRS feeds`.

### Task 12: binCraft feeds: BinCraft + PublicBinCraft (golden)

**Files:**
- Create: `crates/as-feeds/src/feeds/{bincraft.rs,publicbincraft.rs,bincraft_parse.rs}`
- Modify: harness. Link or extract `BinCraftParser.cs` (BinCraft plugin) and the PublicBinCraft parser. Build synthetic binCraft buffers in C#: a header plus 3 records, with values chosen so that every field offset is distinguishable (each `s16` field holds a distinct value). Run both C# parsers, and write the base64 input plus the parsed output to `tests/golden/bincraft.json`.
- Test: `crates/as-feeds/tests/bincraft.rs`

**Interfaces:**
- Produces:
  - `bincraft_parse::{parse_bincraft(&[u8]) -> Result<Vec<FeedPlane>, FeedError>, parse_public(&[u8]) -> Result<Vec<FeedPlane>, FeedError>}`. Byte layouts are in survey §6, as implemented in the respective C# parser. Reproduce the suspect BinCraft offsets (track @24, gs @26, seen_pos @54) and log them as a finding.
  - **BinCraft** feed: url `http://44.149.131.185/tar1090/data/aircraft.binCraft.zst`, `compressed` true. zstd via the `zstd` crate. `enabled_by_default` false.
  - **PublicBinCraft** feed: host `https://globe.airplanes.live`, URL `<host>/re-api/?binCraft&zstd&box=minLat,maxLat,minLon,maxLon`, cookie store on. `enabled_by_default` false. Disclaimer: this is not a documented public API, so check the provider's terms.
  - Ruling (ledger): no Chrome header spoofing; `as_net::user_agent()` only.

- [ ] **Step 1:** Run the harness. Expected: `bincraft.json` with 2 parsers × 3 records.
- [ ] **Step 2:** Write tests:
  - `parse_bincraft_golden`, `parse_public_golden`.
  - `zstd_roundtrip_through_feed` (mockito serves `zstd::encode_all(fixture)`).
  - `empty_body_is_err`, `bad_zstd_magic_is_err`, `record_count_exceeds_buffer_is_err` (no panic).
  - `bincraft_disabled_by_default`.

  Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p as-feeds`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(as-feeds): binCraft feeds (golden parser)`.

### Task 13: OpenSky + ADSBSupport

**Files:**
- Create: `crates/as-feeds/src/feeds/{opensky.rs,adsbsupport.rs}`
- Create: `tests/fixtures/feeds/opensky_states.json`: one recorded anonymous `/api/states/all?lamin=…` response cut to ≤ 25 states, including one `on_ground` state and one state with a null callsign. Also create `adsbsupport_planes.json` (synthetic, `{planes:[…]}` shape from survey §6).
- Test: `crates/as-feeds/tests/opensky.rs`, `tests/adsbsupport.rs`

**Interfaces:**
- Produces:
  - **OpenSky**: settings `url` (`https://api.opensky-network.org/api/states/all`), `token_url` (`https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token`), `oauth_client_id`, `oauth_secret` (both empty means anonymous).
    - Token: client-credentials POST, cached until expiry, `Authorization: Bearer`.
    - Mapping per survey §6: velocity `*3.6/1.852`, geo altitude `*3.28084`, skip on_ground and speed ≤ 0.
    - Reproduce: the bbox is not sent (#37: `Settings.URL` used instead of the substituted URL), `Type = field[5]` (longitude), and a null callsign drops the plane. Log each.
  - **ADSBSupport**: settings `url` (default from survey §6), `username` (`airscout`), `password` (empty, user-entered; no getkey), `use_geo_alt` (false), `timeout_s` (30).
    - Basic auth.
    - The `token:` header is sent only when the user sets a token field (`token`, empty default).
    - No 30 s start sleep.
    - Reproduce `Time = UtcNow` (#36).
    - `enabled_by_default` false. Disclaimer: needs credentials from the service operator.
  - `get_planes` with an empty password returns `Err(Config)` without a network call.

- [ ] **Step 1:** Record `opensky_states.json` once (anonymous, small bbox around JO50). Write `adsbsupport_planes.json`.
- [ ] **Step 2:** Write tests:
  - `opensky_maps_fixture`.
  - `opensky_oauth_fetches_and_caches_token` (mockito: token endpoint hit once for 2 cycles; the second request carries the Bearer header).
  - `opensky_401_is_err`.
  - `adsbsupport_basic_auth`.
  - `adsbsupport_no_password_is_config_err`.
  - `secrets_not_in_error_display` (format an error after a 401 and assert the password is absent).
  - `#[ignore]` live OpenSky.

  Run them. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test -p as-feeds`. Expected: PASS.
- [ ] **Step 5:** Commit `feat(as-feeds): OpenSky and ADSBSupport feeds`.

### Task 14: RTL1090 (Beast/AVR over TCP)

**Files:**
- Create: `crates/as-feeds/src/feeds/rtl1090.rs`
- Test: `crates/as-feeds/tests/rtl1090.rs`

**Interfaces:**
- Consumes: `as_adsb::{Decoder, framing::{BeastReader, avr_line}}`.
- Produces the **RTL1090** feed. Settings: `server` (`localhost`), `port` (31001), `binary` (true = Beast), `use_geometric_alt_only` (false), `mark_local` (false).
  - `start` spawns a receiver thread: `TcpStream`, read timeout 1 s, reconnect after 10 s on error/EOF. The thread pushes into a `Decoder` behind a `Mutex`, so there is no unsynchronised read (unlike C# #28).
  - `get_planes` returns `decoder.planes()` mapped to `FeedPlane`:
    - alt = baro, or baro + geo-minus-baro when `use_geometric_alt_only`;
    - reg/type `[unknown]`, category 0;
    - `@` prefix on call when `mark_local`;
    - only alt > 0.
  - `stop` ends the thread within 2 s.
  - The time computation bug (#29, `.Seconds` instead of `TotalSeconds`, local time) is reproduced only if it affects emitted `FeedPlane.time`. Read `RTL1090.cs:560-580`. If it only touches logging, ledger that instead.

- [ ] **Step 1:** Write tests (a `TcpListener` on `127.0.0.1:0` as a fake receiver):
  - `beast_stream_yields_plane` (the Task 7 vectors as Beast frames, split into 3-byte writes).
  - `avr_stream_yields_plane`.
  - `server_closes_then_reconnects` (accept, close, accept again; the plane still arrives; uses a test-only back-off override of 100 ms).
  - `stop_is_prompt`.

  Run them. Expected: FAIL.
- [ ] **Step 2:** Implement. The back-off is a field with default 10 s, overridable through a `#[doc(hidden)]` constructor for tests.
- [ ] **Step 3:** Run `cargo test -p as-feeds`. Expected: PASS.
- [ ] **Step 4:** Commit `feat(as-feeds): RTL1090 Beast/AVR TCP feed`.

### Task 15: FlexJSON

**Files:**
- Create: `crates/as-feeds/src/feeds/flexjson.rs`
- Create: `tests/fixtures/feeds/flexjson_{vrs_like,array_of_arrays,nested}.json` (synthetic; one shape per heuristic path)
- Test: `crates/as-feeds/tests/flexjson.rs`

**Interfaces:**
- Produces the **FlexJSON** feed. Settings: `url` (""), `timeout_s` (30), `units_alt` ("ft"), `units_speed` ("kts"), `min_elements` (10), `min_planes` (50), and `index_hex, index_utc, index_type, index_lat, index_lon, index_alt, index_track, index_speed, index_reg` (all -1, persisted after detection).
  - The algorithm follows `FlexJSON.cs`: flatten, most common array length, column auto-detect with a 90 % threshold.
  - Reproduce: `index > 0` excludes column 0, the `"m "` / `"km/h "` trailing-space comparisons, and integer-only parsing (#39). Log them.
  - Empty `url` returns `Err(Config)`.

- [ ] **Step 1:** Write tests:
  - `detects_columns_<shape>` for each fixture (expected indices derived by hand from the C# code; note the derivation in a test comment).
  - `column_zero_never_detected` (pins #39).
  - `empty_url_is_config_err`.
  - `below_min_planes_returns_empty`.

  Run them. Expected: FAIL.
- [ ] **Step 2:** Implement.
- [ ] **Step 3:** Run `cargo test -p as-feeds`. Expected: PASS.
- [ ] **Step 4:** Commit `feat(as-feeds): FlexJSON heuristic feed`.

### Task 16: as-signals

**Files:**
- Create: `crates/as-signals/{Cargo.toml,src/lib.rs}`
- Test: `crates/as-signals/tests/signals.rs`

**Interfaces:**
- Produces:
  - `SignalDb::{open(dir), open_in_memory()}`. File `signals.db3`. Table `SignalLevels(Level REAL, LastUpdated INTEGER PRIMARY KEY)` in unix seconds.
  - Methods: `insert_or_update_if_newer(level, at)`, `bulk_insert(&[(f64, DateTime<Utc>)])`, `find(at) -> Option<f64>` (exact second), `delete_older_than`, `count`.
  - `speclab_url(base: &str, file: &str, f1: f64, f2: f64) -> String`: reproduces the `?f1=…?f2=…` bug from `MapDlg.cs:7671`; log it.
  - `parse_speclab_fft(&str) -> Result<(DateTime<Utc>, f64), SignalError>`: max of the bins, header unix time. Read `MapDlg.cs:7600-7709` for the JSON shape. The poller itself lives in the stage-3 GUI.

- [ ] **Step 1:** Write tests:
  - `find_exact_second_only`.
  - `bulk_insert_and_count`.
  - `speclab_url_reproduces_bug`.
  - `parse_fft_max` (a synthetic SpecLab JSON written from the C# parser's field names).

  Run them. Expected: FAIL.
- [ ] **Step 2:** Implement.
- [ ] **Step 3:** Run `cargo test -p as-signals`. Expected: PASS.
- [ ] **Step 4:** Commit `feat(as-signals): signal level database`.

### Task 17: CLI smoke commands + docs + CI

**Files:**
- Modify: `crates/as-cli/src/main.rs`, `crates/as-cli/Cargo.toml`
- Modify: `docs/logic-findings.md` (check that every finding from Tasks 1–16 is present and numbered), `README.md` (stage-2 usage)
- Modify: `.github/workflows/*.yml` only if new crates need anything (zstd builds from source with `cc`; check Windows)
- Test: `crates/as-cli/tests/cli.rs` (extend)

**Interfaces:**
- Consumes everything above.
- Produces these subcommands:
  - `airscout-cli aircraft-update --data <dir> [--base-url]`: runs Task 2's updater and prints the counts as JSON.
  - `airscout-cli planes --feed <name> --area <minlat,minlon,maxlat,maxlon> --data <dir> [--cycles N] [--feed-setting key=value ...]`: runs the worker for N cycles (default 1) and prints the processed PlaneInfo list as JSON.
  - `airscout-cli nearest --from <loc> --to <loc> --planes <file.json> --data <dir> [path flags as stage 1]`: builds the path (stage 1), runs `get_nearest_planes` on a `PlaneJson` list, and prints potential, int_qrb and minutes per plane.
  - `airscout-cli feeds`: lists the feeds with enabled-by-default and disclaimer.

- [ ] **Step 1:** Write CLI tests:
  - `feeds_lists_all_11`.
  - `planes_from_mock_readsb` (mockito; `--feed-setting url=<mock>`).
  - `nearest_on_fixture_path` (reuses the stage-1 fixture data dir and a planes JSON with one plane crossing at potential 100).
  - `aircraft_update_from_mock`.

  Run them. Expected: FAIL.
- [ ] **Step 2:** Implement.
- [ ] **Step 3:** Run the full gate on macOS: `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked`. Then run the Linux docker command from the stage-1 ledger. Expected: both green. Windows is checked by CI once pushed (push only when the user asks).
- [ ] **Step 4:** Commit `feat(as-cli): stage-2 smoke commands (aircraft-update, planes, nearest, feeds)`.

---

## Notes for the executor

- Harness changes accumulate in Tasks 3, 4, 5, 7, 8, 11 and 12. After each, run `tools/golden-harness/run.sh` (docker) and commit the regenerated fixture together with the task.
- "Reproduce" always means: implement the C# behaviour, add a test that pins it, and add a `docs/logic-findings.md` row. "Not reproduced" always needs a `Ruling:` line in the ledger.
- Recording live responses (Tasks 11 and 13) is one request each, with our User-Agent. If the service is down or blocks the request, write a synthetic fixture from the documented format, mark it in `tests/fixtures/feeds/README.md`, and ledger it.
