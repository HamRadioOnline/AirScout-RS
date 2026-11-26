# SDD ledger — plan: docs/superpowers/plans/2026-11-23-stage3c-history-maintenance.md

Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md (reachable). Branch: stage3c-history-maintenance, base 4185580 (plan commit on top of main 59c9b44).
User approvals (2026-11-23): plan OK; History from URL = option B (generic, user URL, no default, disclaimer); fs4 dependency OK; subagent-driven; push only on request.

## Pre-flight

| Tasks | Produces vs consumes | Found |
|---|---|---|
| 1→all | app/ submodules (history.rs, tracking.rs, maintenance.rs), pub paths unchanged | consistent |
| 2→5,6,8,10,13,15 | Analysis, Track, Doppler/DopplerStrategy, gui flags | consistent names |
| 3→4,5,9,10,11 | PositionDb get_all_hex/get_all_calls/get_by_*_in/get_all_with/delete_all/bulk_insert, PlaneInfoCache set/clear | consistent |
| 4→5,8 | ImportStats, read_json/read_csv/read_hist, write_json/write_csv | consistent |
| 5→6,7,8,9,10,11,12 | JobRunner, JobCtx, ServiceEvent::Job, JobEvent (+variants per task), AnalysisJob, run_analysis/cancel_analysis, HistoryData, HistoryIndex | consistent; later tasks add enum variants |
| 6→7,9,10 | HistoryState, LifeMode, request_paths_for_mode, set_history_range, scroll_to, marker_bins | consistent |
| 8 self | DiskSpace trait + Fs4Disk | fs4 approved |
| 12→13,14 | as_db helpers, with_conn, maintenance.rs DbId/MaintenanceJob | consistent |
| others | self-consistent | — |

Ruling P1: plan "Open question 1" (fs4) is answered — user approved 2026-11-23 — cost: none.
Task 1: complete (commits 4185580..0b0246a, review clean)
Task 1: note: app/mod.rs = 733 lines after split (bound for 3c: ~793); set_beeper stays in mod.rs.
Task 2: complete (commits 0b0246a..f8657ba, review clean)
Task 2: minor (deferred): const-date unwrap in Analysis Default; DopplerStrategy::None shadows Option::None in glob imports; 3b-load test doesn't set a non-default existing key.
Task 3: complete (commits f8657ba..72bff5f, review clean)
Task 3: minor (deferred): get_all_with progress callback runs under DB mutex (deadlock if it touches PositionDb, doc it); whole-scan lock ~224 ms/500k release (no contention in HISTORY); get_all_hex/calls unordered (C# same) — callers sort.
Task 4: Ruling: minor "one malformed record / 1e999 fails whole file" taken into fix round — contradicts brief "a per-record error never fails the whole file" — cost: none.
Task 4: minor (deferred): naive T-separated time rejected; hist imports in key order (BTreeMap) not file order; whole JSON parsed into Vec<Value> (memory); JSON reader upper-cases hex/call (C# not); NaN/inf written as NaN/null; missing tests for write_csv sanitising, numeric strings, fractional/offset Time, hist BOM.
Controller: leftover stash@{0} (Task 3 impl WIP on f8657ba, content already committed in 72bff5f) — leave, mention to user.
Task 4: fix round 1/5 (2 addressed, 0 open — finite Lat/Lon required, per-record parse via split_top_level; commits 75306d4..9148d7a)
Task 4: complete (commits 72bff5f..9148d7a, review clean)
Task 4: minor (deferred): trailing comma counts 1 extra skip; junk after closing bracket ignored; hist duplicate keys each import; no lat/lon range check on JSON/hist import; unterminated string swallows rest of file into one skip.
Task 5: Ruling: AnalysisJob::LoadHistory carries ttl_min (run_analysis gets no settings) — interface change for Task 6 — cost: none.
Task 5: minor (deferred): OFF leaves last progress text in status (C# "Cancelled."); any other job cancels a running history load leaving neither loaded nor loading (Task 7 must disable buttons while loading); enrichment cancel drops all (needs Not-reproduced entry, Task 16); "Adding info"/"finished" progress texts not ported; "N imported" = rows written (document); bulk_insert error aborts whole LoadFiles job (C# per file); .hist call lookup hides AircraftDb errors; delete_all may exceed 5 s busy timeout vs feed/maintainer writers; cancelled job overlaps successor until next check; index/positions passed separately (debug_assert len); cancel test only pre-set flag, no tests for mid-read cancel, spawn failure, panic→Failed, file import cancel.
Task 5: fix round 1/5 (1 addressed, 0 open — first call per hex 1:1; commits 3f02883..2708aae)
Task 5: complete (commits 9148d7a..2708aae, review clean)
Task 5: minor (deferred): enrichment cancel checked every 1000 rows (C# per row).
Task 5: carry to Task 6/7: AppState::start_history_load / stop_history exist in app/history.rs (wire to ON/OFF); LoadHistory { ttl_min }; Task 7 must disable other job buttons while a history load runs.
Task 6: Ruling: fix round takes spec gap (200k debug bound 100→50 ms), finding-list correction (OFF time.online needs no finding), MULTI watchlist re-send in HISTORY while playing forward (C# MapDlg.cs:6094/:5101, port 1:1), leave_history guard against double OFF — cheap, user-visible — cost: none.
Task 6: minor (deferred): HistoryIndex+markers built on UI thread (~22 ms release / ~106 ms debug per 500k, once per load; move into job later); other job during load drops result and set_history_range says "No positions" (misleading); HistoryState.job_id could be pub(crate). Markers ascending relies on get_all_with ORDER BY LastUpdated ASC (Task 3) — verified by controller.
Task 6: fix round 1/5 (4 addressed, 0 open — 50 ms bound, finding retracted, HISTORY MULTI watchlist re-send, leave_history guard; commits 278ce70..2c6c21e)
Task 6: complete (commits 2708aae..2c6c21e, review clean)
Task 6: note for Task 16: watchlist re-send deviation — port re-sends on any positive step (C# only PlayMode FORWARD; not FASTFORWARD, not FASTBACK reaching positive width via Rewind); name both cases.
Task 6: minor (deferred): watchlist_dirty survives early-return history ticks / pause (re-send later than C#); history.rs:~309 dirty reset consistency unchecked.
Task 7: complete (commits 2c6c21e..fa3e8df, review clean)
Task 7: minor (deferred, final review should weigh): range_fields commits on every lost_focus (needless tick, truncates sub-second min/max → first/last report dropped); reload overwrites "N imported"/"N deleted" status; no reload after failed partial LoadFiles/Clear; load_list clones Vec per frame, no ScrollArea; Tmp listing on UI thread (small dir, OK); clear modal no Esc/click-outside; load remembers bare name (C# full path), files[0] alphabetical; WIRED is one global const; missing tests Failed→busy cleared, reload after LoadFiles/Clear, Min/Max commit.
Task 8: implementer a599be1fe473043cf (resumed after interrupted first run); disk rule added to instructions
Task 8: review 1 (a65ca31): spec ✅, quality approved, 0C/0I/8M. Controller re-grade by effect: M3 userinfo not redacted → Important (violates credential constraint); M1 fresh zip deleted on local csv error → Important (forces ~8 GB re-download); M4 no read timeout → Important (blocked thread after stall/cancel).
Task 8: Ruling: M2 archive_url inserts `%Y-%m-%d.zip` into the path before any `?query` instead of plain concatenation — plan's concatenation makes query-token URLs unusable; C# behaviour with query URLs is unverified — cost if wrong: one log finding / revert of a small helper.
Task 8: minor (deferred): M5 no dedicated Cancel button (HISTORY OFF only); M6 whole-day csv read into memory; M7 cancel mid-write leaves partial {date}.csv (no .part); M8 no tests for content-length mismatch / stall path.
Task 8: Ruling: M4 read timeout — reqwest 0.12 blocking has no read_timeout; job returns within ~100 ms on stall/cancel, detached socket thread bounded by 12 h total timeout / server close — no new dependency for a leaked idle thread — cost if wrong: one idle thread+socket per stalled download until restart.
Task 8: fix round 1/5 (3 addressed + ruling M2 applied, 0 open; commits a65ca31..bf66a2c)
Task 8: minor (deferred): 12 h MAX_TRANSFER caps slow downloads (<~190 KB/s for 8 GB); archive_url ignores #fragment; password with unescaped / or ? leaks tail past strip_userinfo; history_download.rs 525 lines (>500 guideline).
Task 8: complete (commits fa3e8df..bf66a2c, review clean after 1 fix round)
Task 9: BASE bf66a2c
Task 9: implementer a5807cb769d3467dc, commit c3e250a; carried Task 7 layout fix (separator pushed Functions group off-window). Controller: cleanup now cargo clean --profile dev (keeps target/upstream).
Task 9: review 1 (c3e250a): spec ✅, quality needs fixes, 0C/1I/11M. I: Close cancels whatever job runs (no job-id match), PlaneLists Failed leaves dialog stuck. Controller re-grade: M5 Windows-invalid chars in file name → Important (cross-platform target, export fails on Windows); M10 real-looking ICAO hex fixture → Important (generic-data constraint).
Task 9: minor (deferred): M1 number-format comment wrong (.NET 4.7.2 R15 vs shortest round-trip); M2 C# header trailing spaces undocumented; M3 `;`/CR/LF cleaning undocumented finding; M4 exports.rs imports CSV_SEPARATOR from ui module; M6 DISTINCT scans ignore cancel; M7 exported CSVs appear in Tmp load list; M8 thin cancel test; M9 path_csv_eps_values doesn't check info_point cols; M11 typed hex not upper-cased (C# same). #118 is a port layout bug → stays out of logic-findings.
Task 9: fix round 1/5 (3 addressed, 0 open; commits c3e250a..977952e)
Task 9: minor (deferred): stale-dialog test passes without job-id guard (superseded-while-waiting case untested); superseded waiting lists job → dialog waits forever; PlaneLists handler lacks own job_id check.
Task 9: complete (commits bf66a2c..977952e, review clean after 1 fix round)
Task 10: BASE 977952e
Task 10: implementer acaf230a90afaa13d, commit 06a0438; C# GetAll extrapolates to step time (brief said report time) → ported 1:1 as #123; crossing_history.rs 545 lines, ui/analysis.rs 559 lines (split candidates for final review).
Task 10: review 1 (06a0438): spec ❌ (1), quality needs fixes, 0C/1I/9M. I: CSV category written `Medium`, C# enum name `MEDIUM`. #123 (step time) confirmed against PlaneInfoCache.cs:316-323. Controller re-grade: M1 path calc ignores cancel → Important (Back cannot stop long path build); M3 #123 unpinned by test → Important (prediction-affecting 1:1 port); M7 src 545 / tests 626 lines → Important (global constraint new files <~500).
Task 10: minor (deferred): M2 SignalDb errors swallowed as "no level"; M4 startindex test proves only no-panic; M5 #125 wording vs clone before mark_ambiguous; M6 Calculate rerun allowed (C# disables) unlogged in #128; M8 ui_analysis dir.keep() leaks temp dirs; M9 chart span from count_hi only.
Task 10: fix round 1/5 (4 addressed, 0 open; commits 06a0438..5b5f4b3)
Task 10: minor (deferred): leftover eprintln! in step-time test; cancel test uses pre-set flag only; ui/analysis.rs 559 lines.
Task 10: complete (commits 977952e..5b5f4b3, review clean after 1 fix round)
Task 11: BASE 5b5f4b3
Task 11: implementer a14bdfbf811bdf2e4, commit 2c32511; WIRED scaffolding removed (Traffic was last). #133 needs rewording at Task 16.
Task 11: review 1 (2c32511): spec ✅, quality approved, 0C/0I/4M. ⚠️ get_all ordering verified by controller: positions.rs:151 ORDER BY LastUpdated ASC. ⚠️ findings #130-#134 → Task 16.
Task 11: minor (deferred): routes projected per frame without culling; no exact-100 km test, cancel only pre-set; open() range from loaded history not DB oldest/youngest (brief says oldest/youngest); thinning on UI thread.
Task 11: complete (commits 5b5f4b3..2c32511, review clean)
Task 12: BASE 2c32511
Task 12: implementer aa564c1301a4f0936, commit 9403480; tests/maintenance.rs 868 lines; maintenance jobs not cancellable on dialog close; ids from 1<<62.
Task 12: review 1 (9403480): spec ❌ (2), quality needs fixes, 0C/2I/7M. I1 DB descriptions not verbatim (Stations "Maidenhead Locator", Positions/Signals invented, Signals title); I2 tests/maintenance.rs 868 lines. Safety focus all held. Controller re-grade: M2 Play not refused while maintenance job runs → Important (feed writes lost on 5 s busy timeout).
Task 12: minor (deferred): M1 Locations answer stamped with current pattern (stale matches, not unsafe); M3 maintenance cancel checks dead code (no cancel on close; DeleteMapTiles unstoppable); M4 sizes stale/never arrive; M5 delete station not atomic; M6 unused test helper param; M7 tile guard accepts relative data dir (still only ./tiles).
Task 12: fix round 1/5 (3 addressed, 0 open; commits 9403480..6795d82)
Task 12: minor (deferred): Play-after-Failed maintenance path untested.
Task 12: complete (commits 2c32511..6795d82, review clean after 1 fix round)
Task 13: BASE 6795d82
Task 13: implementer a77c464a38d725e39, commit 93c3b57; Coverage job carries base_url+area; own job slots; real cancel via Services::cancel_maintenance; coverage zoom aggregation (4-/2-char) beyond brief; grid centre sample (C# corner) + padded normalisation.
Task 13: review 1 (93c3b57): spec ❌, quality needs fixes, 0C/1I/11M/2nit. I: grid passes setinvalidtozero=false → missing tiles -32767 m wreck normalisation; C# indexer defaults true (MapStationDlg.cs:320, ElevationDatabase.cs:731-738).
Task 13: Ruling: plan said `store.elevation(model, centre, false)`; C# uses true → port 1:1 with true — plan text contradicted the C# it cites — cost if wrong: missing tiles show 0 m instead of being flagged.
Task 13: Controller re-grade: padded-view normalisation → Important (visible colours differ from C#, cheap fix); unlogged deviations (aggregation/amber mixed state, >2000 merge wording #147, status without model prefix, label hiding <28 px, ±1° fit) → must be named as findings this round.
Task 13: minor (deferred): CoverageData::new ~100 ms UI hitch at 324k tiles; visible() scans all tiles per repaint; temp dir left on panic, cancelled cleanup untested; plus remaining review minors/nits in task-13-review.md.
Task 13: fix round 1/5 (3 addressed, 0 open; commits 93c3b57..5125da2); 3 minor notes in task-13-rereview-1.md (deferred).
Task 13: complete (commits 6795d82..5125da2, review clean after 1 fix round)
Task 14: BASE 5125da2
Task 14: implementer a0c89cf4ace9aee89, commit 0138a14; run(targets, data_dir, settings_path); shared data_dir_unsafe guard (now rejects relative data dir); RootApp shows dialog before AirScoutApp::new.
Task 14: review 1 (0138a14): spec ❌, needs fixes, 0C/4I/6M. I-1 clean misses <model>-sea-squares.txt + calculator-<model>.stamp; I-2 run() deletes on UI thread (Review Focus 3 wants job thread); I-3 Cancel exits -1 but C# Cancel starts app without deleting (only window close aborts); I-4 tests call run() on real $HOME/Tmp and /etc/hosts.
Task 14: Ruling: I-3 port C# 1:1 — Cancel = start app without deleting, window close = abort (exit -1) — brief misread CleanupDlg.cs:126-131 / MapDlg.cs:637-646 — cost if wrong: one button's semantics.
Task 14: Controller re-grade: shared guard now rejects relative data dir → Task 12 "Delete Map Tiles" always fails in relative `data` fallback → Important; fix by absolutising data_dir before the guard, not by rejecting.
Task 14: Controller re-grade: M-1 HOME unwrap in test → Important (hosted Windows CI panics); M-3 settings target accepts a directory → Important (destructive, cheap guard).
Task 14: minor (deferred): M-4 window pos/title after Continue; M-5 Tmp checkbox lacks dir hint; M-6 no kittest for dialog wiring.
Task 14: fix round 1/5 (7 addressed, 0 open; commits 0138a14..1c2bf0d)
Task 14: minor (deferred): window cannot close during long delete; detached worker partial delete only on kill; "Continue anyway" button beyond C#; brief-correction finding "#153b" needs a real number in Task 16.
Task 14: complete (commits 5125da2..1c2bf0d, review clean after 1 fix round)
Task 15: BASE 1c2bf0d
Task 15: implementer ac5246f6e9a419147, commit 93a31f0; deselect with activate keeps TRACK until next tick (C# 1:1, brief said NONE); extra ui/map_track.rs; app/mod.rs 733 lines; tooltip pad2 .NET 00.00 form.
Task 15: review 1 (93a31f0): spec ✅ (both brief deviations confirmed against C#), quality needs fixes, 0C/1I/6M. I: Services::ground_elevation on UI thread in track_tick (DB/tile I/O, mutex contention with path worker).
Task 15: Ruling: global constraint "no file I/O on UI thread" outranks the plan's synchronous call — heights come from a worker (reuse path-worker station heights or a background event) — cost if wrong: first tracking values arrive one tick later.
Task 15: Controller re-grade: M1 A→B→A Doppler keeps old A values (C# nulls oldvalues on step for another hex, :8064-8067) → Important (prediction-affecting, port 1:1); M2 wrong speed-sign comment → fixed in same round (cheap).
Task 15: minor (deferred): M3 pad2(NaN) gives 00NaN for ≤1 s after SINGLE→TRACK; M4 render test asserts nothing; M5 test name contradicts behaviour; M6 report claim too narrow.
Task 15: fix round 1/5 (3 addressed, 0 open; commits 93a31f0..08397f9)
Task 15: minor (deferred): self.track not cleared on stop (previous Play values briefly visible); heights fixed per Play (#158).
Task 15: complete (commits 1c2bf0d..08397f9, review clean after 1 fix round)
Task 16: BASE 08397f9
Task 16: implementer adfb82af8f1bfbaf4, commit 9882379; findings #106-#189; mac+linux(aarch64) gate 691/0; doppler_shift doc comment fixed (#179).
Task 16: review 1 (9882379): spec ✅, quality approved, 0C/0I/6M.
Task 16: minor (deferred): wording nits #181, #173, README "50 GB" vs #106 50 GiB, #189, #136/#172 classification labels, #171; #106 progress-text part unverified.
Task 16: complete (commits 08397f9..9882379, review clean)
Final review (59c9b44..9882379): with fixes, 0C/2I/8M. I1 quit in HISTORY saves time.online=false → next session shows no planes; I2 Options OK in HISTORY restarts maintainer → deletes imported positions >7 d.
Final: Ruling: Traffic frame cost with 500k positions (declined) — stays deferred minor; walkers draw only on repaint, thinned once — cost if wrong: stutter in Show All Traffic with huge history.
Final: Ruling: Windows Tmp/tmp case-insensitive (declined) — second delete is a no-op, harmless — cost if wrong: one spurious error line on Windows.
Final: Ruling: C# query-token URL behaviour (declined) — keep Task 8 M2 ruling — cost if wrong: small helper revert.
Final: Ruling: C# History-from-URL usability after OFF maintainer (declined) — upstream behaviour, port matches after OFF; note stays in findings — cost if wrong: none for port.
Final: Re-grade into fix wave: strip_userinfo leak with unescaped / ? # in password → Important (credential rule); Settings Debug prints history URL unredacted → Important (credential rule); ON/analysis buttons not blocked by running maintenance job → Important (history load vs destructive clear); docs: #155 wrong about C# (MapDlg.cs:3889), README cancel claim, JSON hex/call upper-casing finding missing → fix in wave.
Final: minor (deferred): Set Time usable in HISTORY; five files >~500 lines; Show All Traffic stutter unmeasured.
Final: fix wave (commits 9882379..9743d45): 6/6 addressed in re-review, no new C/I.
Final: Ruling: residual — password containing `"`, `<`, `>` or whitespace still visible in status text (Debug safe) — such characters are illegal unescaped in URLs, rare; no second fix wave — cost if wrong: a malformed credential URL's password shows in the status line.
Final: minor (deferred): re-review minor notes in final-rereview.md; reload after Positions clear in HISTORY; Set Time in HISTORY; ui/analysis.rs 565 lines.
