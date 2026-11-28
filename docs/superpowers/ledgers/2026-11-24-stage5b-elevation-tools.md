# SDD ledger — plan: docs/superpowers/plans/2026-11-24-stage5b-elevation-tools.md

Branch stage5b-elevation-tools from a7aa1cf; plan commit ca2cc84. Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md; survey + owner decisions docs/superpowers/surveys/2026-11-24-suite-tools-survey.md.
Owner: use a little Gemini 4 Neon (antigravity) as a full council member on a few reviews.
Owner decision 2026-11-24 (S13 downsample): fix both — i32 sum, a void anywhere in the 3x3 block gives a void — keep the C# sample layout 1:1; log the deviation as a finding. Plan approved ("Yes, start").

## Pre-flight
Pre-flight: 23 rows, 16 conflicts — table and rulings in preflight.md (copied into the final ledger).
Ruling: pre-flight C1-C16 adopted as proposed in preflight.md (each with its cost line there) — they follow the real fixtures, the C# source and the rubric; notable: C1 the byte-exact fixture test keeps the plan's number rule, checks all other lines byte for byte, requires equal values on the 189 17-digit lines whose served last digit is one higher (.NET Framework formatting), and pins that count; C6/C7 plan-mandated tests strengthened (distinct-colour pixels; literal expected files.cat); C8 square_rgba accepts only 5×10, 50×100, 150×300 and a valid tile name (crafted tile could force a 2.3 GB allocation).

Task 1: dispatched implementer (opus, to_loc_json + read_files_* + palette_color), BASE ca2cc84 — C1 passed
Task 1: DONE_WITH_CONCERNS 8f9df56 (1176 pass; 2119/2305 byte-exact, exactly 189 differing 17-digit lines, equal values). Concerns: serde_json float_roundtrip enabled (feature unification → workspace-wide); Corrupt error now names the cached zip path, not the URL.
Task 1: Ruling: serde_json `float_roundtrip` accepted — the default parser reads 1709 of 9220 fixture coordinates one ulp off, so no byte-exact writer test is possible without it; correct rounding everywhere is the safer default and every test stays green — Task 7 notes it in finding 5 — cost if wrong: slightly slower float parsing across the workspace.
Task 1: review clean (spec ✅, quality approved; C1 test cannot pass trivially; fetch map identical; palette = C# GetColor)
Task 1: minor (deferred): fixture counts 2305/2119 pinned; read_files_zip builds the full map before retain; to_loc_json chunks(rows.max(1)) writes ragged columns if elv.len() != rows*columns → Task 3 must guard tile shapes
Task 1: complete (commits ca2cc84..8f9df56, review clean)
Task 2: dispatched implementer (opus, golden harness tilegen), BASE 8f9df56 — C2-C5 passed; downsample golden records C# as-is for the S13 fix comparison
Task 2: DONE_WITH_CONCERNS 57b5045 (2027 cases, 1.9 MB, reproducible; C10G 575 written; 1176 pass). Concerns: .NET 8 copy writes Bounds first (Task 3 drops it); CS0109 warning kept verbatim; dsrc/dsrc-ov/cmp-b have no tile cases (Task 3 selects by tile cases); extra downsample_deviation case → Task 5 must test it.
Task 2: review clean (spec ✅, approved; verbatim copies diffed vs upstream; downsample re-implemented in Python matches golden)
Task 2: minor (deferred): Bounds first in members (upstream ElevationTile.cs:111, served tiles from another build → Task 3 drops it, finding says so); CS0109 warning; header omits Visited field and one blank line in IsGLOBEFile
Task 2: carry → Task 3: select tile sources by srtm_tile/globe_tile presence; drop Bounds; guard tile shape (rows*columns) before to_loc_json. → Task 5: a test over downsample_deviation (clean/raw = C#, overflow = i32 sum/9, void = -32768); compare output is little-endian while inputs are big-endian.
Task 2: complete (commits 8f9df56..57b5045, review clean)
Task 3: dispatched implementer (opus, elevation-tile-gen crate + tiles), BASE 57b5045 — Task 2 carries passed
Note: main CI 45488798760 (a7aa1cf) all green incl. hosted windows-latest → stage 5a hosted Windows gate passed (5a Final Ruling I1 satisfied)
Task 3: DONE_WITH_CONCERNS c40a375 (10 new tests; golden matched first run; 4 mutations each caught; 1186 pass; WINGNU_OK). Concerns: write_loc removes its own .part on failure; extra pub globe_block_square; trailing-dot names rejected; no_source_is_error passes --out; finding notes (one LastUpdated per run; mtime only; failed dir create is an error, C# GLOBE swallows it :487-495).
Task 3: Ruling: removing the tool's own .part file on a failed write stays — C11 reads "no tool deletes a file" as user data only — cost if wrong: one remove_file line.
Task 3: review dispatched (Gemini 4 Neon, council seat)
Task 3: review (Gemini 4 Neon) — spec ✅, Approved, 9/10 (score recorded); no Critical/Important
Task 3: minor (deferred): write_loc index check by byte length (lowercase / Kelvin-sign index writes odd path inside --out, library-only); check_len rows*cols*2 unchecked mul on hand-built Source; create_dir_all error lacks the dir; .part removal attempted when File::create failed; check_len and write_loc guards untested; two cite ranges off by one (source.rs:86 → :130-196, :120 → :72-128)
Task 3: complete (commits 57b5045..c40a375, review clean)
Task 4: dispatched implementer (opus, zip + catalogue + mockito e2e), BASE c40a375 — C7 passed
Task 4: DONE_WITH_CONCERNS aca09e9 (1196 pass, 10 new; WINGNU_OK). Concerns: zip verify before --remove-loc not reached by a CLI test; on verify mismatch the new zip already replaced the old (no .loc deleted); extras (TileIndex vs file name check, 16 MB entry cap, 2 tests); symlink skip untested; files.cat written before files.zip
Task 4: review (opus) — spec ✅, quality needs fixes: I1 (partly plan-mandated) a second zip of a square drops tiles an earlier --remove-loc deleted (reproduced; C# same, MainDlg.cs:566-571); I2 no test reaches the verify call before delete
Task 4: Ruling: I1 fixed against the 1:1 plan text — the new zip keeps old-zip entries that have no .loc on disk (a .loc on disk wins), logged as a Port deviation finding in Task 7 — data loss of self-hosted tiles beats 1:1 for a tool step — cost if wrong: an intentionally removed tile reappears from the old zip (delete the zip to drop it).
Task 4: Ruling: M1 (verify the .zip.part before the rename, keep the old zip on mismatch) and M3 (strip_suffix for .loc) join round 1 — cheap and on the same guard path — cost if wrong: a few lines.
Task 4: minor (deferred): M2 no dir sync before deletes; M4 case-sensitive .loc match (JO50MB.LOC skipped, not deleted); M5 e2e expected values via get_elevation (same lookup as store.elevation), no void-cell assert; M6 tile-name check and symlink skip untested
Task 4: fix 134b1c2 (1199 pass; RED shown; WINGNU_OK). pack.rs 575 lines (~145 module tests of crate-private pack_square); every zip run verifies; a broken old square zip now fails the run instead of being replaced.
Task 4: Ruling: pack.rs at 575 lines accepted — the extra lines are module tests that must reach the crate-private pack_square — cost if wrong: a later split.
Task 4: fix round 1/5 (4 addressed, 0 open — I1 old-zip carry-over, I2 verify seam test, M1 verify .part before rename, M3 strip_suffix; commits aca09e9..134b1c2) — no data-loss path found
Task 4: minor (deferred): broken old zip fails its square (manual fix); duplicate flat names in foreign zips keep the first; verify doubles read I/O
Task 4: carry → Task 7: Port deviation finding for the old-zip carry-over (C# MainDlg.cs:566-571 rebuilds from disk only; draft in task-4-report.md "Fix round 1")
Task 4: complete (commits c40a375..134b1c2, review clean)
Task 5: dispatched implementer (opus, compare/downsample/preview), BASE 134b1c2 — C6, C8, C9, C10, C16, S13 and the downsample_deviation carry passed
Task 5: implementer DONE_WITH_CONCERNS (aa4f545) — preview memory grows with input (~90 KB/tile); compare/preview overwrite --out via .part; pack.rs helpers + SHAPES crate-visible; read_file 129.6 MB cap; duplicate tile index = error; main.rs 405 lines
Task 5: dispatched task reviewer (opus), package review-134b1c2..aa4f545.diff
Task 5: review — spec ✅, quality needs fixes: I1 read_tiles keeps tiles of all squares before square/shape check (zip bomb ~20 GB)
Task 5: minor (deferred): duplicate-tile error untested (preview.rs:716)
Task 5: minor (deferred): dir symlink not followed in preview input (preview.rs:722) — state in help/findings
Task 5: minor (deferred): compare/preview overwrite --out like C# (File.Delete :1107) — T7 finding 10 + preview note
Task 5: minor (deferred): hgt.rs:290 unreachable unwrap_or fallback
Task 5: minor (deferred): fnv_bytes duplicates fnv in tests/common
Task 5: minor (deferred): read_file 129.6 MB cap untested (sparse file)
Task 5: minor (deferred): duplicate tile index error vs C# last-wins — T7 preview note
Task 5: fix round 1 — resume implementer with I1
Task 5: fix round 1 done (ea3e970) — I1 read_tiles_stops_at_another_square RED→GREEN; minors 1,6 tested; tests/hgt.rs 589 lines (note)
Task 5: re-review — I1, minor 1, minor 6 ADDRESSED; no new breakage
Task 5: minor (deferred): tests/hgt.rs 589 lines, could split hgt/preview
Task 5: complete (commits 134b1c2..ea3e970, review clean)
Task 6: dispatched implementer (opus, elevation-coverage), BASE ea3e970 — C11, C14, preflight rows 11/22/23 passed
Task 6: implementer DONE_WITH_CONCERNS (04e74d8) — 8/8 coverage tests, gate 1220 passed, WINGNU_OK
Task 6: Ruling: own zip open with 128 MiB cap on files.cat instead of read_files_zip; zip becomes a normal dep of as-cli (no new crate) — bounded-memory rule outranks the brief's helper choice — cost if wrong: a few lines, revert to read_files_zip
Task 6: Ruling: tile_cell also requires is_ascii() beyond C14 — prevents byte-slice panic on non-ASCII digits, rejects no valid locator — cost if wrong: none
Task 6: Ruling: as_net::download_if_newer reads the body unbounded — pre-existing as-net behavior, out of T6 scope; carry to T7 as a finding — cost if wrong: a large or hostile response can exhaust memory on explicit --download only
Task 6: review — spec ✅ (agrees with all 3 rulings), quality needs fixes: I1 128 MiB files.cat cap unverified vs real catalogues (SRTM1 ~112 MB, GLOBE ~178 MB est.) — --download could fail on default server
Task 6: Ruling: no live fetch to measure real files.zip sizes — no network without explicit user action; fix by raising cap to 1 GiB and streaming the entry into read_files_cat through a counted take — cost if wrong: 1 GiB cap still too low (unlikely) or more memory for the parsed map
Task 6: minor (deferred): ElevationStore::open creates other models' .db3 + migrate under --data — T7 README/finding note
Task 6: minor (deferred): no test pre-existing airscout-cli-coverage-{pid} dir left alone; no --level tile with --data run; no --catalogue expect(0) run
Task 6: minor (deferred): write_json leaves <path>.part on partial write failure (target intact)
Task 6: minor (deferred): 300 s client timeout undocumented in help
Task 6: fix round 1 — resume implementer with I1 + cap test
Task 6: fix round 1 done (1047ff0) — 1 GiB counted stream, catalogue_cap_is_inclusive_and_over_is_error RED→GREEN
Task 6: re-review — I1 ADDRESSED, no new breakage
Task 6: minor (deferred): comment says ~33 bytes/line, sample is 32
Task 6: complete (commits ea3e970..1047ff0, review clean)
Task 7: dispatched implementer (opus, docs/findings/gates), BASE 1047ff0 — all T7 carries listed in dispatch
Task 7: implementer DONE_WITH_CONCERNS (7cda256) — all gates green (macOS 1221, hamlib, LINUX_OK on rerun, WINGNU_OK); findings #291–#305 (+#304 old-zip carry-over, +#305 unbounded download body)
Task 7: Ruling: 15 findings instead of 13 accepted — #291–#303 keep brief order and README refs; two carries needed own rows — cost if wrong: renumbering two rows
Task 7: minor (deferred): wintest_service::malformed_datagrams_do_not_stop_receiver (stage 4a loopback UDP) failed once in Linux docker, green on rerun — possible flake on hosted CI
Task 7: review — spec ✅, quality approved
Task 7: minor (deferred): #299 cites :1267, File.Create is :1268
Task 7: minor (deferred): README --histogram 'also writes a CSV' — flag takes a path
Task 7: complete (commits 1047ff0..7cda256, review clean)
Final: dispatched whole-branch review (opus), package review-a7aa1cf..7cda256.diff; spec :85/:87 checked by controller — already match Q6
Final: review — merge with fixes. I1 Catalogue::fetch reads whole files.cat into a map before area filter (memory regression vs old code, untrusted http download). I2 compare overwrites --out/--histogram silently, even an input. M8 doc cites (#299 :1268; source.rs :72-128/:130-196; README --histogram takes a path).
Final: Ruling: I2 fixed against the 1:1 README wording — compare refuses an existing --out/--histogram unless --overwrite, as downsample does; C# writes a fixed name, never a user-typed path, so no C# behaviour to keep — cost if wrong: one extra flag for scripted reruns
Final: Ruling: declined-to-judge items stand — #305 stays an open finding (shared as-net, outside 5b); plain http default matches upstream; harness C# copies verified through golden outputs; no speed requirement for full GLOBE run; coverage GeoJSON schema has no consumer — cost if wrong: later stage work
Final: fix wave dispatched (opus, one fixer): I1, I2, M8; FIX_BASE 7cda256
Final: fix wave done (273366b I1, 4f99679 I2, 4d7907c docs/M8) — gate 1225 passed
Final: Ruling: preview also refuses an existing --out unless --overwrite (fixer's call, accepted) — C# MainDlg.cs:843 writes fixed ElevationTiles.tif; `preview JO50.zip --out JO50.zip` would replace the source — cost if wrong: one extra flag
Final: minor (deferred): tests/hgt.rs 656 lines; Capped/MAX_CATALOGUE_BYTES/read_files_cat_where now pub in as_elevation; M7 elevation-coverage builds full map up to cap before area filter
Final: re-review — I1, I2, M8 ADDRESSED; no new Critical/Important
Final: minor (deferred): compare does not refuse --out == --histogram same new path (pre-existing)
Final: complete (branch a7aa1cf..4d7907c)
Finish: Ruling: fixed stage-5a test race in listen_shares_port_with_receiver (failed 2/2 full workspace runs with AddrInUse; free_udp_port picked then released) by binding port 0 shared and reading it back — controller fix, test-only — cost if wrong: none; full suite 1225/1225 after
