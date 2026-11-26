# SDD ledger — plan: docs/superpowers/plans/2026-11-24-stage5a-dev-tools.md

Branch stage5-suite-tools from 40f973e; plan commit 5a9e6f5. Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md; survey + owner decisions docs/superpowers/surveys/2026-11-24-suite-tools-survey.md (d9b5e58).
Owner: use a little Gemini 4 Neon (antigravity) as a full council member on a few reviews.

## Pre-flight
Pre-flight: 14 rows, 9 conflicts — table and rulings in preflight.md (copied into the final ledger).
Ruling: pre-flight C1-C9 adopted as proposed in preflight.md (each with its cost line there) — they follow the real APIs, the C# source and the rubric; notable: C1 horizon-gen runs check_area (no download) per station and sets COMPLETE so "incomplete" means data missing; C6 BNONE test failure = stop and report, never rewrite; C9 the settings parse-error leak fix is named in the Task 4 commit body, no logic finding.

Task 1: dispatched implementer (sonnet, Locator_* members), BASE 5a9e6f5
Task 1: DONE 27784f2 (1132 pass, RED seen); review dispatched (sonnet)
Task 1: review clean (spec ✅, quality approved); C# ViewClient reads all three at MapViewDlg.cs:672-674 (missing key crashed it)
Task 1: minor (deferred): 3 cosmetic minors (i32→f64 via f64::from; rustfmt expansion stations.rs:131; test placement web_api.rs:210)
Task 1: complete (commits 5a9e6f5..27784f2, review clean)
Task 2: dispatched implementer (sonnet, wintest-sim + bind_shared), BASE 27784f2
Task 2: DONE_WITH_CONCERNS cc59938 (9 new tests, gate ok, WINGNU_OK). Concerns: RED was compile failure; echo-suppression test weak; Windows ConnectionReset skip untested; as-stations dep unused until T3; main.rs 609 lines
Task 2: review clean (spec ✅, quality approved); app receiver socket unchanged (same calls, order, cfg, 250 ms timeout)
Task 2: minor (deferred): --listen 3601 case would run 3601 s on port 9872 if validation regressed; Windows ConnectionReset arm untested; as-stations dep unused until T3; echo filter ignores source IP (README note); no MAX_RECEIVED test; unreachable encode error; listen tests 300 ms start / 1.5 s window may flake on slow CI
Task 2: complete (commits 27784f2..cc59938, review clean)
Task 3: dispatched implementer (opus, horizon-gen), BASE cc59938 — C1, C2, C3, C6, C7 rulings passed
Task 3: NEEDS_CONTEXT (uncommitted) — outside_elevation_data_is_listed_incomplete fails: as_elevation::get_locs_from_rect (1:1 of C# GetLocsFromRect, ElevationDatabase.cs:580-611) samples from the min corner in 1°/2° steps, so an area narrower than a step that crosses a square boundary yields only the corner square; same gap in prepare_area (app, path/horizon subcommands). bnone_pass_takes_no_point passes (C6 not triggered).
Task 3: Ruling: option A — horizon-gen splits the area at the 2°×1° square grid and runs check_area per piece; as-elevation stays 1:1 (stage 6 fixes the library); Task 5 logs the C# GetLocsFromRect gap as a "Ported 1:1" finding naming prepare_area/app/path/horizon and the horizon-gen workaround as a "Port deviation" — tool output should say "incomplete" when data is missing, and the library change belongs to the measurable stage 6 — cost if wrong: ~15 lines duplicate the grid logic until stage 6.
Task 3: DONE 4a34b27 (1150 pass); split helper tests written with code (RED via outside_elevation_data_is_listed_incomplete). Review dispatched (Gemini 4 Neon, council seat)
Task 3: review (Gemini 4 Neon) — spec ✅, Approved, 8/10 (score recorded). Two Important are the plan-mandated C6 tests (bnone_pass_takes_no_point guards the library, columns_match_library mirrors the loop).
Task 3: Ruling: the two C6 Important items stay as ruled in pre-flight C6 — no C# oracle exists; bnone passed, so finding 1 stands — cost if wrong: a shared-loop bug passes both copies.
Task 3: minor (deferred): case-sensitive file-name dedup (TE1ST_JO61AA vs TE1ST_jo61aa overwrite on APFS/NTFS); square_pieces untested for negative/boundary/pole cases, check_args rejections, failure continuation, COMPLETE reset; per-square stderr lines and sea_file re-read per square; lon floor simplification; cite :580-611 ends at :612
Task 3: complete (commits cc59938..4a34b27, review clean)
Task 4: dispatched implementer (opus, catcheck + cat_config move + SettingsError::Parse), BASE 4a34b27 — C4, C5, C9 passed; secret-not-in-error test required
Task 4: DONE_WITH_CONCERNS 0386dd4 (1169 pass/5 ignored; hamlib + as-cat clippy clean; WINGNU_OK). Concerns: catcheck main.rs 499 lines; brief extras (clap exit 1, rig line after connect, interval 100-10000 ms, list-ports no settings, read-back after set); stale comment as-settings/src/cat.rs:1-3; omnirig test windows-only; hamlib tests cannot link locally
Task 4: review (opus) — spec ❌ only for I1 (Linux docker gate not run for Task 4); quality approved; no credential path; app CAT mapping identical; doppler-test restore holds except Ctrl-C; extras accepted
Task 4: Ruling: I1 not a fix round — the Linux docker gate (incl. hamlib clippy with -p catcheck and catcheck/hamlib, C5) runs in Task 5 over the whole branch, which covers the new catcheck tests on Linux before any merge — cost if wrong: a Linux-only failure surfaces one task later and is fixed in Task 5.
Task 4: minor (deferred): rig error text not read on a rejected set; read-back offline after a successful set exits 2 ("nothing set"); extras untested (usage exit 1, interval range, list-ports with broken settings); restored: no path untested; dead Cmd::ListPorts arm in rig_cmd; list_ports_does_not_panic cannot fail (plan-mandated); stale comment as-settings/src/cat.rs:3; line_column no multi-byte test; omnirig test runs only in hosted Windows CI
Task 4: complete (commits 4a34b27..0386dd4, 1 ruled)
Task 5: dispatched implementer (opus, docs/findings/all gates incl. Linux docker), BASE 0386dd4 — extra findings GetLocsFromRect (1:1) and horizon-gen split (deviation); stale cat.rs comment fix
Task 5: DONE_WITH_CONCERNS 0969645 (findings #278-#290; macOS 1169/5 ignored; hamlib both clean; Linux docker LINUX_OK 1169; WINGNU_OK). README text corrected to code (wintest-sim --listen sends from the listening socket; --serial-port; ASSETPATH data starts with frequency). README.md:293 example 192.168.1.10 checked by controller: not the upstream private address (generic example since ec53d50).
Task 5: review clean (spec ✅, quality ✅; ~30 cites checked; binaries run against README examples)
Task 5: minor (deferred): #289 should say "with precision 2" (C# default precision 3, ElevationDatabase.cs:580); #278 "every angle is NaN" → "no point is taken"; #283 cite :1501 is the guarding if (acceptable)
Task 5: complete (commits 0386dd4..0969645, review clean)
Final: whole-branch review dispatched (opus), range 40f973e..0969645
Final: Opus review — with fixes; I1 hosted windows-latest never ran (branch not pushed); I2 README manual rig-restore recipe after Ctrl-C wrong for a saved split state (set_tx_frequency with split off writes the main VFO, as-cat/src/rig.rs:152-161); 7 Minor. Agrees with all rulings.
Final: Ruling: I1 handled as in 4c/4d — the hosted Windows job runs after the owner-approved push; a failure there is fixed before the stage counts as done — pushing is the owner's call — cost if wrong: a Windows-only failure is found after the merge.
Final: Ruling: fix wave = I2 (correct README restore order for split and simplex saved states; no Ctrl-C handler, no new crate), M1 case-insensitive file-name dedup in horizon-gen (test-first), M2 README note: use the default broadcast with --listen, M5 wording of #278, #283, #289 — cost if wrong: one small fix wave.
Final: minor (deferred): M4 read-back not ONLINE after a successful set exits 2; M6 rigctld timeout tests assert < 5 s / < 10 s not timeout + 1 s; M7 dead Cmd::ListPorts arm (main.rs:236)
Final: fix wave DONE 378c8f2 (file_names_differing_in_case_do_not_collide RED→GREEN; gate green). Controller verified #289 claim: every port caller uses precision 2 (catalogue.rs:73, prepare.rs:39, horizon_gen.rs:315).
Final: scoped re-review dispatched (sonnet)
Final: re-review — I2, M1, M2, M5 all ADDRESSED; no new Critical/Important
Final: minor (deferred): README has no restore recipe for a saved split=NONE (read failed)
Final: clean — branch ready (hosted Windows CI pending the push, Ruling above)

## Pre-flight scan and rulings (full)

# Pre-flight scan: stage 5a plan

Plan: `docs/superpowers/plans/2026-11-24-stage5a-dev-tools.md` (5a9e6f5). Spec: `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`. Owner decisions Q4, Q5, Q6 and Q10 (survey, binding). Method: SDD `references/preflight.md`; rubric: `task-reviewer-prompt.md`.

## What was checked

- **Interfaces against `crates/*`.** Every consumed signature exists as the plan names it:
  - `as_wintest::{WtKind::parse (lib.rs:64), wire_name, WtMessage, encode (:112), decode (:140), Decoded, DecodeError}` and `as_wintest::dotnet::fixed(x, decimals)` (dotnet.rs:58; it writes `-Infinity` and `NaN` for non-finite values).
  - `airscout::wintest_service::bind_socket` is at `:95-114`; `use socket2` is at `:16`. `socket2` is used nowhere else in `crates/airscout` (src or tests), so the plan can drop it.
  - `StationDb::open`, `location_get_all(None)` ordered by call (db.rs:210), `Location::new(call, loc, lat, lon, GeoSource, hits, DateTime)`. The call is upper-cased and trimmed (`normalise_call`).
  - `ElevationStore::open`, `elevation(model, lat, lon, set_invalid_to_zero)`, `create_path_from_bearing(.., save, cancel)` (path.rs:258), `ElevationPath.{path, valid}`, `default_step_width_m`.
  - `epsilon_max_from_path` (horizon.rs:117). `HorizonPoint::default()` is `eps_min = f64::MIN`.
  - `Band::{all_except_none_and_all, to_ghz, to_mhz, name}`, `default_band_table()`, `EARTH_RADIUS_KM`, and `crate::{Res, parse_model}` (private in main.rs, reachable from child modules).
  - `as_cat::{Rig (rig.rs:8-24; RIT on/off only, :20), CatWorker::{new, pass, finish}, DopplerTarget, CatEvent, make_rig, factory, hamlib_available, omnirig_available, Engine, CatConfig, MAX_FREQ_HZ, RigMode::name, RigSplit, RigRit, RigStatus::name, RigIdent.rig_type}`. The rigctld ident is `[rigctld] host:port` (rigctld.rs:129). The NOCAT text is `rigctld host:port: e` (:143).
  - `cat_service.rs:23-89` is exactly `MIN_MS`/`MAX_MS`, `parity`, `stop_bits`, `flow` and `cat_config`.
  - `as_settings::{CatEngine, Cat fields, Settings::default_path, load_or_default}`; lib.rs:19 is `pub use gui::…`; `SettingsError` is at `:36-51`; `load_or_default` is at `:370-394`.
  - `as_web::Num` serialises 10.0 as `10.0`. `SETTINGS_MEMBERS` is `[&str; 40]` at web_api.rs:56-97. `stations.rs:14-15` and `:127-136`, and `web_settings_json.rs:172-174` and `:248`, match.
  - 4b fake `crates/as-cat/tests/common/fake_rigctld.rs` has `start`, `set_behaviour(Behaviour::Silent)`, `state`, `lines`, `accepts` and `RigState { split, split_freq, .. }`, and it is `#![allow(dead_code)]`. The path `../../as-cat/tests/common/fake_rigctld.rs` resolves from `crates/catcheck/tests/`.
  - `serial2::SerialPort::available_ports() -> io::Result<Vec<PathBuf>>` (serial2 0.2.39).
- **Commands.** The binary is `airscout-cli` (`[[bin]]` in `crates/as-cli/Cargo.toml`). Workspace members are `crates/*`, so `crates/catcheck` joins without a root edit. These exist: features `as-cat/hamlib` and `airscout/hamlib`; test targets `web_api`, `wintest_service`, `integrations`, `ui_stations`, `cat_service` and `app::corrupt_settings_are_not_overwritten`. In assert_cmd 2.2.2, `Command::cargo_bin` and `assert_cmd::cargo::cargo_bin` are not deprecated, so clippy `-D warnings` passes. `docs/logic-findings.md` ends at #277, so #278 is next. The spec rows are at `:85` (generators), `:86` (rainscout), `:87` (wintest-sim) and `:88` (catcheck).
- **Behaviour traces.**
  - Silent rigctld: `get_freq` gives `Timeout`, so `CatRig::get_status` gives `Offline` with no error text, and the line is exactly `OFFLINE`.
  - Refused port: `NoCat`, so the line is `NOCAT rigctld 127.0.0.1:P: …`.
  - Doppler A pass: enter, then `set_split_mode`, which sends `S 1`, `F rx` and `I tx`. Leave restores mode, split, RIT, RX and TX, so `restored: yes` is reachable.
  - BNONE pass: `f1_radius(0, ..)` is `inf` or `NaN`, and so is `asin(NaN)`. No point is taken and there is no cut, as the plan says.
  - `--timeout-ms 200` survives the clamp (`MIN_MS` = 100).
  - `TE1ST/P`, `TE1ST_P` and `../TE1ST` sort to `../TE1ST`, `TE1ST/P`, `TE1ST_P`. The expected names follow from that order.
- **Leak claim (real).** A scratch run of toml 0.8.23 (the version in the lock) showed that `Display` quotes the broken line:
  - `TOML parse error at line 2, column 24 | 2 | secret = "SECRET-TEST-2 | ^ invalid basic string`;
  - `line 4 … client_secret = "SECRET-TEST-1`;
  - a type error, `invalid type: integer \`5\``, also quotes values.
  - `span()` gives `Some(41..41)` and `Some(79..79)`, so the tests' `line 2` and `line 4` hold.
- **Leak display sites.** One site builds the error: `Settings::load_or_default` (both `toml::from_str` calls, lib.rs:385 and :388, use the one `parse_err` closure). One site formats it: `app/mod.rs:98-101` (`e.to_string()`). One site shows it: `ui/status.rs:13-19`, in the status bar. `app/mod.rs:761` and `dialogs.rs:287` only test `is_some()`. `as-cli` never loads a settings file, and no other code parses the settings file. Task 4's change in the variant therefore covers every place the text is shown, `Debug` included, because the toml error is no longer stored.
- **C# cites (31 checked against `target/upstream/AirScout` @ 91b02cb, all correct):**
  - `Settings.Designer.cs:1496`, `:1508`, `:1520`; `Settings.settings:368-376`, `:369`, `:372`; `MapViewDlg.cs:672-674`;
  - `WinTestSimulator/MainDlg.cs:22-41`, `:24-25`, `:25`, `:30`, `:32-36`, `:37-38`; `WinTestSimulator/Properties/Settings.settings:5-19`;
  - `WinTest.cs:105-148`, `:113-114`, `:118`, `:128-129`; `MapDlg.cs:7032`;
  - `HorizonGenerator/MainDlg.cs:36-100`, `:45-52`, `:47`, `:52`, `:61`, `:62`, `:64-73`, `:66-70`, `:71`, `:78`, `:80` (typo `eps[144M[deg]` present), `:86`, `:95-98`;
  - `PropagationHorizon.cs:153-185`, `:173-180`, `:178`; `ElevationDatabase.cs:738-744`, `:776-787`, `:1365-1369`, `:1368`, `:1397`, `:1407`, `:1501`; `StationDatabase.cs:633-643`;
  - `CATCheck/MainDlg.cs:391-398`, `:676-686`, `:685`, `:688-699`, `:697`, `:817-845`;
  - `Band.cs`: `BNONE = 0`, `BALL = 999999999`, so the loop makes 14 passes.
  - `CATCheck/MainDlg.Designer.cs:1200` (`ClientSize`) and `:1209` (title) exist but say nothing about the interface. This is harmless.
- **Ledger copy.** `global-constraints.md` is identical to the plan's Global Constraints.

## Table

| # | Tasks | Produces vs consumes / self-consistency | Finding |
|---|-------|------------------------------------------|---------|
| 1 | T1 | Constants `LOCATOR_MAX_LENGTH`/`LOCATOR_AUTO_LENGTH` (gui.rs, re-export lib.rs:19) are consumed by `stations.rs` (const `/ 2` = 5, unchanged) and `web_settings_json.rs`. The three fields go after `:174`. The test array goes from 40 to 43 in the same order. `Num(10.0)` gives `10.0`, so `"Locator_MaxLength": 10.0,` holds. `loc_from_lat_lon`'s 5th argument is `autolength`. | Clean. |
| 2 | T2 | `bind_shared` is consumed by `bind_socket` (same signature, then read timeout) and by `wintest_sim::run`. Tests: `encode`/`decode` round trips hold. `--msg none` and `--msg 7` give `WtKind::None`/`Unknown`, so the run errors. `252` gives `AsSetPath`. Garbage gives `Empty` or `NoNameSeparator`, never a panic. 9000 B is below the macOS `maxdgram` of 9216. | Clean. Nits: `bind_shared` takes `SocketAddr`, so `run` needs `SocketAddr::from((a.bind, a.port))` and the test needs `"127.0.0.1:P".parse()`. The error-path tests (`bad_port_and_listen_rejected`, `unknown_msg_is_error`) should also pass `--addr 127.0.0.1 --bind 127.0.0.1`, as the Global Constraint says for "every" test. |
| 3 | T3 | `horizon_rows` uses `create_path_from_bearing`, whose `valid` is the store's in-memory `COMPLETE` bit (path.rs:174-179, `DbStatus::UNDEFINED` at open). `horizon-gen` never sets that bit (no `prepare_area`/`check_area`), so **every** path is invalid and every station is listed `incomplete`. `outside_elevation_data_is_listed_incomplete` passes anyway, and no test asserts an empty `incomplete`. | **C1**. Also **C2** (Review Focus 5 wording), **C3** (`--stations te2nd` needs a case-insensitive match the plan does not state), **C6** (plan-mandated tests), **C7** (C# `short` h1). |
| 4 | T4 | `SettingsError::Parse {path, line, column}` is consumed by catcheck stderr and by the app's `load_settings` (unchanged; `corrupt_settings_are_not_overwritten` only checks that `settings.toml` is in the text, which still holds). `cat_config` moves with clamps; `pub use` keeps `airscout::cat_service::cat_config` for `tests/cat_service.rs`. The leftover imports (`Engine`, `SerialConfig`, `CatEngine`) in cat_service.rs must go, or clippy fails. The traces above confirm every exact output line in the tests. | **C4** (the "before the rig is opened" text contradicts doppler-test steps 2-3). **C5** (the hamlib gate never builds `catcheck --features hamlib`). Interface cites `Designer.cs:1200/:1209` are irrelevant but correct. |
| 5 | T5 | README, spec and findings #278 to #288 (11 items, in order). Finding 1 depends on the outcome of T3 `bnone_pass_takes_no_point` (handled). The spec replacement names `:85-88`, but `:86` is the rainscout row. | **C8**. No finding is planned for the app leak; see C9. |
| 6 | T1 × T4 | Both edit `crates/as-settings/src/lib.rs`: T1 at `:19` (one `use` line, no line shift), T4 at `:36-51` and `:370-394`. | No conflict. If T1 adds lines, T4's line cites shift, but the item names are unambiguous. |
| 7 | T1 × T5 | Finding 11 describes T1: order after `Planes_Filter_Max_Circumcircle`, `10.0`, `true`, the setting value. | Consistent. |
| 8 | T2 × T3 | Both edit `as-cli/src/main.rs`, `as-cli/Cargo.toml` and `Cargo.lock`. T3 consumes `as_wintest::dotnet::fixed` through the `as-wintest` edge that T2 adds, so T3's "no `+name`" lock check holds only after T2. The line budget is 604 + about 8 + about 8, which is ≤ 620. T3 moves `fixture_data_dir` into `tests/common/mod.rs`; `cli.rs` then loses imports that only that function used. | Sequential order T2 before T3 is required (the plan order). Clean. |
| 9 | T2 × T4 | Both edit `crates/airscout/Cargo.toml` (T2 drops `socket2`; T4 adds `features = ["settings"]` to `as-cat`) and `Cargo.lock`. | No conflict. |
| 10 | T3 × T4 | Both edit `Cargo.lock` (T3 adds the `as-stations` edge; T4 adds `catcheck` and the `as-cat` to `as-settings` edge). `as-settings` depends on `as-geo`, `as-elevation`, `as-aircraft` and `as-stations`, none of which depends on `as-cat`, so there is no cycle. | Clean. |
| 11 | T2 × T5 | Findings 8 and 9 and the README socket rows match T2: `SO_BROADCAST` on send, address reuse only with `--listen`, `--listen` 1-3600 s. | Consistent. |
| 12 | T3 × T5 | Findings 1-7 match T3: 12 columns, typo fixed, one file per row, safe and unique names, exit 1 on a failed station, no save, invariant `.` and `\n`, the shared cut path 1:1. | Consistent, except that C1 changes what "incomplete" means in the README. |
| 13 | T4 × T5 | Finding 10, the README catcheck examples (`localhost`, no LAN address) and `cargo build -p catcheck --features hamlib` (the feature exists after T4) match T4. The privacy line holds: catcheck prints only the ident, the rig state and the status or error text. | Consistent. The plan text calls the status bar the "control panel" (C9). |
| 14 | Plan-wide | The Global Constraints match the tasks; the network rules hold (no HTTP in 5a, which the C1 ruling keeps). Review Focus items 1-4 map to tests that can fail. Review Focus 5 claims "a horizon of zeros" (C2). | See C1 and C2. |

## Conflicts and proposed rulings

**C1. Important. `horizon-gen` reports every station `incomplete`, and no test catches it.**
- What happens: `create_path_from_bearing` takes `valid` from the store's in-memory `COMPLETE` bit. A freshly opened store is `UNDEFINED`, and the plan never sets the bit.
- Result: every path is invalid and every CSV is listed under `incomplete`. `outside_elevation_data_is_listed_incomplete` passes trivially.
- Ruling: per station, call `as_elevation::check_area` (no download, no HTTP client):
  - Area: `crate::bbox` of the station plus its 360 destination points at `--dist`, as `horizon_cmd` does.
  - Sea file: `<data>/<model in lower case>-sea-squares.txt`, the same name as `crate::sea_file`.
  - Then call `store.set_bounds(rect)`, and set or reset `DbStatus::COMPLETE` from `st.complete`. This is `prepare_area` without a client.
  - Add `assert_eq!(v["incomplete"], json!([]))` to `writes_one_csv_per_station`.
- Why: the validity report must mean "elevation data missing", not "never checked". The scan only reads.
- Cost if wrong: one scan of a few squares per station. A wrong scan only mislabels `incomplete`. The CSV numbers do not change, because `valid` is only reported.

**C2. Minor. Review Focus 5 wording is wrong.**
- What happens: missing tiles sample as elevation 0, because `create_path` maps `TILE_MISSING` to 0.
- Result: the horizon is that of sea-level ground, which gives negative angles, not zeros.
- Ruling: change the wording in the ledger's review focus to "an elevation path of zeros (sea level), 1:1, listed under `incomplete`". No code change, and no value assertion is added.
- Why: reviewers must not flag correct non-zero angles.
- Cost if wrong: none.

**C3. Minor. The `--stations` call match is underspecified.**
- What happens: the test passes `--stations te2nd` and expects `TE2ND`. The plan only says "`all` in any case".
- Ruling: compare the trimmed, upper-cased argument with the stored call. The call is stored upper-cased (`normalise_call`). The error message quotes the argument as given.
- Why: the test requires it, and it is the same rule the database applies.
- Cost if wrong: none; a lower-case call simply works.

**C4. Minor. Task 4 contradicts itself.**
- What happens: the plan says "every Doppler step goes through `check_freq` before the rig is opened". Doppler-test steps 2-3 need the saved RX from an open rig when `--base-hz` is not given.
- Ruling: the step order wins.
  - With `--base-hz`, check every step before any connect. The test expects `accepts() == 0`.
  - Without it, the rig is opened and read first. Every step is checked before the first set command. The test expects no `F`/`I`/`S`/`M`/`X`/`U` line.
  - Set commands with an argument are always checked before any connect.
- Why: the tests already encode this. The rig-safety rule concerns writes, not reads.
- Cost if wrong: none.

**C5. Minor. The hamlib gate never builds catcheck with hamlib.**
- What happens: the hamlib feature check (`-p as-cat -p airscout`) never builds `catcheck --features hamlib`. No gate builds `as-cat` without `settings`: workspace feature unification turns it on.
- Ruling: in Tasks 4 and 5, extend the hamlib check to `-p as-cat -p airscout -p catcheck --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib`, on macOS and in the Linux docker. Add one `cargo clippy -p as-cat --all-targets --locked -- -D warnings`, which builds `as-cat` without `settings`.
- Why: both feature edges are new in 5a, and a `cfg` slip would otherwise ship unbuilt.
- Cost if wrong: two short extra clippy runs.

**C6. Important, plan-mandated. Two Task 3 tests are weak by design.**
- `bnone_pass_takes_no_point`: the plan says "if it fails, keep the test with the observed values". A test rewritten to its observed output cannot fail. My trace supports the plan's expectation (NaN, no point, no cut).
- `columns_match_library`: the test re-implements the shared-path loop from `horizon_rows`. This is duplicated logic: a bug in that loop passes both copies.
- Ruling: keep both tests. There is no independent C# oracle, because HorizonGenerator does not build (finding 3), and the bin crate exposes no library. Two changes:
  - If `bnone_pass_takes_no_point` fails, the implementer stops and reports to the controller (DONE_WITH_CONCERNS). It does not rewrite the expectation. The controller then rules on finding 1.
  - `columns_match_library` gets a doc line: it guards the CSV wiring and format (band order, F1 per band, degree conversion, `fixed(…, 8)`), not the maths.
- Why: the rubric treats both patterns as defects. The human decides, and the cheapest guard is a stop rule.
- Cost if wrong: one extra round trip if the BNONE behaviour differs.

**C7. Minor. C# `h1` is a `short`.**
- What happens: in C#, `h1` is `short`: `(short)(Database[…] + antenna_height)` with a `short` height of 40 (`HorizonGenerator/MainDlg.cs:48-49`). The plan uses `f64::from(elv) + a.height`.
- Result: identical for whole-metre heights (every default and every test). A fractional `--height` has no C# counterpart.
- Ruling: keep `f64`. Finding 4 (New design) adds one sentence: a fractional `--height` is not truncated as the C# `short` was.
- Why: `--height` is a new flag. The 1:1 rule covers the C# behaviour at its fixed parameters, and those give identical results.
- Cost if wrong: a sub-metre difference in `h1` for users who pass fractional heights.

**C8. Minor. The spec edit range includes the rainscout row.**
- What happens: Task 5 Step 2 names spec `:85-88`, but `:86` is the `rainscout` row, which is not in 5a and must stay.
- Ruling: replace lines 85, 87 and 88 with the three new rows, and keep line 86 unchanged.
- Why: RainScout comes after stage 6 (Q9). Deleting its row would drop it from the spec.
- Cost if wrong: none.

**C9. Minor. The app leak is real; Task 4 covers it; two wording fixes.**
- Coverage: `load_or_default` is the only producer, and `ui/status.rs:13-19` is the only display, so Task 4's variant change covers every place the text is shown.
- Wording: the plan says "control panel"; the text is shown in the status bar.
- The leak is a port bug (stage 3b), not upstream behaviour, so no logic finding is required.
- Ruling: no extra code. The Task 4 commit body says: "settings parse errors no longer quote the file (the app status bar showed the broken line, which can hold a credential)". The plan's ruling text reads "status bar".
- Why: the port policy logs C# behaviour; port bugs are fixed and named in the commit.
- Cost if wrong: the fix is only recorded in git history, not in `logic-findings.md`.
