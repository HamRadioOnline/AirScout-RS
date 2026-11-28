# SDD ledger — plan: docs/superpowers/plans/2026-11-25-stage6-area1-review.md
Spec: docs/superpowers/specs/2026-11-25-stage6-logic-fixes-design.md (approved 2026-11-25)
Branch stage6-area1-propagation from main 22b17cc (spec fa1c766, plan commit HEAD)
Pre-flight: tasks share one artefact (docs/logic-fixes.md table). T1 produces rows + `TBD (task N)` owner tags; T2–T4 consume by tag. T3 consumes T2's #7 baseline statement (golden deltas measured under current sphere model). No conflicts.
Ruling: docs-only part, gates = verification greps per task + `git diff --stat` shows no crates/ or tests/golden/ change — no cargo gate needed — cost if wrong: none (no code)
Ruling: models — T1 sonnet (mechanical inventory); T2–T4 opus (first-principles maths judgment); reviewers sonnet, final opus
Task 1: implementer dispatched (sonnet), BASE 8609d2e
Task 1: implementer DONE_WITH_CONCERNS 63f5674, 13 area-1 rows (#7 first; T2: 7,15; T3: 1,2,3,11,179,282; T4: 4,5,8,10,289)
Ruling: #276 (antimeridian, Ported 1:1 + deviation) has no own row; Task 2 covers it inside #15 and names #276 there — same defect, one fix — cost if wrong: one extra row later
Ruling: #293/#294 stay area 4 (elevation-tile-gen) per port-location rule — spec rule is explicit — cost if wrong: elevation-data fixes wait until area 4
Task 1: carry → Task 3: horizon_azimuth (crates/as-propagation/src/horizon.rs:176) has a near-field suppression C# lacks; touches #3
Task 1: review spec ✅ quality ✅, 0 Critical/Important, 2 Minor
Task 1: complete (commits 8609d2e..63f5674, review clean)
Task 2: implementer dispatched (opus), BASE 63f5674
Task 2: implementer DONE_WITH_CONCERNS f0da59c; rows #7 (owner), #15 (fix, covers #276); new #343–#348
Ruling: #16 not in Task 2 — Task 1 placed it in area 2 (as-aircraft) per port-location rule; plan text predates inventory — cost if wrong: none, it stays listed under Not area 1
Task 2: carry → Task 3: #344 shares the intersection code with #2; #2's golden delta must say #344 changes 288 propagation_path.json raw points first
Task 2: carry → Tasks 3, 4: golden case ref format `<file>.json:<fn>[i]` = index i of the file's cases array (rule line under the columns table)
Task 2: carry → fix part: WGS84 option needs Karney (Vincenty fails near antipode); a dev-dependency needs owner OK if chosen
Task 2: review spec ✅ quality ✅, 0 Critical/Important, Minors: `<fn>[i]` reads as per-fn index (is whole cases-array index); #344 C# cite :163-166 should be :149 — both to final fix pass
Task 2: complete (commits 63f5674..f0da59c, review clean)
Task 3: implementer dispatched (opus), BASE f0da59c
Task 3: implementer DONE_WITH_CONCERNS eff3fa9; #1 fix, #2 fix, #3 fix, #11 fix, #179 keep, #282 fix; new #349 fix, #350 fix, #351 owner (step width), #352 fix, #353 keep
Ruling: Task 1 carry on horizon_azimuth near-field suppression was wrong — C# has it at PropagationDatabase.cs:867-878 — #3 written on that basis — cost if wrong: none
Task 3: carry → area 3: band-dependent k-factor defaults (1.6..1.33, as-settings) lack a refraction basis — log a row when area 3 starts
Task 3: concern → review: ITU-R section numbers other than P.526-15 §2.1 eq.(2) and the textbook cites are from memory, unverified
Task 3: review spec ✅ quality ✅, 0 Critical/Important; Minors to final pass: Willis Doppler is ch. 6 not ch. 3; P.526-15 §2.1 eq.(2) eq-number unconfirmed (formula checked); Skolnik §3.1 unconfirmed; #1 effect misses as-cli `path` JSON (crates/as-cli/src/main.rs:309); #3/#349 should cite PropagationHorizon.cs:160 as correct precedent; #351 ignores ASTER 5-sample moving average
Task 3: complete (commits f0da59c..eff3fa9, review clean)
Task 4: implementer dispatched (opus), BASE eff3fa9
Task 4: implementer DONE_WITH_CONCERNS 1ba6d6e; #4 #5 #8 #10 #289 all proposed fix (golden none); new #354 fix; area-1 table 25 rows, 0 TBD
Task 4: cache statement — ElevationPath bump: #10 #289 #7B; PropagationPath: #349 #350 + ElevationPath ones; PropagationHorizon: #3 + same; #354 changes both propagation keys
Task 4: concern → review: #8(a) assumes sector i centred on bearing i — reviewer to settle from C#
Task 4: review spec ✅ quality needs fixes — Important: #8 bundles (b) fix with undocumented convention (a); split, (a) owner. #8(a) verdict: not provable from C#
Task 4: minor (deferred): #5 test must check cache contents not only read counts; #289 default-area wording imprecise; stale #5/#289 wording in logic-findings.md; report has no minmax.rs conclusion
Task 4: fix round 1 dispatched (resume implementer)
Task 4: fix round 1/5 (1 addressed, 0 open — #8 split; commits 1ba6d6e..356506c)
Task 4: complete (commits eff3fa9..356506c, review clean after 1 round)
Side: README shortening delegated to Codex (gpt-6.2-sol, worktree) by owner request
Side: README shortened by Codex (810773b in its worktree), cherry-picked signed onto branch; 756 -> 252 lines; controller checked feeds table, ports, privacy claims vs code
Final: review — not ready; 3 Important: #15 effect overpromises (path_bbox guard services.rs:410-412 not in scope); #289 enlarges default area 598→624 squares unstated; cross-row interactions (#7B vs #343/#344/#345/#15, #351B vs #349/#3, #10⊥#289) unstated. Must-fix minors: #1 and #352 effect miss as-cli path JSON (main.rs:309); #179 Willis chapter. Unverified cites to mark: P.526-15 §2.1 eq.(2), Skolnik §3.1, Williams Aviation Formulary section, Karney arXiv:2308.00495, GeodTest.dat DOI
Final: fix wave dispatched (opus, fresh implementer), BASE 803484f
Final: fix wave done ca54218 (all 7 items); #15 now cross-crate (airscout, as-cli), coupled to #289
Final: re-review all 7 ADDRESSED, no new breakage; minors deferred: #352 dist_km int→float in as-cli JSON (state type), integer D gives two points at D
Owner 2026-11-25: list approved; #7 A (keep sphere), #351 B (halve steps), #8(a) centred/round; rest as proposed. Commit d94af34. Review part done.
