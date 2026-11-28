# Stage 6 area 1 (propagation and elevation profile), review part — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce the area-1 approval list in `docs/logic-fixes.md`. Every open defect in `as-geo`, `as-elevation` and `as-propagation` gets a row with a fix proposal, the user effect and the golden delta. The first-principles review adds any new defects. This part changes **no product code**. It ends when the owner approves the list in chat; the fix part is planned from the approved list.

**Architecture:** Documentation and analysis only. Four tasks: (1) inventory and file skeleton, then three review tasks, one per topic, each filling its rows. Proposals argue from first principles or a cited reference. Each one names the independent source for its new expected values (spec, "Golden-test handling").

**Tech Stack:** Markdown; read-only use of the Rust crates, `tests/golden/*.json`, and the upstream checkout `target/upstream/AirScout` (at `fbadef0`). Optional: throwaway probe code in the scratchpad (never committed) to measure a defect's size.

**Spec:** `docs/superpowers/specs/2026-11-25-stage6-logic-fixes-design.md` (parent: `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`, stage 6).

## Global Constraints

- **Open row:** a row in `docs/logic-findings.md` whose status label is missing, **Ported 1:1**, **Ported 1:1 (…)** or **Unverified**. Rows labelled **Port deviation**, **Not reproduced…**, **New design**, **Not ported…**, **Resolved…**, **Moved…**, **Not applicable**, **Kept** or **Stage 4d** are not open. They are listed only if the review finds that the port still has the defect.
- **Area-1 rows:** open rows whose port code is in `crates/as-geo`, `crates/as-elevation` or `crates/as-propagation`. Use the port location, not the C# one. A row whose port code spans areas goes to the earliest area. A row whose port code is in a later area's crate is not area 1, even when its C# file is in `ScoutBase.Core`. For example, `Callsign.Cut` (#14) belongs to the crate that holds `Callsign`.
- **`docs/logic-fixes.md` table columns, exact:** `| # | Problem | Fix proposal | Effect for the user | Golden delta | Decision |`.
  - `#` is the findings number.
  - **Decision** is `fix`, `keep` (with the reason) or `owner`. In this part it holds the proposal, written `proposed: fix`, `proposed: keep — <reason>` or `owner — <question>`. Only the owner's approval turns it into the final value.
- **Golden delta** names the affected cases as `<file>.json:<case name>` and gives the size of the change, measured or bounded. Write `none` if no golden case is touched.
- **Expected-value source:** every `fix` proposal names where the new expected value will come from. The source is either a hand-worked case, or a cited independent reference such as Karney/`geographiclib` or an ITU-R recommendation with its number and section. For large composites it is invariants plus spot values. It is never the fixed Rust code.
- **Earth model (#7):** one row, the first row of the area-1 table, Decision `owner`. Give both options (sphere R = 6371 km, WGS84) with their golden delta.
- **k-factor:** it stays the user setting `BandSettings.k_factor` (default 1.33, `crates/as-settings/src/band_settings.rs:28`). A change to its default, or to the refraction model, is an `owner` row.
- **New findings:** use the next free number in `docs/logic-findings.md`, which is #343 at the start. Add the same row format with the label **Open (stage 6 area 1)** and a matching `logic-fixes.md` row.
- **C# cites** use upstream `fbadef0f`, re-checked in `target/upstream/AirScout`. Rust cites use `path:line`.
- **No product code changes.** No change to `tests/golden/`. Probe code lives only in the scratchpad.
- Privacy: synthetic positions in examples (for example JO50AA ↔ JO60AA), no real call signs.
- Commits signed (`SSH_AUTH_SOCK=~/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock`, `git commit -S`). No push. Prefix every cargo command with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

## Review Focus

1. **Antimeridian paths.** For example JM-square to a station across 180° E/W: distance, bearing, step points and tile lookups must not wrap the long way round. Task 2 checks each `as-geo` function with a probe case across 180°.
2. **Near-polar and very short paths.** Both endpoints north of 80°, and a path shorter than one step width. Midpoints, bearings and step counts must not produce NaN or an empty path. Tasks 2 and 3 cover this.
3. **Paths longer than the profile step budget.** Long EME-free tropo paths over 1000 km: step-width aliasing must not skip a ridge that blocks the path. Task 3 sizes the worst case with a probe.
4. **Tile and obstruction edges.** A point exactly on a tile border, or a local obstruction at azimuth 0/360. Each must give the same elevation or obstruction from either side. Task 4 checks this.
5. **Fresnel and clearance at both ends.** The F1 radius at distance 0 and at full path length must be 0, and positive in between. Task 3 checks this against the #1 proposal.

---

### Task 1: Inventory and `docs/logic-fixes.md` skeleton

**Files:**
- Create: `docs/logic-fixes.md`
- Read: `docs/logic-findings.md`, `crates/as-geo`, `crates/as-elevation`, `crates/as-propagation`

**Interfaces:**
- Produces: `docs/logic-fixes.md` with the intro, the section `## Area 1: propagation and elevation profile`, and the table.
  - The table holds one row per area-1 open row, in findings order. The exception is #7, which comes first.
  - Each row has **Problem** filled in (one sentence, from the findings row) and the other columns set to `TBD (task N)`, where N is the review task that owns the row.
  - Task ownership: Task 2 has geodesy and earth model. Task 3 has refraction, Fresnel, path, horizon and intersection-in-path. Task 4 has elevation tiles and obstructions.
- Also produces: a section `## Not area 1` listing each candidate row that was ruled out of area 1, with its target area and one reason.

- [ ] **Step 1:** List the candidate open rows. Apply the open-row rule from the Global Constraints to every row of `docs/logic-findings.md`. The known area-1 candidates at plan time are #1, #2, #3, #4, #5, #7, #8, #10, #11, #15, #16, #172, #179, #263, #282, #289, #293 and #294. Confirm each one and look for any others.
- [ ] **Step 2:** For each candidate, find the Rust code that ports it (`path:line`) and assign the area. Record the ruled-out rows in `## Not area 1`.
- [ ] **Step 3:** Write `docs/logic-fixes.md`.
  - Intro: what the file is, the column meanings, and the Decision values, copied from the Global Constraints.
  - The area-1 table, as described under Interfaces.
- [ ] **Step 4:** Verify. Every area-1 row number appears exactly once in the table: `grep -c '^| [0-9]' docs/logic-fixes.md` equals your area-1 count. `#7` is the first table row.
- [ ] **Step 5:** Commit `docs: stage 6 area 1 inventory`.

### Task 2: Geodesy and earth model

**Files:**
- Modify: `docs/logic-fixes.md` (the rows owned by Task 2), `docs/logic-findings.md` (new rows only)
- Read: `crates/as-geo/src/latlon.rs`, `crates/as-geo/src/maidenhead.rs`, `tests/golden/latlon.json`, `tests/golden/maidenhead.json`

**Interfaces:**
- Consumes: the Task 1 table.
- Produces: the filled rows for #7, #15, #16 and any new geodesy findings.
  - The #7 row gives the measured difference between the sphere and WGS84 (Karney) for distance and bearing on three synthetic paths: 50 km, 500 km and 2000 km.
  - Later tasks rely on the #7 row to state the delta baseline: their golden deltas are measured under the current sphere model.

- [ ] **Step 1:** Review every public function in `latlon.rs` and `maidenhead.rs` from first principles: distance, bearing, destination point, midpoint, intermediate points, locator conversion. Cover the antimeridian, the poles, coincident points and antipodal points (Review Focus 1 and 2).
  - Use a scratchpad probe that compares the port against `geographiclib`, or against hand-worked values. Do not add a dependency to the workspace.
- [ ] **Step 2:** Fill the Task 2 rows. Add a new finding (findings row plus fixes row) for each defect the review finds.
- [ ] **Step 3:** Verify. No `TBD (task 2)` is left: `grep -c 'TBD (task 2)' docs/logic-fixes.md` → `0`. Each `fix` row names its expected-value source.
- [ ] **Step 4:** Commit `docs: stage 6 area 1 geodesy review`.

### Task 3: Refraction, Fresnel, path stepping, horizon

**Files:**
- Modify: `docs/logic-fixes.md` (the rows owned by Task 3), `docs/logic-findings.md` (new rows only)
- Read: `crates/as-geo/src/propagation.rs`, `crates/as-propagation/src/*.rs`, `crates/as-elevation/src/path.rs`, `crates/as-elevation/src/horizon.rs`, `tests/golden/propagation_*.json`, `tests/golden/elevation_path.json`

**Interfaces:**
- Consumes: the Task 1 table, and the #7 baseline statement from Task 2.
- Produces: the filled rows for #1, #2, #3, #11, #179, #282 and any new findings on refraction, Fresnel, stepping or horizon.

- [ ] **Step 1:** Review the maths from first principles.
  - Effective earth radius with k, and the earth-bulge term.
  - The F1 radius `sqrt(λ·d1·d2/(d1+d2))` and the clearance test.
  - Path step width against the elevation tile resolution (aliasing, Review Focus 3).
  - Horizon computation per azimuth, and the doppler formula (#179).
  - The endpoint cases (Review Focus 5).
  - Cite the reference for each formula, for example ITU-R P.526 for Fresnel and diffraction or ITU-R P.453 for refractivity.
- [ ] **Step 2:** For #1 and #2, name the exact golden cases that change. Get their new expected values (hand-worked) and the C# values from the JSON.
- [ ] **Step 3:** Fill the Task 3 rows. Add new findings.
- [ ] **Step 4:** Verify. `grep -c 'TBD (task 3)' docs/logic-fixes.md` → `0`.
- [ ] **Step 5:** Commit `docs: stage 6 area 1 propagation review`.

### Task 4: Elevation tiles and local obstructions

**Files:**
- Modify: `docs/logic-fixes.md` (the rows owned by Task 4), `docs/logic-findings.md` (new rows only)
- Read: `crates/as-elevation/src/{tile.rs,store.rs,model.rs,obstruction.rs,minmax.rs,cache.rs,catalogue.rs}`, `tests/golden/elevation_tile*.json`, `tests/golden/elevation_tilegen.json`

**Interfaces:**
- Consumes: the Task 1 table.
- Produces: the filled rows for #4, #5, #8, #10, #172, #263, #289, #293, #294 and any new tile or obstruction findings. It also states, for the fix part, which caches need the algorithm-version bump (spec, "Persisted data").

- [ ] **Step 1:** Review the tile lookup, the interpolation and the edge handling (Review Focus 4). Also review the obstruction azimuth wrap, min/max, and the cache keys.
  - For each fix, say whether cached data in `cache.rs` or in the horizon store would be served stale, and therefore whether an algorithm-version bump is needed.
- [ ] **Step 2:** Fill the Task 4 rows. Add new findings.
- [ ] **Step 3:** Verify.
  - `grep -c 'TBD' docs/logic-fixes.md` → `0` for the whole area-1 table.
  - Every Decision cell starts with `proposed: fix`, `proposed: keep` or `owner`.
- [ ] **Step 4:** Commit `docs: stage 6 area 1 elevation review`.

## After Task 4

Run the final review of the whole review part. Then show the owner the area-1 table, with the `owner` rows first, and **stop** for approval. The owner's answers are written into the Decision column and committed. The fix part of area 1 is planned from the approved table.
