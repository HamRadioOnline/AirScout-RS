# SDD ledger — plan: docs/superpowers/plans/2026-11-23-stage3a-live-map.md

Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md (reachable). Branch: stage3a-live-map, base 032f23f.
Survey source for Task 1 copy: .superpowers/sdd/2026-11-23-stage3a-live-map/gui-survey.md

## Pre-flight

| Tasks | Produces vs consumes | Found |
|---|---|---|
| 1→2,4,6,7,8 | GuiSettings/InfoWin/Area fields | consistent names |
| 1 self | as-settings→as-aircraft dep for PlaneCategory | no cycle (only as-cli depends on as-settings) |
| 2 self | test needs PropagationPath without elevation | `PropagationPath::new(params, eps1, eps2, valid)` is pub — OK |
| 2→7 | TickResult, tick() | consistent |
| 3→8 | PathOverlay, locator_grid, rings, beam_wedge | consistent |
| 4→8,9 | plane_color, plane_icon_px, tooltip_lines(p, info, my) | consistent |
| 5↔6 | profile(p, terrain:&[f64], max_alt) vs PathReady.terrain: Vec<f64> | plan does not say how terrain is sampled → Ruling R1 |
| 5 self | Consumes names `ElevationPath`, signature takes slice | Ruling R1 covers |
| 6→7 | PathService/FeedService concrete; Task 7 wants injectable `trait Services` | Task 7 defines trait + impl for the real services — OK |
| 6 self | moves CLI prepare; CLI tests must stay unchanged | OK |
| 7→8,9 | AppState fields (path, terrain, tick, selected) | consistent |
| 2,10 | findings numbering from #82 | OK |
| 8 self | manual run against OSM/OpenSky | needs network+display; implementer may not have display → Ruling R2 |
| others | self-consistent | — |

Ruling R1: `PathReady.terrain` holds exactly one ground elevation [m] per `path.info_points()` entry (same length, same order), sampled with `store.elevation(model, lat, lon, false)` (missing → 0 like GetElevation); `profile()` asserts equal lengths and treats a mismatch as empty series — why: Task 5 tests compare against info_points; cost if wrong: chart terrain resolution 1 km instead of step width (cosmetic, easy change).
Ruling R2: Task 8 manual run is attempted by the controller after the task, not by the implementer; if no display/network, ledger "manual check skipped" — why: subagents run headless; cost: visual bugs found later.
CI main d8ab2df (run 45185624267): all 4 jobs green incl. self-hosted windows-gnu cross build.
Task 1: complete (commits 032f23f..e0bd387, review clean)
Task 1: minor (deferred): unused-dependency manifest warnings in airscout until later tasks use the crates; re-check at end.
Task 1: minor (deferred): my_calls MRU max 10 not enforced (no push API yet).
Task 1: note: eframe 0.37 uses App::ui(&mut Ui, &mut Frame), not update().
Task 2: Ruling: `shown` filters on Planes_MinAlt (settings.planes_min_alt_m), not Planes_Filter_Min_Alt — C# MapDlg.cs:4976 does so (verified); plan text was wrong — cost if wrong: filter field has no visible effect except highest_potential (as in C#).
Task 2: Ruling: highest_potential also reset per path (C#), covered by finding #82 — 1:1 port policy — cost: MULTI shows last path only (3b concern).
Task 2: Ruling: chart points only with exactly one path (SINGLE) — C# fills them only in SINGLE — cost: none in 3a.
Task 2: Ruling: alarm = first match in hex order; C# last-match over Dictionary order is not reproducible — cost: different plane named in alarm when several qualify.
Task 2: Ruling: TickInput.now = CurrentTime for GetAll and get_nearest_planes (C# uses UtcNow for the latter); equal in LIFE; 3c plan decides for HISTORY.
Task 2: complete (commits e0bd387..43080d4, review clean)
Task 2: minor (deferred): chart gating uses paths.len()==1, C# uses PathMode==SINGLE — 3b (MULTI) must add a mode flag to TickInput.
Task 2: minor (deferred): no test for "higher potential without int_point does not replace"; module doc range UpdatePlanes cites 4956-5229; 500 ms debug timing test could flake on loaded runners.
Task 3: Ruling: beam_width(10 dBi) = 50.1° from the C# formula; plan's 16.82° was a plan error — cost: none.
Task 3: Ruling: hot markers use C# rule max(h1,h2) < max_alt (no > 0), visible polyline 0 < h < max_alt — C# 1:1 — cost: markers may sit outside the magenta segment as in the original.
Task 3: Ruling: grid cap drops the fine level first, empty grid if coarse level alone > 2000 — partial grid looks like a bug — cost: no grid at extreme views.
Task 3: Ruling: rings 361 points (0..=360 as C#); grid clamped to world, no one-cell margin — cosmetic.
Task 3: Ruling (supersedes earlier cap ruling): grid over cap must not go blank — drop the fine level first, then fall back to the next coarser zoom band's cell size until <= 2000 cells — a blank grid at zoom 14 on a large window is a visible defect — cost: coarser grid than C# on huge views.
Task 3: minor (deferred): cell_count/level_cells duplicate index_range; arc radius vs n rounding; possible duplicate point at exactly ±180; sizes() dedup obscure; zero-length path draws nothing (markers still drawn); no pole test.
Task 3: fix round 1/5 (3 addressed, 0 open — grid cap fallback, wrapped views, pinned path tests; commits 42067f2..f1ecd17)
Task 3: complete (commits 43080d4..f1ecd17, review clean)
Task 4: Ruling: tooltip_lines(p, info, _my, now: DateTime<Utc>) — C# Time line is CurrentTime+mins; `my` unused by C# — Tasks 8/9 callers pass AppState now — cost: none.
Task 4: Ruling: units follow InfoWin.metric only (C#); imperial flag ignored; category None → Medium size (C#).
Task 4: Ruling: plan-mandated test tooltip_without_crossing_has_no_dist is wrong vs C#: planes with potential not 100/75/50 use CreatePlaneSimple (MapDlg.cs:4751-4820, chosen in DrawPlanes 4980-5006) — tooltip_lines picks Simple vs Detailed by potential, test fixed to the Simple output — C# authority — cost: none.
Task 4: minor (deferred): comment that Dist 193mi for 120 km is finding #16; unused _my param; Duration overflow unreachable.
Task 4: fix round 1/5 (1 addressed, 0 open — Simple tooltip for gray planes; commits 8143c95..ad58780)
Task 4: complete (commits f1ecd17..ad58780, review clean)
Task 4: minor (deferred): no explicit test that potential 50 gets the Detailed tooltip.
Task 5: complete (commits ad58780..3b110a7, review clean)
Task 5: minor (deferred): min_h_area expectation same formula as code; no-info-points test via step_width_m<=0; doc that empty series → y_max 0; y_max for negative peaks (C# form) undocumented.
Task 6: Ruling: model None → PathFailed("no elevation model selected") in the service (store has no None DB). Task 7 must port SetElevationModel (MapDlg.cs:3752): C# default Elevation_GLOBE_Enabled=True ⇒ effective model GLOBE; 3a has no *_Enabled flags yet, so AppState uses settings.elevation_model, or GLOBE when it is None — cost: user with None gets GLOBE downloads (as C# default).
Task 6: Ruling: prepare_area always runs, download flag = gui.elevation_download (it marks the model complete; needed for validity) — cost: none.
Task 6: Ruling: ground-elevation fallback walks ASTER1..GLOBE treating all models enabled (no *_Enabled flags in 3a; 3b revisits) — [amended: stop-at-TILE_MISSING now matches C#, see fix round 1] — cost: station height from another model than C# would use when flags would disable one.
Task 6: Ruling: local obstruction always f64::MIN (no obstruction data in store yet; LocalObstructionsDlg is 3b).
Task 6: Ruling: running path computation not cancelled; burst may yield PathReady for first and last request, middle dropped — cost: one wasted computation.
Task 6: Ruling (supersedes fallback ruling): ground_elevation reproduces C# GetElevation (MapDlg.cs:3649): move to the next model only on ELV_MISSING, stop at TILE_MISSING (→0); a model without DB/Err falls through to the next — port policy — cost: none.
Task 6: minor (deferred): spawn(..).ok() swallows thread-spawn errors (services.rs:244, :532); Drop/stop joins path thread → exit may hang during downloads (300 s timeout) — revisit in Task 7 shutdown; path_bbox duplicates CLI bbox; tmp_dir not created; set_bounds is shared global state per request; ServiceEvent::Error never emitted.
Task 6: fix round 1/5 (3 addressed, 0 open — stale COMPLETE bit, deterministic newest_request_wins, C# GetElevation walk; commits 43ccb05..5509578)
Task 6: complete (commits 3b110a7..5509578, review clean)
Task 6: minor (deferred): pause/resume test hooks are pub(doc hidden) API.
Task 7: Ruling: collapsible group boxes deferred (cosmetic); Multi tab and status LEDs are 3b scope — cost: none for 3a.
Task 7: Ruling: exit detaches PathService join into a background thread; process may exit mid-computation — DB writes are per-path transactions — cost: a half-done path is recomputed next start.
Task 7: minor (deferred): late Feed(Stopped) can show "Feed: off" after quick pause/play; Play doesn't select Single tab (C# 4002); band combo/filter disabling untested in kittest; tick_due timing untested; FeedService::start opens SQLite on UI thread; ui writes side_panel_width into settings each frame; repaint every 250 ms while idle; ./data fallback path silent.
Task 7: fix round 1/5 (1 addressed, 0 open — path request generation id; commits 076c36c..62eeee9)
Task 7: complete (commits 5509578..62eeee9, review clean)
Task 7: minor (deferred): pause does not bump path_gen (current path still accepted while paused — intended).
Task 8: Ruling: beam wedge uses default_antenna_gain(band) (no QRV gain in AppState yet; 3b station dialog adds it); wedge length = path distance — cost: wedge width differs from a user's real antenna.
Task 8: note: ui/map.rs 601 lines (>500 guideline) — reviewer to judge; manual OSM/OpenSky run pending (R2, controller).
Task 8: Ruling: Thunderforest (OCM Landscape) omitted in 3a — needs a user API key; 3b options dialog may add it with a key setting — cost: one provider fewer.
Task 8: Ruling: overlays drawn in Map::show closure instead of walkers plugins (same projector) — accepted; first frame zooms to settings.area extent — accepted.
Task 8: minor (deferred): selected plane culled off-screen loses line/tooltip; Fit/bbox and beam wedge not antimeridian-aware; attribution link hard-coded OSM; UA HeaderValue failure silent (.ok()); first-frame zoom not capped / overrides pending zoom; Fit+auto-center uses great-circle mid not bbox centre; ui/map.rs 601 lines (move pure geometry out); per-frame small Vec allocations; leftover unused-dep warnings (egui_plot until Task 9, as-signals workspace).
Task 8: fix round 1/5 (1 addressed, 0 open — hit_plane/toggle_selection extracted and tested; commits 9b5dc71..2e736a8)
Task 8: complete (commits 62eeee9..2e736a8, review clean)
Task 8: minor (deferred): no kittest click wiring test; ui/map.rs 517 lines; empty-map click does not deselect.
Task 9: complete (commits 2e736a8..b47d8d5, review clean)
Task 9: minor (deferred): no hex tiebreak in plane sort; plane_rows and profile series rebuilt every frame (cache per tick/path_gen); profile only smoke-tested; bottom height persist-on-change untested; only Call cell clickable; My/DX labels centre-anchored; Min_H fill full magenta α0.25 vs C# desaturated.
Task 10: Ruling: Step 1 "push to a branch" not done — user rule "push only when asked"; CI equivalence via local macOS gate + Linux docker gate; Windows cross build verified in CI after the user's push — cost: windows-gnu breakage found later.
Task 10: Ruling: ledger copy to docs/superpowers/ledgers/ done by the controller after the final review (ledger still grows).
Task 10: complete (commits b47d8d5..d85abda, review clean) — macOS gate green; Linux docker gate green on 2nd run (1st: ld signal 9 OOM, CARGO_BUILD_JOBS=2 + debug=0 passed).
Task 10: minor (deferred): README Linux data path example should be lowercased (~/.local/share/airscout-rs); OpenTopoMap usage policy not mentioned; linker OOM risk on small runners.
Final review: With fixes. 0 Critical, 5 Important, 8 Minor.
Final: Ruling: Important 3 — add a Path_MaxLength guard on Play (refuse, status message, DX field marked) although C# SINGLE does not block — it is a GUI-layer guard that prevents mass elevation downloads on a locator typo; Review Focus #3 demands it — cost: a user cannot start paths > Path_MaxLength (default 1000 km) without raising the setting.
Final: Ruling: Important 4 (manual end-to-end run) — not done by the controller (would open a window and hit OSM/OpenSky/airscout.eu from the user's desktop unasked); handed to the user at finish — cost: tile/TLS/feed issues found by the user.
Final: Ruling: Minor 2 (prepare_area Err leaves COMPLETE bit and bounds stale) re-graded Important by effect — same failure class as the Task 6 critical (wrongly valid path stored) — enters the fix wave.
Final: Ruling: README Linux path enters the fix wave (reviewer: must-fix, trivial).
Final: minor (deferred): feed error stays in status after recovery; alarm_play_sound default true but no sound (doc line); OpenTopoMap attribution URL; map tooltips use Utc::now() not tick time; window geometry saved while minimized/off-screen.
Final: fixed 1-6 (elevation note, visible alarm, Path_MaxLength guard, feed generation id, prepare_area continue-on-error, README) — commit c6c1cc5; re-review: all addressed, no new Critical/Important.
Final: minor (deferred): AreaStatus.errors (download cause) not shown in the PathNote; with network down Play tries every missing square (one timeout each); old feed can write one batch into the cache after Pause.
