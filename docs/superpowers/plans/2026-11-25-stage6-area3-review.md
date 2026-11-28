# Stage 6 area 3 (feeds, database and time), review part — Implementation Plan

> **For agentic workers:** Execute this plan task-by-task without delegation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce the area-3 approval list in `docs/logic-fixes.md`. Every open defect in `as-feeds`, `as-db`, `as-stations`, `as-settings` and `as-signals` gets a row with a fix proposal, the user effect and the golden delta. The first-principles review adds any new defects. This part changes **no product code**. It ends when the owner approves the list in chat. The fix part is planned from the approved list.

**Architecture:** Documentation and analysis only. Four tasks: (1) inventory and file skeleton, then three review tasks, one per topic, each filling its rows. Proposals argue from first principles or a cited reference. Each fix names the independent source for its new expected values (spec, “Golden-test handling”).

**Tech Stack:** Markdown and read-only use of the Rust crates, `tests/golden/*.json` and the upstream checkout `target/upstream/AirScout` (at `fbadef0`). Optional throwaway probes live in the scratchpad.

**Spec:** `docs/superpowers/specs/2026-11-25-stage6-logic-fixes-design.md` (parent: `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`, stage 6).

## Global Constraints

- **Open row:** a row in `docs/logic-findings.md` whose status label is missing, **Ported 1:1**, **Ported 1:1 (…)** or **Unverified**. Rows labelled **Port deviation**, **Not reproduced…**, **New design**, **Not ported…**, **Resolved…**, **Moved…**, **Not applicable**, **Kept** or **Stage 4d** are not open. They are listed only if the review finds that the port still has the defect. Existing **Fixed in stage 6** rows stay in their completed area.
- **Area-3 rows:** open rows whose port code is in `crates/as-feeds`, `crates/as-db`, `crates/as-stations`, `crates/as-settings` or `crates/as-signals`. Use the port location, not the C# one. A row whose port code spans areas goes to the earliest area. Include the rows routed here from the earlier areas and verify their current location and status.
- **`docs/logic-fixes.md` table columns, exact:** `| # | Problem | Fix proposal | Effect for the user | Golden delta | Decision |`.
  - `#` is the findings number.
  - **Decision** is `fix`, `keep` (with the reason) or `owner`. In this part it holds the proposal, written `proposed: fix`, `proposed: keep — <reason>` or `owner — <question>`. Only the owner's approval turns it into the final value.
- **Golden delta** names the affected cases as `<file>.json:<case name>` and gives the size of the change, measured or bounded. Write `none` if no golden case is touched.
- **Expected-value source:** every `fix` proposal names where the new expected value will come from. Use a hand-worked case or a cited independent reference. For large composites use invariants plus spot values. It is never the fixed Rust code.
- **k-factor:** it stays the user setting `BandSettings.k_factor`. A change to its default or to the refraction model is an `owner` row. Give both options for the band-dependent defaults carried from area 1 and name the settings migration for a default change.
- **Persisted data:** name a migration for each proposal that changes on-disk data. Name any necessary algorithm-version bump for persisted computed results.
- **New findings:** use the next free number in `docs/logic-findings.md`, which is #371 at the start. Add the same row format with the label **Open (stage 6 area 3)** and a matching `logic-fixes.md` row.
- **C# cites** use upstream `fbadef0f`, re-checked in `target/upstream/AirScout`. Rust cites use `path:line`.
- **No product code changes.** No change to `tests/golden/` or `Cargo.lock`. Probe code lives only in the scratchpad.
- Privacy: examples and probes use only synthetic calls, positions and credentials. Credentials never appear in logs, Debug output or errors. Network fixtures use localhost.
- Commits signed (`SSH_AUTH_SOCK=~/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock`, `git commit -S`). Verify `%G?` is `G`. No push. Prefix every cargo command with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` and use `--locked` where the command accepts it.

## Review Focus

1. **Time zones, UTC and DST.** Trace timestamps from each format to UTC, through midnight and DST transitions. Check epoch units, precision and position age.
2. **Clock skew and ordering.** Check receipt-time fallbacks, reports at the same time or out of order, and the boundary with area 2's future-report admission. Check monotonic clocks for elapsed time.
3. **Database migrations and schema versions.** Review new, older and newer databases, migration failure and concurrent opens. Check station and signal timestamp keys, imports and conflict rules.
4. **Settings migration and defaults.** Compare fresh, partial and legacy files and feed defaults. Review the band-dependent k-factor carry from area 1.
5. **Feed parsing and credentials.** Follow null, garbage and huge inputs from HTTP and decompression to row parsing. Trace credentials through URL substitution, Debug, errors and worker events.

---

### Task 1: Inventory and `docs/logic-fixes.md` skeleton

**Files:**
- Modify: `docs/logic-fixes.md`
- Read: `docs/logic-findings.md`, `crates/as-feeds`, `crates/as-db`, `crates/as-stations`, `crates/as-settings`, `crates/as-signals`

**Interfaces:**
- Produces: `## Area 3: feeds, database and time` and its table, with one row per open finding in findings order.
  - Fill **Problem** from the finding. Assign the remaining columns to the review task that owns the row.
  - Task 2 owns feed adapters, the feed worker and feed time. Task 3 owns database, stations and signals. Task 4 owns settings, defaults and credential handling.
- Also produces: `## Not area 3`, with the target and one reason per ruled-out candidate.

- [ ] **Step 1:** Apply the open-row rule to all findings and review the earlier areas' routing notes and the area-1 k-factor carry.
- [ ] **Step 2:** Locate each candidate in Rust and assign it to the earliest area that owns its port code.
- [ ] **Step 3:** Prepare the table and ruled-out list. Follow the existing column definitions.
- [ ] **Step 4:** Verify that every identified area-3 number appears once and in findings order.
- [ ] **Step 5:** Commit the review plan as `docs: stage 6 area 3 review plan`. Carry the inventory into the review-list commit.

### Task 2: Feed adapters, worker and time

**Files:**
- Modify: `docs/logic-fixes.md`, `docs/logic-findings.md` (new rows only)
- Read: `crates/as-feeds/src/{feeds,pipeline.rs,vars.rs,worker.rs}`, `crates/as-feeds/tests`, `tests/golden/{feed_json,bincraft,plane_checker,adsb_decoder}.json`

**Interfaces:**
- Consumes: the Task 1 inventory.
- Produces: feed and worker rows, including the RTL1090 time policy, plausibility, per-format mapping, failover and malformed and oversized responses.

- [ ] **Step 1:** Trace the normal parse and admission paths for each feed. Check timestamp units, fractions, DST independence, ordering and clock skew against hand-worked UTC inputs.
- [ ] **Step 2:** Check record structure and numeric conversions with mixed valid and invalid rows. Check request and decompression boundaries.
- [ ] **Step 3:** Fill the rows and name the affected golden cases and independent expected values. Add newly verified defects.
- [ ] **Step 4:** Verify Task 2 rows are complete.

### Task 3: SQLite, stations and signals

**Files:**
- Modify: `docs/logic-fixes.md`, `docs/logic-findings.md` (new rows only)
- Read: `crates/as-db`, `crates/as-stations`, `crates/as-signals`, `tests/golden/callsign.json`

**Interfaces:**
- Consumes: the Task 1 inventory.
- Produces: the database, station and signal rows and explicit migrations for changes to persisted data.

- [ ] **Step 1:** Check schema-version gates, transactional migration, and legacy data. Trace import retry and duplicate-key rules through their callers.
- [ ] **Step 2:** Review callsign and locator normalization and the Spectrum Lab parsing and storage contracts with small synthetic examples.
- [ ] **Step 3:** Fill the rows and add new findings. Record golden deltas and the migration for any changed disk state.
- [ ] **Step 4:** Verify Task 3 rows are complete.

### Task 4: Settings, defaults and credentials

**Files:**
- Modify: `docs/logic-fixes.md`, `docs/logic-findings.md` (new rows only)
- Read: `crates/as-settings`, feed configuration and HTTP/error paths in `crates/as-feeds`, and the propagation cases that take an explicit effective radius.

**Interfaces:**
- Consumes: the Task 1 inventory and Tasks 2–3 proposals.
- Produces: the settings and credential rows, including the owner decision on k-factor defaults.

- [ ] **Step 1:** Compare fresh, partial and legacy settings. Trace secrets through configuration, formatting, request errors and diagnostics.
- [ ] **Step 2:** Give both k-factor options, the settings migration and invalidation implications for a change, and the golden delta. Fill the remaining rows and add new findings.
- [ ] **Step 3:** Verify zero unresolved placeholders, exact Decision values, unique matching finding numbers, and a source and golden delta for every proposed fix.
- [ ] **Step 4:** Commit `docs: stage 6 area 3 review list`.

## After Task 4

Run the final review of the whole review part. Show the owner the area-3 list with the `owner` questions first and **stop** for approval. The owner's answers are written into the Decision column and committed. The fix part of area 3 is planned from the approved table.

## Area 3 list review response

- #376 (Critical) — Applied #365's inclusive 10 s admission, with a separate choice for each possible loss limit and a timeline covering all four sinks and quiet/error cycles.
- #13 — Defined locator validation, trusted-position lookup and coarse equipment defaults for web, Win-Test and desktop.
- #50 — Made live admission and event order atomic across workers, retained source-time metadata guards and specified an interleaving test.
- #51/#55 — Separated wire categories and post-enrichment A388 handling from callsign mapping and its fixture assertions.
- #60/#378 — Separated registration and dispatch, reconciled empty and invalid envelopes, and isolated numeric decoding and mapping failures.
- #61/#62/#384 — Specified versioned record slices, validity, signed age and full BinCraft fixture deltas, including the retained zero-stride rules.
- #64 — Preserved encoded URL bytes, scoped stock endpoint detection and checked bounds on both authentication paths.
- #75 — Defined checked epoch-unit precedence, explicit stale-field handling, DST offsets and the missing/invalid distinction.
- #77 — Separated statistics and limits from the owner choice on ambiguities, and defined names, units and inference lifecycle.
- #81 — Specified removal and replacement of invalid baselines and finite validation in both write paths, after #379.
- #305 — Specified caller byte budgets, exclusive staging, conditional GET, sanitized failures and concurrent publication checks.
- #329 — Specified host-specific defaults and self-feed protection, plus a fresh/legacy and explicit/missing TOML matrix.
- #371/#373 — Made legacy omissions a separate owner choice with effective values, cache implications and one versioned save path.
- #340 — Canonicalized stored settings through both hosts' normal and editor save paths, preserving custom bytes.
- #372 — Specified shared typed open errors, the pre-pragma gate, the locked recheck and the lifetime of the guarantee.
- #374/#257/#277 — Contained credentials at diagnostic origins and recorded the public serialization residual for area 4.
- #375/#382 — Gave per-feed and archive ceilings, a Spectrum Lab limit and per-archive failure semantics.
- #377 — Bound validators to source and bytes, added scoped Location dependency and preserved atomic calculator timestamps.
- #383 — Defined invalid legacy read behavior and a one-time freshness bypass for valid replacement.
- #14 (Minor) — Separated normalization and single-slash deltas and specified the existing multiple-slash rule.
- #52/#54/#55 (Minor) — Attributed fixture assertions and deltas and scoped the airborne count to adapter outputs.
- #67/#79/#80/#341/#380 (Minor) — Corrected overwrite, bin parsing, host/disclaimer and epoch conversion citations.
- #305 and exclusions (Minor) — Used the closed-area reopening rule and marked earlier dispositions and #92 closed.
- #41/#69/#379 (Minor) — Linked the duplicate receipt policy, declared three bins in #79 and isolated overflow from last-bin truncation.
