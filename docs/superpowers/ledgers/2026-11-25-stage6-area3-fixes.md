# Stage 6 area 3 implementation ledger

[Implementation plan](../plans/2026-11-25-stage6-area3-fixes.md) · [Design](../specs/2026-11-25-stage6-logic-fixes-design.md) · [Implementation records](../../logic-fixes.md#area-3-implementation-records)

Branch `stage6-area3-feeds-db` from `470a3d7`. Signed fix plan `8360bf0`.

## Task results

| Task and commit subject | RED → GREEN | Gates and golden deltas |
|---|---|---|
| 1. Callsigns (#14), `fix(as-stations): normalize and split portable callsigns` | Both new focused regressions failed against the original cutter and passed after the change. The complete callsign fixture test consumes all five independently derived deltas. | Format and as-stations Clippy passed. as-stations tests: 21 passed, 0 failed, 1 ignored. |
| 2. VRS timing (#58), `fix(as-feeds): count full VRS polling durations` | The synthetic 1200 ms poll initially counted as 200 ms. The corrected duration and 900 ms comparison select the faster localhost endpoint. Saturation is covered. | Format and as-feeds Clippy passed. as-feeds tests: 104 passed, 0 failed, 2 ignored. No golden change. |
| 3. Versioned startup (#372), `fix(as-db): guard schema versions at startup` | RED: `migrate` accepted a schema-ahead v2 database with v1 support (`Ok(())`) and fresh `SignalDb::open` left `auto_vacuum` at `0` because WAL created page 1 first. GREEN: shared `open_versioned*`/`open_in_memory_versioned*` and `migrate` inspect `user_version` on the raw connection before pragmas, apply `PRAGMA auto_vacuum = FULL` only when `page_count == 0` before `BEGIN IMMEDIATE`, recheck under an immediate transaction (including when initially at the supported version), reject schema-ahead with typed `SchemaAhead { actual, supported }` preserving file bytes/journal/rows, and apply WAL and synchronous defaults only after guarded initialization succeeds across aircraft, position, station, signal, elevation and propagation owners. | Format and locked all-target Clippy (`-D warnings`) passed for as-db, as-signals, as-aircraft, as-stations, as-elevation, as-propagation, as-feeds, as-cli, airscout and airscout-planeserver. Combined touched-crate tests: 206 passed, 0 failed, 3 ignored (12 new `finding_372` tests). No golden change. |
| 4. Signal parsing and merge (#79, #81, #379), `fix(as-signals): parse full spectra and retain second maxima` | All four focused regressions failed before the change and passed after it. The largest finite level survives within its second, a legacy database migrates and reopens, and the complete FFT parses by named header fields with count and finiteness checks. | Format and as-signals Clippy passed. as-signals tests: 11 passed, 0 failed, 0 ignored. No golden change. Spectrum Lab transport coverage follows task 6. |
| 5. Private publication (#381, #305), `fix(as-net): bound downloads and publish complete private files` | RED when download exceeded the cap and settings save deleted a staging file it did not create. GREEN with exact and oversized bodies, 304, concurrent publication and isolated failures. | Format and locked Clippy passed for as-net, as-settings, as-cli, as-aircraft, as-elevation and as-stations. Combined tests: 260 passed, 0 failed, 3 ignored. Source and content validator provenance follows task 12. No golden change. |
| 6. Diagnostic and transport boundaries (#374, #375), `fix(as-feeds): bound transport and sanitize diagnostics` | RED for credential-bearing diagnostic formatting and the zstd window. GREEN for safe feed/config/worker/archive diagnostics, declared and streamed limits, exact decoded and UTF-16 caps, truncation, and Spectrum Lab HTTP input. | Format and touched-crate Clippy passed. Initial touched-crate suite: 1124 passed, 2 failed, 5 ignored. Both failures were PlaneServer assertions for the old Debug/staging contract. Updated configuration and archive targets: 23 passed, 0 failed, 2 ignored. Focused feeds retest: 5 passed. No golden change. |
| 7. Settings and endpoints (#329, #340, #371, #373), `fix(as-settings): resolve versioned profiles and feed endpoints` | Four desktop regressions and the fresh PlaneServer regression failed before the fix. All passed after host-specific defaults, versioned legacy band resolution and canonical saves. Direct, alias and redirected local feed tests cover rejection. | Fmt and Clippy passed for as-settings, airscout-planeserver, airscout, as-feeds and as-cli. Combined tests: 981 passed, 0 failed, 2 ignored. Cache parameters include effective radius, so distinct settings have distinct keys. No golden change. |
| 8. JSON adapters and type enrichment (#48, #51, #52, #54, #55, #59, #60, #65, #66, #67, #378, #380), `fix(as-feeds): preserve valid JSON observations and metadata` | Thirteen focused regressions failed against the starting behavior. They passed after per-record typed decoding, checked source clocks, grounded altitude, identity recovery and wake classification. The airline-backed callsign test uses a synthetic IATA prefix. | Format and Clippy passed for as-feeds, airscout, airscout-planeserver and as-cli. Combined tests: 928 passed, 0 failed, 2 ignored. Two feed JSON composites use documented golden deltas. The existing serde_json dependency enables borrowed raw values with no lockfile change. |
| 9. OpenSky coverage (#64), `fix(as-feeds): apply OpenSky request coverage` | Localhost received the literal tokens before the fix. Both anonymous and bearer-auth requests now send the same expanded bounds and preserve encoded bytes. The stock endpoint matrix exercises authority, scheme, port, path, query and fragment. | Format and as-feeds Clippy passed. as-feeds tests: 123 passed, 0 failed, 2 ignored. No golden change. |
| 10. Checked ages and BinCraft (#61, #384), `fix(as-feeds): decode checked position ages and BinCraft records` | Three red regressions covered wrong wire layout and negative ages. Both binary adapters now validate complete slots, signed position ages and independent validity flags. JSON ages below zero reject only their row. | Format and Clippy passed for as-feeds and as-golden. Combined tests: 127 passed, 0 failed, 2 ignored. Two documented BinCraft composites compare recursively against the independent C# PublicBinCraft field layout. |
| 11. FlexJSON (#70–#75, #77), `fix(as-feeds): resolve FlexJSON fields and clocks safely` | Eight focused regressions failed against the old column mapper. They passed with real object keys, nullable and fractional values, shared numeric rules, signed UTC parsing and rejection of nonunique positional assignments. Runtime inference tests verify layout change and settings reload. | Format and as-feeds Clippy passed. as-feeds tests: 131 passed, 0 failed, 2 ignored. No golden change. |
| 12. Station imports (#377, #382, #383), `fix(as-stations): repair and bound station imports` | Three focused regressions failed against the pre-fix source: scope expansion on a cached 304 returned (0, 0) instead of (1, 1), the decoded member cap was ignored (an over-budget member parsed), and quoted NaN/Infinity and latitude 91° passed ingestion. After the fix they pass, together with source-switch full-body, generation-invalidated QRV refresh, legacy NULL repair, decoded cap and truncated-archive atomicity regressions. | Format and Clippy passed for as-stations, as-net and airscout. as-stations tests: 29 passed, 0 failed, 1 ignored. as-net tests: 6 passed. airscout maintenance tests: 23 passed. No golden change. |
| 13. QRV resolution (#13), `fix(as-stations): resolve equipment for coarse locators` | RED: `qrv_coarse_locator_resolution` and `qrv_coarse_locator_is_no_match` failed against the pre-fix source — a valid 4-character locator errored (`Err(InvalidLocator)`) instead of returning no match, and `/qrv.json?loc=JO50` answered an invalid-locator error. GREEN: exact lookups return no match for a coarse locator and an error only for malformed text; a trusted position inside the coarse cell derives the six-character key, and a coarse locator alone yields non-persisted band defaults. | Format and Clippy passed for as-stations and airscout. as-stations tests: 30 passed, 0 failed, 1 ignored. airscout web_stations tests: 15 passed, 0 failed. airscout wintest_nearest + services tests: 18 passed, 0 failed. No golden change. |
| 14. Live admission and retries (#47, #49, #50, #376), `fix(as-feeds): admit live reports atomically with bounded retries` | RED: the folded track check accepted 90° versus 270° as identical; a zero interval retried a failing start with no pause; an older report still left the worker as a live successor; and reports within +10 s were held while the pending queue grew without bound. GREEN: shortest angular separation rejects the reverse track and accepts a turn across north and exactly 45°; the interval floors at 1 s; a single atomic cache admission returns winners only (99/100/130 yield 130 as the live successor) with older reports staying history-only; and admission through receipt + 10 s inclusive with coalescing, 10 min residence and an 8192-entry cap drains on successful, empty and failed polls. | Format and as-feeds/as-aircraft Clippy passed. as-feeds tests: 139 passed, 0 failed, 2 ignored. as-aircraft tests: 68 passed, 0 failed, 1 ignored. as-cli/airscout/airscout-planeserver check clean. No golden change. |
| 15. Audit, full gates and ledger, `docs: stage 6 area 3 fix ledger` | Audit (not a fix). Verified all 57 area-3 rows. All 44 `fix` rows already carry **Fixed in stage 6** with a pointer in `docs/logic-findings.md` and an area-3 implementation record in `docs/logic-fixes.md` naming the C# location at fbadef0f, the fix in C# terms and a proving test; all 88 recorded test names grep to existing `#[test]` functions (0 missing). Added the missing **Kept** + approval-list pointer to the 13 `keep` rows (#41, #46, #53, #56, #57, #62, #63, #68, #69, #76, #80, #339, #341). `git diff --stat 470a3d7..HEAD -- tests/golden Cargo.lock` is empty. All 9 rows of `docs/logic-fixes-area3/golden-deltas.tsv` (callsign ×5, feed_json ×2, bincraft ×2) are consumed by the callsign, json_feeds and bincraft golden tests, each calling `DeltaManifest::finish()`. | Full macOS gates green (table below). |

## Full gates (task 15)

Every command prefixed `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --all --check` | clean |
| Workspace Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean, no warnings |
| Workspace tests | `cargo test --workspace --locked --no-fail-fast` | 1518 passed, 0 failed, 5 ignored |
| Hamlib Clippy | `cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings` | clean, no warnings |
| Featureless as-cat Clippy | `cargo clippy -p as-cat --all-targets --locked -- -D warnings` | clean, no warnings |

## Blocked rows

None.

## Recovery

Completed tasks 1–15 (signed commits: task 1 `0ff419b`, task 2 `06db360`, task 4 `5b5bbe6`, task 5 `c327afd`, task 6 `549bfa4`, task 7 `20d74af`, task 8 `762c74f`, task 9 `513ee10`, task 10 `70805d0`, plus tasks 11–15 and task 3 on this branch). Area 3 is fully implemented and gated.

## Final review

Four "Important" findings from the Gemini final review of `a3-final-review-gemini.md` were fixed, each with a failing test first (RED → GREEN):

| Finding | Fix | Proving test (RED → GREEN) |
|---|---|---|
| #47 | Skip the track/heading separation check on a zero-displacement update (no defined bearing) while keeping the distance check active. | `plausibility_track_ignores_zero_displacement_updates` |
| #376 | Run stateless per-report validation (ground skip, hex recovery, finite in-area coordinates, component-timed alt/speed) before coalescing, keyed on the recovered hex; empty-hex never coalesces with valid reports. | `finding_376_invalid_or_ground_report_never_shadows_a_valid_one`, `finding_376_recovery_keys_coalescing_on_recovered_hex`, `finding_376_component_timed_reports_drop_out_of_range_motion` |
| #50 | Serialize admission per cache across workers: validate, select winners and enqueue `FeedEvent::Planes` under one lock (`PlaneInfoCache::bulk_insert_validated`), validating each candidate against the accepted live state. | `finding_50_atomic_admission_validates_each_candidate_against_the_accepted_winner`, `finding_50_cross_worker_admission_serializes_live_events` |
| #13 | Thread `trusted: Option<GPoint>` through `qrv_defaulted`/`antenna_of_loc` and pass it only for supplied coordinates or stored non-`FromLoc`/non-`FromBest` positions; coarse locators stay coarse otherwise. | `coarse_locator_alone_uses_band_defaults_not_seeded_qrv`, `coarse_locator_with_stored_station_uses_trusted_position`, `coarse_locator_with_supplied_coordinates_derives_key`, `coarse_locator_uses_trusted_stored_station` |

Workspace gates: 1528 passed, 0 failed, 5 ignored. Clippy (`-D warnings`) and `fmt --check` clean for `as-feeds`, `as-aircraft` and `airscout`.

## Final review, round 2 (re-review of #50)

Two "Important" findings from the round-2 Gemini re-review of `a3-rereview-gemini.md` were fixed, each with a failing test first (RED → GREEN):

| Finding | Fix | Proving test (RED → GREEN) |
|---|---|---|
| #50 (bug) | The worker passed the unfiltered `candidates` to `PositionDb::bulk_insert_plane_infos`, so reports rejected by the atomic plausibility re-validation inside `bulk_insert_validated` still entered history. `bulk_insert_validated` now returns `(winners, validated)`: `winners` drive live events and metadata, while `validated` (candidates that passed `validate` including older/equal-time reports that lost) is written to history. Rejected reports reach neither. | `finding_50_rejected_report_never_reaches_history` |
| #50 (test gap) | `finding_50_cross_worker_admission_serializes_live_events` ran two raw threads on `cache.bulk_insert_validated`; it was rewritten to drive the real worker path (`FeedWorker::spawn_with_sync`) with test-only `WorkerSync` barriers at admission and DB write. It forces a newer 130 to beat an eligible pending promotion 110, forces 150's DB writes to complete before 140's, and checks live-event order, AircraftDb metadata and PositionDb history. | `finding_50_cross_worker_admission_serializes_live_events` |

## Final review, round 3 (WorkerSync deadlock and the #50 cross-worker promotion)

Three fixes from `a3-fix-wave3.md`, each addressed in one signed commit (`test(as-feeds): make the #50 cross-worker test exercise promotion`):

| Item | Fix |
|---|---|
| #50 (worker bug) | `WorkerSync`'s `Arc<Barrier>` rendezvous could block `stop()`/`join()` forever (`Barrier::wait()` is uninterruptible, and the hook fired on empty candidates). Replaced with a one-shot, timeout-bounded `SyncPoint` (`sync_channel(1)` pairs + `recv_timeout`, receiving halves behind a `Mutex` so it stays `Send + Sync`): `pause()` signals arrival then waits at most 2 s for a release, `wait_arrived()`/`release()` are the test side. The `cycle` hooks now `take()` (one-shot) and are gated on non-empty work (`at_admission` on candidates, `at_db_write` on winners/history-eligible), so a quiet cycle passes through and a later cycle can never re-arm it. Production `spawn` is unchanged (passes `WorkerSync::default()`). |
| #50 (test gap) | `finding_50_cross_worker_admission_serializes_live_events` now genuinely routes 110 through the pending queue: a report 11 s ahead of receipt is held on cycle 1 and promoted through `drain_pending` on cycle 2, where it parks at admission before a strictly newer 130 (12 s) is admitted by a second worker that spawns only after `wait_arrived()`. It asserts 110 emits no live event and no `FeedEvent::Error` appears, 130 stays latest in cache and `AircraftDb`, and history holds both. Phase (b) keeps 140 then 150 with distinct callsigns (`DLH140`/`DLH150`), holds 140's writer at `at_db_write` until `worker_b2.join()`, and asserts the source-time guard keeps `call == "DLH150"` and `last_updated == 150`. |
| WorkerSync regression | `worker_sync_points_are_one_shot_and_never_deadlock` proves a worker parked at a never-released `SyncPoint` still stops and joins within the 2 s bound. |

Workspace gates: 1530 passed, 0 failed, 5 ignored. `cargo fmt --all --check` clean; `cargo clippy -p as-feeds --all-targets --locked -- -D warnings` clean. `tests/golden`, `upstream_commit` and `Cargo.lock` unchanged.

## Close-out

| Gate | Tree | Result |
|---|---|---|
| macOS fmt, workspace Clippy, workspace tests | `d83bdfe` | Pass; 1,530 passed, 0 failed, 5 ignored |
| Linux Docker (`clippy`, `test --no-fail-fast`, hamlib pair) | `d83bdfe` | LINUX_OK; 1,530 passed, 0 failed |
| Windows mingw | `d83bdfe` | WINGNU_OK |

- Implementation: Codex gpt-6.2-sol (tasks 1–2, 4–11, stopped at its usage limit and cancelled), DeepSeek V4 Pro (tasks 12–15, fix rounds), Gemini 4 Neon (task 3, #372).
- Final review: DeepSeek V4 Pro approved (0 Important, 2 Minor); Gemini 4 Neon (blocking) requested changes (4 Important: #47, #376, #50, #13), fixed in `5a0836a`; two re-review rounds on #50 (history only from validated reports `34f0834`; deterministic promotion test and one-shot bounded sync hook `d83bdfe`); final re-review approved (0 Important, 2 Minor).
- Linux-only test fix: `d348e65` resends the valid Win-Test datagram after the UDP garbage burst.
- `git diff --stat 470a3d7..HEAD -- tests/golden Cargo.lock`: empty.
- Deferred minors: the Minor findings in the scratchpad review reports (DeepSeek 2, Gemini 5 + 1 + 2) and the maintenance Tables dialog listing the three new internal station tables as `[not found]` (area 4).
- Merged to `main` by fast-forward and pushed (owner authorization for per-area merges, 2026-11-25).
