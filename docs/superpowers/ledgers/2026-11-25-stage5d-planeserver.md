# SDD ledger — plan: docs/superpowers/plans/2026-11-25-stage5d-planeserver.md

Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md (:83). Branch stage5d-planeserver from main adde859; plan commit 475af63.
Owner: plan approved 2026-11-25 ("Jo"); tray on all OS with Linux gtk packages in CI (owner 2026-11-25).

## Pre-flight
Pre-flight: scan by Gemini 4 Neon (council seat, high) in preflight.md — tables (a) shared interfaces, (b) per-task agreement, (c) C1–C18, (d) tray risks per OS. Controller spot-checked C14 (eframe 0.37 epi.rs:170 logic / :185 ui, no update) — confirmed.
Ruling: pre-flight C1–C18 adopted as proposed in preflight.md (each with its cost line there); notable: C1 manual PartialEq on WebConfig with fn_addr_eq; C2 re-export planes_json from airscout::web_service and pin "version":1 in tests/web_service.rs:135; C3 use public as_feeds::feeds::airscoutserver::parse(text, now), as-feeds untouched; C5 WebServer::join added in Task 1; C6 one FeedEvent channel + forwarder per slot; C7 Status.db_error, tests set cfg.data_dir to a tempdir; C8 Service::start_with_web(cfg, feeds, web_cfg) for short test timeouts, 1 MB header + POST checks; C9 shared feed_needs_disclaimer(f) = !enabled_by_default || disclaimer().is_some(), fallback text for BinCraft; C10 save -> Result<(), ConfigError>, non-finite floats reset, min_lat = 95 in test; C11 Task 2 declares all headless deps; lib.rs/Cargo.toml in Files of Tasks 3–5; C12 ctrlc 3 with termination, no tokio feature; C13 start_lines(cfg, bound) and poll local_addr up to 2 s; C14 tray polling in App::logic, start_in_tray on frame 2 only with tray; C15 Linux gtk dep matching tray-icon, menu updates on GTK thread, xdo.h check; C16 tray built in app_creator, event handlers request_repaint; C17 ServiceHandle trait, typed settings widgets, Role::PasswordInput, RGBA icon, port 1..=65535; C18 cites corrected per preflight — cost if wrong: per row in preflight.md
Task 1: dispatched implementer (sonnet), BASE 475af63
Task 1: implementer DONE edf245a, 1308 tests
Task 1: review clean (spec ✅, quality approved), 5 minor
Task 1: carry → Task 6: fix PLANES_JSON_VERSION doc cite in crates/as-web/src/planes.rs to AirScout/Properties/AssemblyInfo.cs:35 (1.4.5.0, major 1), writer MapDlg.cs:8481
Task 1: minor (deferred): WebServer::join swallows a panicked web thread without a doc line; custom_router_used leaves servers to Drop; airscoutserver_reads_it does not assert reg/alt; as-web now depends on as-wintest for fixed (layering)
Task 1: complete (commits 475af63..edf245a, review clean)
Task 2: dispatched implementer (sonnet), BASE edf245a
Task 2: implementer DONE_WITH_CONCERNS 18e08b6 (unused as-web warning until T3; parse error path only; max_error_dist reset) — concerns benign, to review
Task 2: security scan flagged save() temp file mode 0644 with credentials (as-settings Settings::save same pattern) — passed to reviewer as input
Task 2: review clean (spec ✅, quality approved), 5 minor; reviewer rates security-scan file-mode finding real, Low
Ruling: fix the file-mode finding now in Task 2 for both writers (PlaneServerConfig::save and as_settings::Settings::save): temp file created 0600 on Unix (cfg(unix) OpenOptionsExt, create_new), existing mode not widened — both files hold feed credentials; outside the Global Constraint "cfg only for tray" but justified — cost if wrong: one cfg(unix) block per writer
Task 2: fix round 1 dispatched (resume implementer): 0600 temp file both writers, tmp cleanup on rename error, tests; FIX_BASE 18e08b6
Task 2: fix round 1 done 0029a27 (shared as_settings::write_private_atomic, 1324 tests)
Task 2: re-review round 1 addressed, 2 minor
Task 2: minor (deferred): write_private_atomic uses create+truncate after removing stale tmp, not create_new (race needs write access to the config dir); Settings::save doc does not mention owner-only; parse error names path only (no line/col); unused-dep warning until Task 3 (check after T3); load_or_default does not check feed names or bind (T3 handles names)
Task 2: complete (commits edf245a..0029a27, review clean)
Task 3: dispatched implementer (opus), BASE 0029a27
Task 3: implementer DONE_WITH_CONCERNS afd8bf7 (stop detaches hung worker after 1.5 s; helper-thread spawn fail; failed-slot name from cfg position), 1335 tests, 3x stable
Task 3: review clean (spec ✅, quality approved), 7 minor; detached hung worker judged harmless (writes only old cache/status, one upsert to aircrafts.db3; port freed at stop)
Task 3: carry → Task 5: test restart on the same port (stop service, start a second on the same bound port) — Apply relies on it; status tab shows last_error only if newer than last_ok (last_error never cleared)
Task 3: minor (deferred): hang test uses fixed 1500 ms sleep (may not hit the hang on loaded CI); stop doc omits the detached worker's last db upsert and HTTP-timeout lifetime; joiner spawn-fail path uncommented; Err row name from cfg position implicit, load_settings error lacks feed name; status() holds Shared lock during get_all scan
Task 3: complete (commits 0029a27..afd8bf7, review clean)
Task 4: dispatched implementer (sonnet), BASE afd8bf7
Task 4: implementer DONE 230e0a8 (ctrlc + nix in lock), cli 8/8 x3
Task 4: review clean (spec ✅, quality approved), 6 minor; both implementer concerns accepted
Task 4: minor (deferred): hand-edited feed name echoed verbatim to stderr; failed bind still prints "serving" line (and :0 for --port 0 when unbound); live-GET test has 1 s slack and an untimed final wait(); sub-1 s status changes not printed; SIGTERM path untested (manual check only)
Task 4: complete (commits afd8bf7..230e0a8, review clean)
Task 5: dispatched implementer (opus), BASE 230e0a8
CI main adde859 (run 45576143690): failure — (1) self-hosted linux: elevation-tile-gen tests/pack.rs self_hosted_tree_feeds_the_updater, child killed by assert_cmd timeout(120 s) (signal 9) on the slow runner; (2) macos-latest: airscout tests/track_service.rs:443 input_change_seen_next_step, race — track_service.rs:246-251 publishes `shown` before applying doppler, test reads shown then doppler.
Ruling: fix both on this branch after Task 5 (not while the T5 implementer builds): track worker applies doppler before publishing shown (real ordering bug, a reader may see the new status with the old doppler); pack.rs run() timeout 120 s -> 600 s (hang guard, not a speed check) — cost if wrong: main stays red until 5d merges
Task 5: implementer DONE_WITH_CONCERNS 633682e (toml downgrade fixed via proc-macro-crate 2.0.0 pin; last_error_at missing; overrides beat Apply; tests/cli.rs changed; tray untested), 1353 tests, LINUX_OK WINGNU_OK
Task 5: review — spec ❌ (Task 3 carry last_error freshness only approximated), Needs fixes: I1 last_error_at missing (gui guesses age), I2 Wayland ignores set_visible (close-to-tray dead, start_in_tray shows window); 8 minor
Task 5: minor (deferred): M1 frame counter counts egui passes, comment inaccurate; M2 Apply stops service on UI thread (freeze up to ~2 s); M3 ci-prepare comment wrong, only libgtk-3-dev needed to build (xdo/appindicator dev harmless); M4 GNOME without AppIndicator extension: invisible tray, close/start_in_tray leave app unreachable (README in T6); M5 missing AppIndicator runtime: panic text, "tray thread ended" names no cause; M6 restart_on_same_port sends no request before stop (TIME_WAIT); M7 apply test does not check stop-before-start; M8 proc-macro-crate 2.0.0 pin held only by lock, a plain cargo update brings back toml 0.8.2 — check toml after any update
Task 5: carry → Task 6: README notes for M4 (GNOME AppIndicator extension) and Wayland hide limit; keep docker apt line consistent with ci-prepare.sh
Task 5: fix round 1 dispatched (resume implementer) I1, I2; FIX_BASE 633682e
Task 5: fix round 1 done cbdba9b, 1358 tests
Task 5: fix round 1/5 (2 addressed, 0 open — I1 last_error_at, I2 Wayland close/start_in_tray; commits 633682e..cbdba9b)
Task 5: minor (deferred): Wayland without tray still minimises on start_in_tray; no test for Wayland start_in_tray or Minimized(false) on Show; WAYLAND_DISPLAY alone may misdetect when winit falls back to X11
Task 5: complete (commits 230e0a8..cbdba9b, review clean after round 1)
CI-fix: dispatched implementer (sonnet) for the two main CI failures, BASE cbdba9b
CI-fix: DONE_WITH_CONCERNS 461719e, e5e25ca (race evidence 1/200 before, 0/200 after; tiles.rs, hgt.rs guards also raised), 1358 tests
CI-fix: review approved, 3 minor
CI-fix: minor (deferred): no test pins doppler-before-shown order (by construction; a hook not worth it); 0/200 evidence weak statistically; root cause of the 120 s kill (slow self-hosted runner) not profiled, 600 s guard could hide a real slowdown
CI-fix: complete (commits cbdba9b..e5e25ca, review clean)
Task 6: dispatched implementer (sonnet), BASE e5e25ca
Task 6: implementer DONE_WITH_CONCERNS 8625345, findings #325–#338, LINUX_OK WINGNU_OK, 1358 tests
Ruling: labels "Not ported"/"Not reproduced" accepted for #326, #327, #333 (used before at #312, #313); no findings rows for the doppler-order fix and the 600 s test guard (not C# deviations; earlier Rust-side fixes have none) — cost if wrong: two missing rows
Task 6: review — spec ❌ I1 (#326 cites DatabaseUpdater.cs:259-266, URLs are at :253-257, loop :249-260); 9 minor
Ruling: Task 6 minors M1 (bind not IPv4-only), M2 (PublicBinCraft has a disclaimer too), M4 (#269 not marked amended), M5 ("(C9)" internal ref, "only way" wrong), M7-Windows (console window note), M8 (ci-prepare comment mixed) re-graded to the fix round — they are false or internal statements in public docs a user acts on; one-line edits — cost if wrong: one extra doc round
Task 6: minor (deferred): M3 OpenSky token POST not in socket row; M6 weak cites #335/#336; M7 systemd After= no effect in user manager, launchctl load legacy; M9 one long README line
Task 6: fix round 1 dispatched (resume implementer): I1 + M1 M2 M4 M5 M7-Windows M8; FIX_BASE 8625345
Task 6: fix round 1 done 7569e67
Task 6: fix round 1/5 (7 addressed, 0 open — I1 #326 cite, M1 M2 M4 M5 M7 M8; commits 8625345..7569e67)
Task 6: minor (deferred): README:643 long unwrapped line
Task 6: complete (commits e5e25ca..7569e67, review clean after round 1)
Final: whole-branch review dispatched (opus), MERGE_BASE adde859
Final: verdict ready with fixes — I1 Linux binary links GTK 3, --headless (Raspberry Pi systemd unit) fails without libgtk-3-0, README silent; promote Task 4 minor "failed bind still prints serving line" (Review Focus #5); optional M3 OpenSky token POST in socket row
Ruling: fix wave = I1 as a README fix (runtime libgtk-3-0 needed also headless) + promoted serving-line minor + M3; a cargo feature gate for the tray is not done — owner decided tray on all OS, not behind a feature (2026-11-25); surfaced to the owner at finishing — cost if wrong: headless-only Pi users install ~GTK runtime
Final: minor (deferred): m2 headless status line keeps stale last_error; m3 window mode broken config exits 2 with stderr only; m4 main.rs lacks #![forbid(unsafe_code)]; m5 no hint for a restricted feed already in config; m6 duplicate feed names allowed
Final: fix wave dispatched (sonnet), FIX_BASE 7569e67
Final: fix wave done 37f96f1, 1360 tests
Final: re-review — I1, promoted minor, M3 all ADDRESSED; no new Critical/Important
Final: minor (deferred): start line may say "not serving: starting ..." if the bind takes longer than the wait (later server: line reports the change); error text twice on stderr (start line + tracked server line)
Final: full suite cargo test --workspace --locked → 1360 passed, 0 failed
