# Stage 6: logic review and fixes — design

Parent spec: `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`, stage 6.
Approved by the owner on 2026-11-25.

## Intent

Stages 1–5 ported AirScout 1:1, including its suspected defects, so that the golden tests prove a faithful
port. Stage 6 now makes the port *right*: it fixes the defects that were reproduced on purpose, and it
reviews the propagation, elevation, intersection and aircraft-prediction maths from first principles to
find the ones nobody has logged yet. The goal is predictions a ham can trust: an aircraft scatter
candidate appears when the physics says it should, not when an inverted comparison or a wrong Fresnel
leg says so.

## Goals

- Every defect in `docs/logic-findings.md` that the port still reproduces gets a decision: fix, keep, or
  owner's call. Today that is ~78 core rows without a status label (#1–#82 and a few later ones) and the 68
  rows labelled **Ported 1:1**.
- A first-principles review of the maths named in the parent spec: WGS84 vs. sphere, refraction / k-factor,
  Fresnel clearance, antimeridian and polar cases, step-width aliasing, time handling, extrapolation of
  aircraft positions. New defects become new findings rows (next free number).
- Each fix: a failing test first, the fix, and the golden-test delta recorded in `docs/logic-fixes.md`.
- The owner sees each area's list before any fix of that area ships.

## Non-goals

- No compatibility mode. Fixes **replace** the 1:1 behaviour (owner, 2026-11-25, option A). There is one
  code path.
- No upstream reports during stage 6. Fixes go to DL2ALF later, one by one, as PRs from our fork of the C#
  repo (owner, 2026-11-25). Stage 6 only records what such a PR would need.
- No new features, no UI redesign. GUI rows are in scope only where they are logic defects.
- ViewClient, RainScout and the release plan stay after stage 6.

## Anti-goals

- A fix whose new behaviour is not argued from first principles or a cited reference. "Looks better" is not
  a fix.
- Regenerating the C# goldens to make tests pass. The C# goldens stay the record of upstream behaviour.
- A fix that silently changes a prediction without a row in `docs/logic-fixes.md`.

## Approaches considered

1. **Triage everything first, then fix.** One big list (~150+ rows) approved once. Full overview, but
   nothing usable for a long time and a list too long to review well.
2. **Area by area** (chosen by the owner). Each area runs: review → list → owner approval → fixes → golden
   delta → merge and push. Short lists, the most valuable fixes first. Risk: a later area can touch an
   earlier decision (e.g. the earth model also affects prediction). A later area that needs to revisit a
   settled decision or fix a remaining defect in a closed area reopens it explicitly in its own approval
   list, citing the prior disposition. Changes to approved behavior require owner approval.
3. **Prediction-affecting only.** Fastest, but leaves known defects standing on purpose.

## Areas, in order

1. **Propagation and elevation profile** — `as-geo`, `as-elevation`, `as-propagation`: earth model, k-factor,
   Fresnel, path stepping, horizon, antimeridian and poles.
2. **Intersection and aircraft prediction** — `as-aircraft`, `as-adsb`, crossing logic: intersection point,
   max-distance checks, track wraparound, extrapolation, time handling.
3. **Feeds, database, time** — `as-feeds`, `as-db`, `as-stations`, `as-settings`, `as-signals`.
4. **GUI and integrations** — `airscout`, `as-wintest`, `as-cat`, `as-web`, `as-dde`, the suite tools.

Each finding row is assigned to exactly one area by its code location; a row that spans areas goes to the
earliest one.

## Per area

1. **Review.** Collect the area's open rows. Run the first-principles review for the area and add new rows.
   Each row gets a proposal.
2. **Approval list.** A section per area in `docs/logic-fixes.md`, one row per finding:

   | # | Problem | Fix proposal | Effect for the user | Golden delta | Decision |
   |---|---------|--------------|---------------------|--------------|----------|

   Decision is `fix`, `keep` (with the reason) or `owner` until the owner decides. The owner approves the
   list in chat; nothing in the area is fixed before that.
3. **Fixes.** Per approved row: failing test, fix, golden delta. The findings row gets the label
   **Fixed in stage 6** with a pointer to its `logic-fixes.md` row; `keep` rows get **Kept**.
4. **Record for upstream.** Per fixed row, `logic-fixes.md` names the C# location at upstream `fbadef0f`, the
   fix in C# terms, and the test that proves it — enough to write the upstream PR later.
5. **Gates and merge.** Full gates (macOS, hamlib pair, Linux docker, Windows mingw) and the final review;
   then merge to main and push per area without a further stop (owner, 2026-11-25).

## Planning rhythm (builder check, Claude's call)

- One plan per area, written when the area starts. Each area plan has two parts: the review part ends at the
  owner's approval of the list; the fix part is written from the approved list. No finding is pre-approved,
  not even obvious ones such as #1–#3.
- The earth-model question (#7, sphere R = 6371 km vs. WGS84) is a single row at the top of the area-1
  list. If approved, it is fixed first, so every later delta in the area is measured against it.
- The k-factor stays a user setting (`BandSettings.k_factor`, default 1.33). A change to its default or to
  the refraction model is an `owner` row, with a settings migration if the default changes.

## Golden-test handling

- The C# golden JSON files in `tests/golden/` and `upstream_commit` stay unchanged.
- A golden case whose value a fix changes keeps its C# value in the JSON. The Rust test for that case
  asserts the new expected value instead, named with the finding number, and `logic-fixes.md` lists the
  case, the C# value, the new value and why.
- The new expected value comes from an independent computation (a reference formula, a published table, or
  a hand-worked case in the test), never from running the fixed Rust code and copying its output.
  Allowed sources, in order of preference: a hand-worked small case; an independent reference
  implementation inside the test or as a dev-dependency (a different formulation, e.g. the `geographiclib`
  geodesics), cited; for large composites (full elevation paths, 360° horizons, multi-plane results),
  invariants and bounds plus a few spot values from the reference (Claude's call).
- Every golden case not touched by an approved fix still matches C#. That is how a fix proves it changed
  only what it meant to.

## Persisted data (builder check, Claude's call)

- Caches that hold computed results (horizons, elevation paths, crossing data) carry an algorithm version.
  A fix that changes such results bumps it, and old entries are recomputed, not served. This keeps the
  anti-goal "no silent change of a prediction" true for cached data.
- A fix may change on-disk data (settings, DB rows) only with a migration, and its list row says so.

## Constraints

- Everything from the parent spec and the earlier stages still applies: cargo env vars, disk limits
  (`target/` ≤ 60 GB, clean at 40), signed commits, push only on owner request, privacy (synthetic call
  signs and positions in tests), credentials never logged, linear history.
- Licence GPL-3.0-only.
- Upstream reference checkout `target/upstream/AirScout` at `fbadef0`; the goldens were made at `91b02cb`.
  Cites in `logic-fixes.md` use `fbadef0f`.

## Left to the builder

- Which reference implementations to check against (e.g. Karney geodesics for WGS84, ITU-R
  recommendations for refraction and Fresnel), as long as each is cited.
- How to split an area into plan tasks.
- Wording of each row's user effect.
