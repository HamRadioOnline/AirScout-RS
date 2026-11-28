# Stage 6 area 4 (GUI and integrations), review part — Implementation Plan

> **For agentic workers:** Execute this plan task-by-task without delegation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce the area-4 approval list in `docs/logic-fixes.md`. Every open defect in the
`airscout` GUI and the integration crates (`as-wintest`, `as-cat`, `as-web`, `as-dde`) and the
suite tools (`as-cli`, `catcheck`, `elevation-tile-gen`, `maptile-gen`, `airscout-planeserver`,
`as-maps`) gets a row with a fix proposal, the user effect and the golden delta. The
first-principles review adds any new defects. This part changes **no product code**. It ends when
the owner approves the list in chat. The fix part is planned from the approved list.

**Architecture:** Documentation and analysis only. Four tasks: (1) inventory and file skeleton,
then three review tasks, one per topic, each filling its rows. Proposals argue from first
principles or a cited reference. Each fix names the independent source for its new expected
values (spec, “Golden-test handling”).

**Tech Stack:** Markdown and read-only use of the Rust crates, `tests/golden/*.json` and the
upstream checkout `target/upstream/AirScout` (at `fbadef0f`). Optional throwaway probes live in
the scratchpad.

**Spec:** `docs/superpowers/specs/2026-11-25-stage6-logic-fixes-design.md` (parent:
`docs/superpowers/specs/2026-11-23-airscout-rs-design.md`, stage 6).

## Global Constraints

- **Open row:** a row in `docs/logic-findings.md` whose status label is missing, **Ported 1:1**,
  **Ported 1:1 (…)** or **Unverified**. Rows labelled **Port deviation**, **Not reproduced…**,
  **New design**, **Not ported…**, **Resolved…**, **Moved…**, **Not applicable**, **Kept** or
  **Stage 4d** are not open. They are listed only if the review finds that the port still has the
  defect. Existing **Fixed in stage 6** rows stay in their completed area.
- **Area-4 rows:** open rows whose port code is in `crates/airscout`, `crates/as-wintest`,
  `crates/as-cat`, `crates/as-web`, `crates/as-dde`, `crates/as-cli`, `crates/catcheck`,
  `crates/elevation-tile-gen`, `crates/maptile-gen`, `crates/airscout-planeserver` or
  `crates/as-maps`. Use the port location, not the C# one. Include the rows routed here from the
  earlier areas and verify their current location and status. GUI rows are in scope **only** as
  logic defects: no new features and no UI redesign.
- **`docs/logic-fixes.md` table columns, exact:** `| # | Problem | Fix proposal | Effect for the user | Golden delta | Decision |`.
  - `#` is the findings number.
  - **Decision** is `fix`, `keep` (with the reason) or `owner`. In this part it holds the
    proposal, written `proposed: fix`, `proposed: keep — <reason>` or `owner — <question>`. Only
    the owner's approval turns it into the final value.
- **Golden delta** names the affected cases as `<file>.json:<case name>` and gives the size of the
  change, measured or bounded. Write `none` if no golden case is touched.
- **Expected-value source:** every `fix` proposal names where the new expected value will come
  from. Use a hand-worked case or a cited independent reference. It is never the fixed Rust code.
- **No bundling:** one row per finding; independent decisions are never merged into a single row.
- **Persisted data:** name a migration for each proposal that changes on-disk data.
- **New findings:** use the next free number in `docs/logic-findings.md`, which is #385 at the
  start. Add the same row format with the label **Open (stage 6 area 4)** and a matching
  `logic-fixes.md` row.
- **C# cites** use upstream `fbadef0f`, re-checked in `target/upstream/AirScout`. Rust cites use
  `path:line`.
- **No product code changes.** No change to `tests/golden/` or `Cargo.lock`. Probe code lives only
  in the scratchpad.
- Privacy: examples and probes use only synthetic calls, positions and credentials. Credentials
  never appear in logs, Debug output or errors. Network fixtures use localhost.
- Commits signed (`SSH_AUTH_SOCK=~/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock`,
  `git commit -S`). Verify `%G?` is `G`. No push. Prefix every cargo command with
  `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` and use `--locked`
  where the command accepts it.

## Review Focus

1. **Integration protocols.** Follow Win-Test UDP, DDE, CAT/hamlib/rigctld and the GS-232A serial
   rotator with malformed, huge and hostile input. Check that a wrong checksum, an oversized reply,
   a non-finite numeric and a slow or dead peer are handled without panic, unbounded memory or an
   endless retry.
2. **Time and units at integration boundaries.** Check azimuth/elevation formatting and rounding,
   the track-output step cadence, and the rigctld/hamlib frequency width across the 32-bit boundary.
3. **Credentials.** Trace feed and endpoint URLs, tokens and keys through the public web JSON
   (`/settings.json`, `/planefeedsettings.json`), Debug, errors and the on-disk config files. A
   credential never appears in logs, errors or public JSON.
4. **Local-only binding and tile policy.** The web and plane-server listeners bind only where the
   original did; no tile route proxies a provider or bulk-downloads from a forbidden host (OSM
   policy).
5. **GUI–background concurrency.** Check the tracking worker, the Win-Test receiver, the CAT
   worker, the web JSON writer and the map preloader against the UI thread, Options restarts and
   HISTORY mode for data races and lost or superseded updates.

---

### Task 1: Inventory and `docs/logic-fixes.md` skeleton

**Files:**
- Modify: `docs/logic-fixes.md`
- Read: `docs/logic-findings.md`, `crates/airscout/src`, `crates/as-wintest`, `crates/as-cat`,
  `crates/as-web`, `crates/as-dde`, `crates/as-cli`, `crates/catcheck`,
  `crates/elevation-tile-gen`, `crates/maptile-gen`, `crates/airscout-planeserver`, `crates/as-maps`

**Interfaces:**
- Produces: `## Area 4: GUI and integrations` and its table, with one row per open finding in
  findings order. Fill **Problem** from the finding; assign the remaining columns to the review
  task that owns the row. Task 2 owns Win-Test, DDE, CAT and the serial rotator. Task 3 owns the
  web HTTP and map/preloader rows. Task 4 owns the GUI service concurrency and the suite tools.
- Also produces: `## Not area 4`, with the target and one reason per ruled-out candidate.

- [ ] **Step 1:** Apply the open-row rule to all findings and review the earlier areas' routing
  notes (the `## Not area 1/2/3` sections of `docs/logic-fixes.md`).
- [ ] **Step 2:** Locate each candidate in Rust and assign it to the earliest area that owns its
  port code.
- [ ] **Step 3:** Prepare the table and ruled-out list. Follow the existing column definitions.
- [ ] **Step 4:** Verify that every identified area-4 number appears once and in findings order.
- [ ] **Step 5:** Commit the review plan as `docs: stage 6 area 4 review plan`. Carry the inventory
  into the review-list commit.

### Task 2: Win-Test, DDE, CAT and serial rotator

**Files:**
- Modify: `docs/logic-fixes.md`, `docs/logic-findings.md` (new rows only)
- Read: `crates/as-wintest/src/{lib,socket,dotnet,track}.rs`, `crates/as-dde/src/{lib,ddeml}.rs`,
  `crates/as-cat/src/{lib,config,backend,rigctld,rigctld_proto,worker,rig,hamlib,omnirig}.rs`,
  `crates/airscout/src/{track_outputs,rotator_gs232,track_service,wintest_service,wintest_nearest,cat_service}.rs`

**Interfaces:**
- Consumes: the Task 1 inventory.
- Produces: the Win-Test, DDE, CAT/rigctld/hamlib and serial-rotator rows, including the checksum,
  band selection, error counters, formatting and the frequency-width rows.

- [ ] **Step 1:** Trace a datagram, a DDE poke, a CAT poll and a rotator query from wire input to
  output, including malformed, oversized and non-finite replies.
- [ ] **Step 2:** Check the error counters and switch-off per output group, and the azimuth and
  elevation rounding at the wire boundary.
- [ ] **Step 3:** Fill the rows and name the affected golden cases and independent expected values.
  Add newly verified defects.
- [ ] **Step 4:** Verify Task 2 rows are complete.

### Task 3: Web HTTP and map/preloader integration

**Files:**
- Modify: `docs/logic-fixes.md`, `docs/logic-findings.md` (new rows only)
- Read: `crates/as-web/src/{server,route,query,json,dto,planes,assets}.rs`,
  `crates/airscout/src/{web_api,web_settings_json,web_stations,web_paths,web_service,map_preloader,tiles}.rs`,
  `crates/as-maps/src/lib.rs`

**Interfaces:**
- Consumes: the Task 1 inventory.
- Produces: the web HTTP, best-case, public-serialization and map/preloader rows, including the
  credential policy for the public JSON and the tile-policy and binding decisions.

- [ ] **Step 1:** Trace each public route from query to response, including malformed parameters,
  missing tiles and the credential-bearing URL fields.
- [ ] **Step 2:** Check the listener bind, the request caps, the redirect policy and the mirror
  provider-host refusal.
- [ ] **Step 3:** Fill the rows and add new findings. Record golden deltas and the public-URL policy.
- [ ] **Step 4:** Verify Task 3 rows are complete.

### Task 4: GUI services and suite tools

**Files:**
- Modify: `docs/logic-fixes.md`, `docs/logic-findings.md` (new rows only)
- Read: `crates/airscout/src/{services,background,cleanup,history_download,crossing_history,exports}.rs`,
  `crates/airscout/src/ui`, `crates/as-cli/src`, `crates/catcheck/src`,
  `crates/elevation-tile-gen/src`, `crates/maptile-gen/src`, `crates/airscout-planeserver/src`

**Interfaces:**
- Consumes: the Task 1 inventory and Tasks 2–3 proposals.
- Produces: the GUI service concurrency, tracking-worker, history/cleanup UI and suite-tool rows,
  including the SRTM tile-generation decisions and the `azel.dat` write.

- [ ] **Step 1:** Check the tracking worker and the background services against Options restarts,
  HISTORY mode and the UI thread.
- [ ] **Step 2:** Review the history, cleanup and export rows and the elevation-tile and map-tile
  tools for on-disk consistency.
- [ ] **Step 3:** Fill the rows and add new findings. Record golden deltas and migrations for any
  changed on-disk data.
- [ ] **Step 4:** Verify zero unresolved placeholders, exact Decision values, unique matching
  finding numbers, and a source and golden delta for every proposed fix.
- [ ] **Step 5:** Commit `docs: stage 6 area 4 review list`.

## After Task 4

Run the final review of the whole review part. Show the owner the area-4 list with the `owner`
questions first and **stop** for approval. The owner's answers are written into the Decision column
and committed. The fix part of area 4 is planned from the approved table.

---

## Area 4 list review response

One line per independent-review finding (5 Critical, 12 Important, 7 Minor); each states how it was addressed in `docs/logic-fixes.md`.

- **C1** (#257/#277 bundling + owner + contradictory mechanism): split into two rows, both `owner` with options A/B/C; mechanism no longer claims to strip URL paths.
- **C2** (#246 false keep): changed to `proposed: fix` — replace `call_missing("IsWritable", …)` with `w()` (`is_writeable`).
- **C3** (#276 stale after #15): re-checked; `destination_point` wraps longitude, so the row moved to `## Not area 4` (wrap by #15, error text by #258).
- **C4** (#236 ambiguous clamp/wrap): now round then wrap azimuth mod 360 and clamp elevation 0..=90 with exact expected values (359.6→000, 359.4→359, −0.1→000, −5→000, 90.4→090).
- **C5** (wrong/no Rust cites, zero C# cites): every row now cites Rust `path:line` and upstream `fbadef0f` C# `file:line`.
- **I1** (#105 callers): all five `local_obstruction` callers cited; bearing from the unshifted settings position specified.
- **I2** (#94 void-tile): `#94` is now `fix` on the `calculator_shift` `ELV_MISSING` guard; the UI-vs-calculator model-order note kept as text.
- **I3** (#223 missing enum arms): changed to `fix` — check suitability on all four strategies A–D.
- **I4** (#221 split destruction): changed to `fix` — restore split when `SplitOn && tx_hz > 0`, else simplex.
- **I5** (#200 bundling/wrong desc): band mix is `fix`; the DB-write half moved to `#265`; "path-show" wording corrected to ASSETPATH.
- **I6** (#265 routes/owner): cited `web_paths.rs:219,249,278,283,364`, added `/nearestplanes.json`, split station-writes vs path-cache writes as one `owner` question.
- **I7** (#239/#203 retry/switch-off): both converted to `owner`; #203 Problem cell fixed (DDE/UDP/Serial count), #239 Effect no longer claims "as UDP and DDE do".
- **I8** (#194 bit-7): converted to `owner`; option A requires comparing the raw `last` byte without the bit-7 mask.
- **I9** (#202 vs #236): `#202` converted to `owner` (UDP wire), `#236` is `fix` (serial rotator) — the two wires decoupled.
- **I10** (#141/#142): both changed to `fix` with concrete expected values.
- **I11** (#293 delta): exact golden delta `elevation_tilegen.json:srtm_tile[25]` (JO50NA) `written:true`→`written:false`; all-void tiles skipped (`Outcome::Void`).
- **I12** (completeness #103/#155/#303/#104): added all four rows; `#103`/`#104`/`#155` `fix`/`owner`, `#303` `keep`.
- **M1** (#294 magnitude/delta): added ~92.6 m/~30.9 m shift, exact 862-case golden delta, `rows-1` vs `rows-2`, unverified-corpus caveat.
- **M2** (#385 clear_table): added `StationDb::clear_table` refusal and named `Location`/`QRV`/`Table_last_updated`.
- **M3** (#262/#263 underspecified): added empty/invalid `band` handling (#262) and `w.best_case=false`/source-unchanged (#263).
- **M4** (#206 atomic write): added unique same-directory temp file + Windows rename caveat.
- **M5** (#264/#249/#180/#184/#311): `#264` and `#249` converted to `owner`/`fix`; `#180`/`#184`/`#311` keep rows now state the real behavior instead of bare "display only".
- **M6** (Not-area-4 ranges): removed #103/#104/#155/#303 (re-opened) and #109/#179/#186/#282/#289/#305 (settled in areas 1–3); added the omitted #125.
- **M7** (generic expected values): added concrete input→output examples to the `fix` rows (#105/#200/#206/#237/#262/#263/#265/#385).

---

## Area 4 list re-review response

One line per scoped re-review finding (2 Critical, 4 Important, 3 Minor); each states how it was addressed in `docs/logic-fixes.md`.

- **R-C1** (#104 backwards `proposed: fix`): changed to `proposed: keep` — the port computes both the horizon and the local-obstruction lookup from the draft My position (`s.my_lat`/`s.my_lon`), so they always match and fix C#'s `OptionsDlg.cs:932` mismatch instead of reintroducing it; the "matches what the settings persist" claim is removed.
- **R-C2** (#265 multi-decision bundling + #200 DB-write half): split into two owner rows. #265 is now the station-insert half only (web `web_paths.rs:278,283` + ASSETPATH `wintest_nearest.rs:106-109`, both through `StationDb::location_find_or_create`; `QRVFindOrCreateDefault` never stores), with option A spelling out the in-memory `FromLoc` centre fallback and noting `StationDb::location_find` returns `None` with no fallback. New finding #386 (`docs/logic-findings.md`) carries the path-cache half (web `web_paths.rs:219,249,364` + ASSETPATH `wintest_nearest.rs:169`, `save = true`), contrasting `save = false` recomputation cost against cache growth. #200 stays `proposed: fix` on the band-mix only.
- **R-I1** (19 rows missing Rust `:line`, bad C#/Rust cites): added exact `path:line` to all 19 rows; fixed `#162` to `CleanupDlg.cs:121-126` (file is 128 lines), `#94` to `PathCalculator.cs:179-191`, `#277` now cites only `Webserver.cs:474` (the VRS-only `:1080`/`VRSWebServer.cs:41,60` removed), and `#276` now cites `latlon.rs:153-162` (`destination_point`, not `mid_point`).
- **R-I2** (#257/#277 overlapping options + web-UI breakage): both now ask two clean non-overlapping options — A omit/blank the field, B strip userinfo + non-placeholder query (with path-token warning) — and both Effect cells state the shipped web-UI impact (`airscout.feeds.js:101,254` for VRS, `airscout.wtkst.js:56` for wtKST).
- **R-I3** (#155 incomplete owner options): Fix proposal and Effect now present A (stop background workers while Options is open) and B (keep them running, generation-aware deletes/clears) symmetrically, citing `app/dialogs.rs:43-55`, `app/maintenance.rs:52-109`, `maintenance.rs:240-247`.
- **R-I4** (#194 broken table + fairness): escaped the pipe as `last \| 0x80` and noted the third-party/homebrew sender compatibility risk under option A.
- **R-M1** (#293 missing label): added an explicit `Expected-value source:` sentence (all-`-32768` SRTM tile after a non-void tile yields `Outcome::Void` / `written: false`).
- **R-M2** (`## Not area 4` range overlap): changed `#134–#140` to `#134–#135, #137–#140` so the Area-2-fixed `#136` is no longer swept in.
- **R-M3** (bare C# filenames + intro wording): prefixed `#105` (`AirScout/PathCalculator.cs`, `AirScout/OptionsDlg.cs`) and `#142` (`AirScout.AircraftPositions/AircraftPositionDatabase.cs`) with their project directories; the Area 4 intro now refers to `#257`/`#277` as *"the two defects"*.
