# SDD ledger — plan: docs/superpowers/plans/2026-11-25-upstream-sync-fbadef0.md

Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md (port policy). Branch upstream-sync-fbadef0 from main bd0f92d; plan commit 0b52c83.
Owner: approach approved 2026-11-25 ("Jo"); U5 no finding.

## Pre-flight
| Pair | Shared | Found |
|---|---|---|
| T1 → T2 | docs/logic-findings.md: T1 corrects cites into OpenSky.cs/ADSBOne.cs/PublicBinCraft.cs at fbadef0; T2 changes header, URL row :357, adds #339+ | consistent: T2 writes after T1, header sentence names both commits |
| T1 self | tests at core.rs:37, json_feeds.rs:529 updated with the new ADSBOne default; checkout move before cite check | consistent |
| T2 self | README rows and findings only, full gates | consistent |
Pre-flight: clean (controller scan; plan small, no Gemini seat used)
Task 1: dispatched implementer (sonnet), BASE 0b52c83
Task 1: implementer DONE_WITH_CONCERNS a10dfb9, 1366 tests; no cite shifted; golden harness unaffected
Ruling: tests/core.rs:37 keeps the old ADSBOne template — it tests %MAXDISTKM% substitution, not the default — cost if wrong: none
Task 1: carry → Task 2: U2 finding notes the C# Load runs two older URL fixes (OpenSky.cs:179, :187) before the new replace (:194); the port's single replace gives the same final value
Task 1: reviewer dispatched (sonnet), package review-0b52c83..a10dfb9.diff, report task-1-review.md
Task 1: review spec ✅ quality ✅, 0 Critical/Important, 4 Minor (task-1-review.md)
Task 1: complete (commits 0b52c83..a10dfb9, review clean)
Task 2: implementer dispatched (sonnet), BASE a10dfb9; controller runs Linux+Windows docker gates in background (scratchpad/gate-linux.log, gate-win.log)
Owner 2026-11-25: 'Push, wenn soweit' — after final review + fix wave: ff-merge to main, push main
Task 2: implementer DONE 0d9b7ad, macOS+hamlib gates green
Task 2: review spec ✅ quality ✅, 0 Critical/Important, 1 Minor (#340 "same final value" inexact for pre-V1.4.4.0 XML)
Ruling: fixed the #340 Minor wording in controller commit (docs + comment only) — a false claim in a finding row misleads stage 6 — cost if wrong: none
Task 2: complete (commits a10dfb9..HEAD, review clean)
Final: whole-branch review dispatched (opus), package review-bd0f92d..89ff24e.diff, report final-review.md
Final: review ready to merge, 0 Critical/Important
Final: minor (deferred): tools/fetch-upstream.sh pins 91b02cb; fbadef0 cites need a manual checkout (note, keep pin for goldens)
Final: minor (deferred): publicbincraft.rs:72-73 info() still says "public tar1090 globe site" though default is self-hosted
Final: minor (deferred): PublicBinCraft error test frees port on 127.0.0.1 but connects to localhost (IPv6 resolution possible)
Owner 2026-11-25: after push, start stage 6
Final: docker gates LINUX_OK, WINGNU_OK; cleaned
