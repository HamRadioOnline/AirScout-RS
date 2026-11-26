# SDD ledger — plan: docs/superpowers/plans/2026-11-25-stage5c-maps.md

Branch stage5c-maps from eee2025; plan commit a661457. Spec: docs/superpowers/specs/2026-11-23-airscout-rs-design.md; survey + owner decisions docs/superpowers/surveys/2026-11-24-suite-tools-survey.md (Q3, Q6). Plan approved by owner 2026-11-25 ("Jo, passt"); method SDD, a little Gemini 4 Neon.
Pre-flight: dispatched to Gemini 4 Neon (antigravity, thought_level high) — writes preflight.md
Pre-flight: table + C1–C22 in preflight.md (Gemini 4 Neon: 7 Critical, 10 Important, 5 Minor)
Ruling: pre-flight C1–C22 adopted as proposed in preflight.md (each with its cost line there) — each checked against walkers 0.66, chrono 0.4, background.rs, app/shell.rs and the C# sources; notable: C1 maps.json written by MapsCases.cs directly ({upstream_commit, db_ids, ranges}); C2 CacheTime 7-digit fraction formatted by hand (chrono has no %.7f); C3 golden stores raw doubles (arm64 vs x86 int cast of ±Inf), preload_tiles clamps, polar test asserts exact non-empty counts; C6 own spawn_map_preloader (spawn_job with led None drops progress); C7 as-maps in workspace deps, Task 4 may update Cargo.lock; C8 store opened in app/shell.rs with_settings, AppState.map_store, with_map_store builders, MapStationDlg::new unchanged; C9 schema check before WAL, BEGIN IMMEDIATE; C10 insert takes Option<NaiveDateTime>; C11 shared is_png_or_jpeg used by import and preloader; C12 preload_tile_count O(1) memory for the Options estimate; C17 spec :149 updated in Task 6 (store not written by browsing); C22 README gives ~75k tiles for z 5..11, Delete All Map Tiles / --clean leave maptiles.gmdb — cost if wrong: per row in preflight.md
Task 1: dispatched implementer (sonnet, golden), BASE a661457 — C1, C3, C21 passed
Task 1: implementer DONE_WITH_CONCERNS (e58e927) — DbIds OSM 1492776782, OpenTopoMap 1138496160, OCM Landscape 1581784214; default area z11 56026, z5..11 74916; jq needs parentheses
Task 1: Ruling: brief jq precedence typo — check form is jq -c '[(.db_ids|length), (.ranges|length)]' — cost if wrong: none
Task 1: review — spec ✅, quality approved
Task 1: minor (deferred): MapsCases.cs:60 int subtraction before long cast (no wrap for these areas)
Task 1: minor (deferred): DbId formula exactly at GMapProvider.cs:234
Task 1: carry → Task 2: lon 180 gives xmax = 2^z; lat 90 gives negative ymin; lat -90 gives +Inf ymax — all handled by C3 clamp
Task 1: complete (commits a661457..e58e927, review clean)
Task 2: dispatched implementer (opus, as-maps), BASE e58e927 — C2, C3, C7, C9, C10, C11, C12, C18, C21 + T1 carry passed
Task 2: implementer DONE_WITH_CONCERNS (dd325e0) — 24 tests, gate 1249 passed; unused workspace dep warning until T3; open() switches an existing C# file to WAL, no index added
Task 2: review — spec ✅ (⚠️ unverified: System.Data.SQLite default may write FFFFFFF trimmed, not .7f; both forms read fine), quality approved
Task 2: Ruling: ⚠️ CacheTime text form — accepted as is; both forms parse on both sides, source not in target/upstream — cost if wrong: C# reads our 7-digit text fine anyway
Task 2: Ruling: TileStore::open sets WAL only when it creates the tables (new/empty file); an existing C# Data.gmdb keeps its journal mode (GMap left WAL off, SQLitePureImageCache.cs:176-183) — carried to Task 3 as a small as-maps change with a test — cost if wrong: slower concurrent preloader writes on a user-supplied C# file
Task 2: carry → Task 3: tests pinning size-check-before-read (get_refuses_oversized_blob variant) and insert atomicity (RAISE(ABORT) trigger on TilesData → count()==0); export reports skipped rows (count vs list)
Task 2: minor (deferred): CacheTime write always .0000000 fraction (plan-mandated)
Task 2: carry → Task 6: clamp outside Mercator — an area wholly above 85.05° gives one row at y=0, state in the deviation row
Task 2: complete (commits e58e927..dd325e0, review clean)
Task 3: dispatched implementer (opus, maptile-gen), BASE dd325e0 — C10, C11, C16, C19, C21 + T2 carries (WAL only on create, size-check and atomicity tests, export skip count) passed
Task 3: implementer DONE_WITH_CONCERNS (e39627e WAL fix, 88977b0 maptile-gen) — gate 1262 passed; cargo tree no reqwest/hyper/tokio; WINGNU_OK (maptile-gen + as-maps only, tests compiled not run)
Task 3: Ruling: combine caps height 65,535 px and canvas 256 MiB beyond the brief — bounded allocation for untrusted input — cost if wrong: a legit giant mosaic refused
Task 3: Ruling: full-workspace Windows gate deferred to Task 6 as planned — cost if wrong: a Windows-only break found later in T6
Task 3: minor (deferred): symlink skip untested (needs cfg(unix)); combine ordinal vs C# culture sort; gmdb import stops on non-size SQL errors; one test written after code
Task 3: review — spec ✅ (⚠️ Windows runtime of set_modified/close/rename; full Windows gate in T6), quality needs fixes: I1 export panics on Windows for CacheTime past ~year 30828 (SystemTime overflow)
Task 3: minor (deferred): empty/junk dir import leaves an empty maptiles.gmdb; PROVIDERS duplicates as_maps::db_id names; sync_all before set_modified order; tests for palette/gray combine, caps, from_gmdb TooLarge, --type on tree import
Task 3: fix round 1 — resume implementer with I1 + minors 1 (comment), 2 (gmdb key checks), 4 (skip read_dir errors)
Task 3: fix round 1 done (90ebe5a) — I1 checked time conversion (1601..9999) RED→GREEN; minors 1,2,4; same panic fixed in import mtime (untested, macOS clamps); dir-skip untested (cfg(unix))
Task 3: re-review round 1 — I1, minors 1,2,4 ADDRESSED; new Important: local_instant fallback (export.rs:27-29) panics near chrono max year (CacheTime REAL 97466824.49, TZ=America/New_York)
Task 3: Ruling: fix at the source too — as-maps cache-time parsing returns None outside years 1601..=9999 — garbage times beyond that range carry no meaning — cost if wrong: none
Task 3: fix round 2 — resume implementer
Task 3: fix round 2 done (9d1bbbe) — as-maps None outside 1601..9999, checked DST fallback; TZ=America/New_York test RED→GREEN
Task 3: re-review round 2 — ADDRESSED, no remaining time panic paths
Task 3: complete (commits dd325e0..9d1bbbe, review clean)
Task 4: dispatched implementer (opus, store-first source), BASE 9d1bbbe — C4, C5, C7, C8, C13, C20 passed
Task 4: implementer DONE_WITH_CONCERNS (e708efd) — 12 tests, gate 1278 passed, hamlib clean; memo cache up to 1024 blobs (worst 4 GiB); store errors cached as misses; empty maptiles.gmdb created at first start (T6 README)
Task 4: review — spec ✅ (⚠️ single Mutex<Connection>: preloader must not share app's map_store), quality approved
Task 4: carry → Task 5: preloader opens its own TileStore connection on store_path (never AppState.map_store) — UI thread must not wait on the preloader's mutex
Task 4: minor (deferred): hit memo holds image bytes (worst 4 GiB with hostile 4 MiB blobs; only misses need memoising?); a store error does not stop later lookups in the same frame (2 s per new tile under a long lock); no test for tile_layer db_id wiring; store_error_falls_back does not assert memoised error
Task 4: carry → Task 6: README notes the empty maptiles.gmdb created at first start, left by --clean
Task 4: complete (commits 9d1bbbe..e708efd, review clean)
Task 5: dispatched implementer (opus, preloader), BASE e708efd — C6, C7, C12, C14, C15, C20, C21 + T4 own-connection carry passed
Task 5: implementer DONE_WITH_CONCERNS (4995f74) — gate 1301 passed, hamlib clean; 16 new tests RED first
Task 5: Ruling: redirects followed only on the mirror's own host — keeps the "mirror only" rule (owner Q3) — cost if wrong: a moved mirror on another host stops working until the URL setting changes
Task 5: Ruling: "finished" text after every run, periodic runs too (C# only after the loop) — visible progress beats 1:1 for a status text; T6 logs it — cost if wrong: one status line per period
Task 5: Ruling: max-zoom clamp in a field deserializer instead of load_or_default — same effect, also rejects nothing (300, -4 clamp) — cost if wrong: none
Task 5: carry → Task 6: deviations redirect-same-host and finished-text-per-run; store error text overwritten by the finished text as C#
Task 5: minor (deferred): two background tests written after wiring; Map tab click in kittest did not switch tab (test sets tab directly; every_tab_renders checks only the last tab)
Task 5: review — spec ✅, quality approved; kittest Map-tab miss = Modal open animation (harness), not a UI bug
Task 5: Ruling: Minor 2 (hand-edited map_mirror_url could point the bulk preloader at tile.openstreetmap.org or another provider) enters Task 6 as a small code fix — preload gate refuses the known provider hosts, with a test — owner Q3 forbids those hosts — cost if wrong: one host check
Task 5: carry → Task 6: deviation row "one tile at a time" notes a detached old thread may finish one GET (≤30 s) after an Options restart
Task 5: minor (deferred): every_tab_renders cannot fail (first click lost during Modal animation; pre-existing tests/ui_options.rs:143-169); redirect policy compares host only, not port/scheme; transient ~150 MiB tile list for a world area at z 11 (brief-mandated, as C#)
Task 5: complete (commits e708efd..4995f74, review clean)
Task 6: dispatched implementer (opus, docs/findings/gates + provider-host guard), BASE 4995f74 — C17, C21, C22 + all T2–T5 carries passed
Task 6: implementer DONE_WITH_CONCERNS (cf8cc06 host guard, 3611b99 docs) — all gates green (macOS 1302, hamlib, LINUX_OK no flake, WINGNU_OK); findings #306–#324; spec :149-152 updated (C17)
Task 6: Ruling: S18a reversed to "Not reproduced" pending review — implementer says check and write go to the same GMap Data.gmdb by default — reviewer verifies against C# — cost if wrong: a wrong finding row
Task 6: Ruling: 19 findings instead of 15 accepted; README privacy note (tile requests reveal the settings area to www.airscout.eu) accepted — cost if wrong: renumbering rows
Task 6: review clean (spec ✅, quality approved). S18a reversal verified against C# (MapDatabase.cs:25/61/69-79, GMap Cache.cs:45, SQLitePureImageCache.cs:144-159; same Data.gmdb by default).
Task 6: minor (deferred): #310 could note GMap Cache.cs:140-160 roaming %APPDATA% fallback where stores split
Task 6: minor (deferred): #306 cite :36-37 points at text-box lines; binding is in designer file
Task 6: minor (deferred): last-two-labels domain rule has no comment on its limits (.co.uk, IP literals, CNAMEs)
Task 6: minor (deferred): no test for background.rs refused-provider-host branch
Task 6: complete (commits 4995f74..3611b99, review clean)
Final: whole-branch review dispatched (opus) on review-eee2025..3611b99.diff
Final: review verdict ready to merge, no Critical/Important.
Final: minor (deferred): tile hit memo holds blobs (tiles.rs:199, :241-250); walkers caches hits, memo of misses would do — optional
Final: minor (deferred): store error does not stop later lookups (tiles.rs:219-236); foreign long write lock could freeze 2 s per new tile
Final: minor (deferred): Options label always says www.airscout.eu even with another mirror URL
Final: minor (deferred): maptile-gen writes through an existing *.part symlink
Final: minor (deferred): one extra progress line can queue around cancel (dropped before UI)
Final: Ruling: no per-store disk cap for the preloader (hostile mirror / plain-HTTP MITM could fill ~300 GB for the default area) kept 1:1 with C#, routed to stage 6 logic fixes — opt-in, off by default, per-tile cap 4 MiB holds — cost if wrong: a user's disk fills during an opt-in run
Final: full suite cargo test --workspace --locked → 1302 passed, 0 failed
