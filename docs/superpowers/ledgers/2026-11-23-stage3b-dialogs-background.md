# SDD ledger — plan: docs/superpowers/plans/2026-11-23-stage3b-dialogs-background.md

Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md (reachable). Branch: stage3b-dialogs-background, base df55afc (plan commit on top of main 1b610a3).
User approvals: plan "joa"; rodio + CI apt install of libasound2-dev/pkg-config on self-hosted runner OK; subagent-driven; push only on request.

## Pre-flight

| Tasks | Produces vs consumes | Found |
|---|---|---|
| 1→3,5,7,11,12,13,14,15 | ElevationEnabled.effective_model, Watchlist, Background, SpecLab, TimeSettings, Secret, first_run | consistent names |
| 1 self | field-level serde default on first_run vs struct-level default | field attr wins — OK |
| 1 self | as-settings gains as-stations dep (DEFAULT_UPDATE_URL) | no cycle (only airscout, as-cli depend on as-settings) |
| 2→3,9,12 | local_obstruction_find, location_get_all | consistent |
| 3→4,5,6,9,12 | PathRequest.targets/PathTarget, PathReady{index,call,gains}, PathsDone, PathEntry, single_path | consistent |
| 4→5 | TickInput.mode: as_aircraft::enums::PathMode (None/Single/Multi exist), per_path_potential | consistent |
| 5→6 | ControlTab::Multi, watch_potential, set_watch_checked | consistent |
| 7→11 | Options OK restarts background (Task 11 not yet built) | Ruling P1 |
| 7→10 | planes.rs created in 7, feed combo in 10 | OK (plan says so) |
| 8→9 | Stations tab buttons open dialogs built in Task 9 | Ruling P2 |
| 10→15 | disclaimer flow reused by wizard | consistent |
| 11→12 | ServiceEvent::BackgroundIdle(Led) | consistent |
| 13 self | renames existing as-signals test (behaviour change, GUI bug not reproduced) | OK, finding update in 13/16 |
| 14 self | ci-prepare.sh apt install | user approved |
| others | self-consistent | — |

Ruling P1: Task 7 adds `Services::restart_background(&mut self, settings: &Settings)` to the trait; RealServices implements it as a no-op until Task 11 fills it — why: Options OK must call something testable now — cost: none.
Ruling P2: Task 8 adds the Obstructions/Horizon buttons only in Task 9 (Task 8 adds just MapStationDlg button) — why: dialogs do not exist yet — cost: none.
Task 1: complete (commits df55afc..9a0a017, review clean)
Task 1: minor (deferred): watchlist sort ordinal vs C# culture CompareTo (display only); doc comment of elevation_model sits on elevation_enabled; no TOML roundtrip test for TimeSettings.offline; Watchlist inner Vec public.
Task 2: complete (commits 9a0a017..3de0ea8, review clean)
Task 2: Ruling: obstruction LastUpdated stored as unix seconds (C# INT32) and blob as raw LE f64x360 (C# BinaryFormatter not portable; binary compat is a spec non-goal) — cost: none.
Task 2: minor (deferred): NULL blob reported as "0 bytes" corrupt; select-then-write not one statement (mutex-held); implementer wrote tests with code (no RED evidence).
Task 3: Ruling: obstruction height = QRV antenna height only (C# MapDlg.cs:4409-4411), plan text said ground+antenna — C# authority — cost: none.
Task 3: complete (commits 3de0ea8..1dec4eb, review clean)
Task 3: minor (deferred, carry to Task 5): pub paths vs private path_indices can desync (make paths read-only or add index); Fit only on first MULTI entry; panic fallback fails already-sent targets; obstruction test uniform over bearings; services.rs 621 lines.
Task 4: complete (commits 1dec4eb..bfedea4, review clean)
Task 4: minor (deferred): per_path_potential filled in all modes (doc it); no test for filtered-out plane giving 0; 1000 ms debug timing budget may flake.
Task 5: Ruling: MULTI skips out-of-range watchlist items although C# computes them — consistent with the 3a Path_MaxLength Play guard (mass downloads on long paths) — cost: OOR stations get no path in MULTI.
Task 5: ⚠ PathsDone after errors — verified: run_paths always sends PathsDone (also after panic fallback), services.rs:224.
Task 5: minor (deferred): station_position DB lookup on UI thread per checked item at Play/resend; each checkbox toggle refits map; MULTI validates My via my_loc_text but sends settings lat/lon; watch_potential test only proves keys; Multi tab buttons not visibly disabled while playing; insert_capped appends (C# inserts at 0).
Controller: license commit 08e5e4e (GPL-3.0-only, LICENSE, NOTICE, README) per user OK 2026-11-23 — outside plan tasks.
Task 5: fix round 1/5 (2 addressed, 0 open — OOR flag covered via dx_watch_item, fit_due tested; commits 90ff858..3a5a383)
Task 5: complete (commits bfedea4..3a5a383, review clean)
Task 5: Ruling: LocationFindOrCreate best-case-elevation position shift (Path_BestCaseElevation, MapDlg.cs:3544-3642/3551) not yet ported for UI paths — prediction-affecting — carried into Task 12 dispatch (Task 12 ports Path_BestCaseElevation; apply to UI path targets too) — cost if dropped: UI path differs from C# with best case on.
Task 6: complete (commits 08e5e4e..5bcd32c, review clean)
Task 6: minor (deferred): Stations event not tied to dialog open id (stale list after quick reopen); checked matched by call only (C# same); rows() clones+sorts per frame.
Task 7: ⚠ keyed tile URL in library logs — verified: airscout installs no logger (no env_logger/tracing), walkers errors not surfaced in UI; risk only if a logger is added later (note for final review).
Task 7: ⚠ restart_background no-op — Task 11 must implement it in RealServices (carry into Task 11 dispatch).
Task 7: complete (commits 5bcd32c..3bc2b09, review clean)
Task 7: minor (deferred): Esc/click-outside on Options modal does nothing; no cross-field validation (min/max alt); no test for key mask toggle / key cleared fallback / all-models-off warning; Thunderforest max_zoom 22 own choice.
Task 8: complete (commits 3bc2b09..352d09f, review clean)
Task 8: minor (deferred): QRV edits lost on key flip while typing; qrv_get_all/qrv_write sync on UI thread (small, mutex exposure); My==DX same key last wins; map_station lat ±90 no clamp, typed loc doesn't recentre; loc-valid check duplicated 3x; findings #88 should also note C# QRV delete-on-empty and Location write on Leave (Task 16 docs); tests mutate state directly.
Task 9: Ruling: horizon ground elevation + station position ported 1:1 (raw single-model lookup HorizonDlg.cs:83; LocationFindOrCreate lookup HorizonDlg.cs:80) instead of documenting deviations — port policy (prediction-affecting) — cost: none.
Task 9: Ruling: CSV separator constant ";" (C# GetCSVSeparator depends on OS culture), header with C# unit names — cost: none.
Task 9: minor (deferred): export silent without horizon (unreachable); horizon buttons in stacked modal untested by click; horizon_plot_polar written every frame; app.rs ~1110 lines (split horizon/obstruction orchestration into submodule).
Task 9: fix round 1/5 (3 addressed, 0 open — no engine NFS write from horizon, raw single-model elevation + LocationFindOrCreate position (#90), spawn error → HorizonFailed; commits 9c44960..ec05b28)
Task 9: complete (commits 352d09f..ec05b28, review clean)
Task 9: minor (deferred): horizon obstruction looked up at draft My position, C# uses saved settings position (differs only for unsaved edits).
Task 10: Ruling: pre-existing Settings Debug leaking feed_settings credentials re-graded Important by effect and taken into Task 10 fix round (this task writes credentials via UI) — cost: none.
Task 10: Ruling: disclaimer Decline keeps old feed (C# sets [none], OptionsDlg.cs:2264) and Windows machine-id gating not ported — GUI-layer; findings entries in Task 16 docs — cost: none.
Task 10: minor (deferred): Feed settings button silent for unknown feed name; "Show credentials" toggle extra; no test that load_settings error never contains a secret; vars.rs:113 puts URL template into Config error (risk if user puts key in URL).
Task 10: fix round 1/5 (3 addressed, 0 open — FeedSettingsResult Debug redacted, Settings.feed_settings → FeedSettings newtype with redacting Debug, invalid stored table reported; commits ba6d2b2..df94d54)
Task 10: complete (commits ec05b28..df94d54, review clean)
Task 11: note for Task 15: restart_background (Options OK) would start background before wizard finished — Options can't open during wizard, so OK; Task 15 gates start_background().
Task 11: Ruling: background elevation job downloads only (new download_area sharing per-square logic, cancelable); path validity (bounds/COMPLETE) decided only by the path service's prepare_area — avoids race on shared store state — cost: LED UPTODATE means "all area squares present", not the C# per-area COMPLETE bit.
Task 11: Ruling: schedule semantics follow upstream (OnStartup=RUNONCE; else Periodically=run now then every period), plan text said startup-then-periodic — C# authority — cost: none.
Task 11: minor (deferred): MaintainerConfig::default ignores AircraftDatabase_Max* settings (no fields yet); aircraft import vs feed second connection busy timeout 5 s.
Task 11: fix round 1/5 (7 addressed, 0 open — download_area no shared state, cancel per square, own stations connection, LED reset, generation ids, maintainer always, upstream schedule; commits fd9b278..159fb59)
Task 11: complete (commits df94d54..159fb59, review clean)
Task 11: minor (deferred): maintainer period uses update_period_min (upstream Database_BackgroundMaintenance_Period=60); cancelled scan loses sea squares found so far; QRV write during long stations import may hit 5 s busy timeout; maintainer Error event not generation-tagged.
Task 12: Ruling: near-field suppression set only by the app (start + apply_options), no per-thread writes — same shared-state race class as Task 9 — cost: none.
Task 12: Ruling: calculator reads My live via shared state (C# reads Settings per location) — cost: none.
Task 12: note: findings numbering continued at #91 (Task 8/9 used #88-#90).
Task 12: minor (deferred): UI best case locator from pos not ld.Loc (8-char loc edge, unreachable); services.rs ~720 lines (split in final wave or later); background wiring untested.
Task 12: fix round 1/5 (7 addressed, 0 open — NFS app-only, start gate, Calculations generation, live My, stamp incl. QRV, best-case integration test, doc; commits 87ff4bf..68866c1)
Task 12: complete (commits 159fb59..68866c1, review clean)
Task 12: carry to Task 15: if the wizard defers start_background, sync_services (NFS + My) must still run before any Play — call sync_services independently of start_background.
Task 12: minor (deferred): sync_my clones call each frame; user QRV/location edits don't bump table timestamps (no calculator rerun; C# uses file mtime); invalid My still writes stamp.
Task 13: Ruling: gm_Nearest (nearest-plane map overlay) not ported — plan says so (GUI-only) — cost: no map highlight of nearest plane.
Task 13: minor (deferred): speclab thread-spawn failure reports no SpectrumError; URL/band read once at start (C# re-reads per loop); SignalDb::open on UI thread at first Play; magenta band uses recorded flag at drain time; no cancel-stops-events test; HTTP body unbounded; nearest box shown on Planes and Spectrum tabs; app.rs 1334 lines.
Task 13: Ruling: buffered recorded levels flushed on cancel (C# worker lives for app lifetime, buffer survives Pause) — stored data must not be lost — cost: none.
Task 13: fix round 1/5 (2 addressed, 0 open — flush buffer on stop + #81 updated, y_range minimum span; commits bb5a4d3..9164e27)
Task 13: complete (commits 68866c1..9164e27, review clean)
Task 13: minor (deferred): y_range clamps bounds so out-of-range levels (< -120) plot off-range; poll has 8 params (clippy allow).
Task 14: Ruling: alarm beeps once per newly alarming hex (plan) while C# beeps every update while any plane alarms (MapDlg.cs:3485) — GUI-layer, not reproduced; finding entry #95 in Task 16 docs — cost: fewer beeps than C#.
Task 14: complete (commits 9164e27..4f04a78, review clean)
Task 14: minor (deferred, final review should weigh): no-audio status note overwrites data-dir open_error in with_settings; N new hexes in one tick beep N times (stacked sines); ci-prepare installs alsa for windows-gnu job too; no tests for re-beep after restart, clear() on pause, SetTimeDlg::parsed, UTC colour; set_time test never runs dialog; app.rs ~1380 lines; windows-gnu build only verified by CI.
Task 15: complete (commits 4f04a78..0006869, review clean)
Task 15: carry from Task 12 resolved: sync_services runs via start_background from startup() and finish_wizard(), tested with recording fake.
Task 15: minor (deferred, final review should weigh): default call "DL2ALF" passes wizard validation (user can finish with author's call); invalid typed locator silently keeps old position; terms_text BOM/CR strip each frame; module doc says "file did not exist" but trigger is first_run; clippy allow above doc comment; no test for Cancel→Close; ordering tests count calls only; app.rs 1442 lines; no kittest of drawn wizard.
Task 16: Ruling: doc-accuracy minors (#99 citation, #96 C# outcome, missing rows for watchlist insert-at-0 and horizon draft-My position) taken into fix round 1 — accuracy of findings is this task's deliverable — cost: none.
Task 16: fix round 1/5 (4 addressed, 2 open — README position-DB failure leaves LED green (background.rs:300), #98 ":210" citation wrong (path reads COMPLETE at ElevationDatabase.cs:1407/1481); commits 8da5e25..2a957d4)
Task 16: minor (deferred, final review should weigh): position-DB maintenance failure returns UpToDate → LED stays green (background.rs:300); README bullet wrap inconsistent.
Task 16: fix round 2/5 (2 addressed, 0 open — README position-DB LED wording, #98 citation; commits 2a957d4..ffed201; docs-only 2-line diff re-checked by controller against background.rs:295-302 and the cited C# claims from the round-1 re-review)
Task 16: complete (commits 0006869..ffed201, review clean)
Task 16: minor (deferred): #101 says nearest box shows call/distance/time, brief says Call, Type, Cat, Alt, Angle, Dist — check against ui/spectrum.rs.
Final review (opus, 1b610a3..ffed201): needs fixes — 4 Important (I1 calculator stores ~0 paths because COMPLETE bit never set after download-only ruling, then stamps; I2 background elevation job ignores gui.elevation_download; I3 Options OK doesn't refresh main My/DX locator fields; I4 Multi tab station-DB query per row per frame), 4 pre-merge minors (QRV/location edits + My position not in calculator stamp; N stacked beeps per tick; wizard invalid locator keeps old position silently; #101 text + README range #88..#104), 2 prediction-affecting minors (3a elevation_model → GLOBE-only in 3b; best-case on → local obstruction never found, no finding).
Final: Ruling: Task 11 download-only ruling revised — its cost missed I1; the fix wave decides (with C# ElevationDatabaseUpdater.cs:479/490 as authority) how COMPLETE gets set without a racing second writer — cost if wrong: calculator still stores nothing.
Final: Ruling: fix wave takes all 4 Important + 4 pre-merge minors + the 2 prediction-affecting minors (fix or document) in ONE opus dispatch — by effect on users — cost: larger fix diff.
Final: fix wave (10 addressed claimed; commits ffed201..7b2c866; I1 via calculator-own check_area + Coverage, no shared COMPLETE writer)
Final: re-review (opus): 10/10 addressed; new Important in fix diff: app.rs:1248 qrv_write bumps QRV table timestamp, which the stations updater also uses as last-import marker (as-stations/src/updater.rs:31-36) → user QRV edit during/after a cancelled or failed import blocks the server QRV import (304, needs_import false) → default antenna heights silently. Not fixed (no second fix wave) — surfaced to user before merge.
Final: minor (deferred): unsaved-path rerun for My outside area crossing missing tile (once per start/period, acceptable); check_area may read half-written sea file → harmless Incomplete; has_tiles LIKE query full scan per square.
Finish: user chose option 1, squash merge into main; QRV timestamp Important left open (not fixed before merge).
Post-merge: QRV timestamp Important fixed on branch fix-qrv-user-edit-marker — user QRV edits write their own Table_last_updated key QRV_UserEdit (as_stations::QRV_USER_EDIT_MARKER), the calculator stamp includes it; tests user_qrv_write_leaves_the_import_marker_alone (RED→GREEN), calculator_reruns_after_a_user_qrv_edit.
