# Upstream sync to fbadef0 (V1.4.5.1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the port from upstream base `91b02cb7` to upstream HEAD `fbadef0f` (V1.4.5.1, 2026-08-29), as the owner decided for after stage 5. Memory: "Upstream sync after spec".

**Architecture:** The code delta of `git diff 91b02cb fbadef0` is small. It lives in three feed plugins and one web server line. Everything else is assembly versions, packages, csproj and the LICENSE file. Three changes are ported 1:1, one is refused, and one does not apply.

**Tech Stack:** Rust 1.95+, as-feeds, mockito, toml.

**Spec:** `docs/superpowers/specs/2026-11-23-airscout-rs-design.md`, the port policy: 1:1 unless logged.

**Owner decisions (2026-11-25):**
- U1–U3 are ported 1:1.
- U4 is not ported and gets a finding.
- U5 needs no port and **no finding**.
- U6 updates the upstream base in the docs.

**Upstream delta (verified, `git diff 91b02cb fbadef0 -- '*.cs'`):**

| # | C# change at fbadef0 | Port |
|---|---|---|
| U1 | `AirScout.PlaneFeeds.Plugin.ADSBOne/ADSBOne.cs:41`. The default URL `https://api.adsb.one/v2/point/%MYLAT%/%MYLON%/%MAXDISTKM%` becomes `…/%MYLAT%/%MYLON%/250`. | 1:1. Change only the default; C# migrates no saved URL. |
| U2 | `AirScout.PlaneFeeds.Plugin.OpenSky/OpenSky.cs`. The default URL `https://api.opensky-network.org/api/states/all` becomes `https://opensky-network.org/api/states/all`. On every load the saved URL is rewritten with `Replace("https://api.opensky-network.org/api/", "https://opensky-network.org/api/")` and saved ("correct URL with V1.4.7.0 automatically"). | 1:1. Change the default and apply the same replace in `load_settings`. |
| U3 | `AirScout.PlaneFeeds.Plugin.PublicBinCraft/PublicBinCraft.cs`. The default `Host` `https://globe.airplanes.live` becomes `http://localhost`. `Disclaimer` changes from `""` to the upstream text (six lines, ends "The auhor of this software will not be responsible in any case."). | 1:1. The default host becomes `http://localhost`. The disclaimer becomes the upstream text verbatim, typo included, replacing our own text. |
| U4 | `OpenSky.cs`, in `GetPlanes`. The new line `Console.WriteLine("Received access token: " + token);` writes the OAuth bearer token to the console. | **Not ported**: it would log a credential. New finding, labelled **Not ported**. To be reported upstream in stage 6. |
| U5 | `AirScout/Webserver.cs:163`. The `netsh http add urlacl` call quotes the user name. | Nothing to port: the port has no `netsh` (#259). No finding (owner). |
| U6 | `AirScout/VersionHistory.txt`, assembly versions 1.4.5.1. | Docs: upstream base fbadef0 / V1.4.5.1. |

## Global Constraints

- Everything in the 5d Global Constraints still applies to the touched crates. That covers cargo env vars, disk limits, signed commits, no push, privacy, credentials and gates (`gates.md` of 5d: macOS, the hamlib pair, Linux docker, Windows mingw).
- `target/upstream/AirScout` is a read-only reference checkout. Task 1 moves it to `fbadef0` (`git -C target/upstream/AirScout checkout -q fbadef0`) and nothing else.
  - Every C# cite in `docs/logic-findings.md` and in code comments that points into `OpenSky.cs`, `ADSBOne.cs` or `PublicBinCraft.cs` is re-checked at `fbadef0` and corrected. No other file changed lines.
  - The findings header names `fbadef0f`. Its new sentence says the base was `91b02cb7` up to and including #338, and that the cites were re-checked at `fbadef0f`.
- The golden files keep `upstream_commit` `91b02cb…`. None of the changed C# files feeds a golden case. Task 1 confirms this with `grep -l 'OpenSky\|ADSBOne\|PublicBinCraft' tools/golden-harness/*.cs` and gives its reasoning in the report. If a case does use one of those files, it is re-run in the docker harness.
- The next finding number is **#339**.
- No test makes a network request.

## Review Focus

1. **A saved OpenSky URL in an existing `settings.toml`** (`feed_settings.OpenSky.url = "https://api.opensky-network.org/api/states/all"`, or with a query) is used as `https://opensky-network.org/api/states/all…` after load. Any other URL, such as a self-hosted one, stays untouched. Test `load_migrates_api_host` in Task 1.
2. **The OAuth token never reaches stderr, logs, status or `Debug`** on the OpenSky path (U4). Test `token_not_logged` in Task 1: run `get_planes` against a mockito token and states server, and assert that the token string is not in `format!("{feed:?}")` and not in any error text.
3. **PublicBinCraft with the new `http://localhost` default** and nothing listening gives a `FeedError` that names no credential, and the worker keeps running. An existing test covers the error path; extend it to the new default.

---

### Task 1: Feeds U1–U4, upstream checkout, cites

**Files:**
- Modify: `crates/as-feeds/src/feeds/adsbone.rs:9`, `crates/as-feeds/src/feeds/opensky.rs:17`, `:223-230`, `crates/as-feeds/src/feeds/publicbincraft.rs:14`, `:61`, `:64-69`
- Test: `crates/as-feeds/tests/core.rs:37`, `crates/as-feeds/tests/json_feeds.rs:529` (default-URL expectations), plus new tests in the existing feed test files
- Upstream checkout: `target/upstream/AirScout` to `fbadef0`

- [ ] **Step 1: Write the failing tests**
  - `adsbone_default_url_is_250`: `DEFAULT_URL == "https://api.adsb.one/v2/point/%MYLAT%/%MYLON%/250"`. Update the existing expectations at `core.rs:37` and `json_feeds.rs:529` to match.
  - `opensky_default_url`: `DEFAULT_URL == "https://opensky-network.org/api/states/all"`.
  - `load_migrates_api_host` (Review Focus 1):
    - `url = "https://api.opensky-network.org/api/states/all?lamin=1"` loads as `https://opensky-network.org/api/states/all?lamin=1`.
    - `http://localhost:8080/api/states/all` stays unchanged.
  - `token_not_logged` (Review Focus 2).
  - `publicbincraft_defaults`:
    - `DEFAULT_HOST == "http://localhost"`;
    - `disclaimer()` equals the upstream text byte for byte, built from the six C# string pieces joined as C# joins them;
    - `enabled_by_default()` stays false.
  - Extend the PublicBinCraft error-path test for Review Focus 3.
- [ ] **Step 2: Run them and see them fail.** `cargo test -p as-feeds`. Expected: the new tests FAIL.
- [ ] **Step 3: Implement**
  - Change the three defaults and the `info()` text that names the default host (`publicbincraft.rs:61`).
  - Add `self.url = self.url.replace("https://api.opensky-network.org/api/", "https://opensky-network.org/api/")` after `set_str(t, "url", …)`, with a comment citing `OpenSky.cs` at fbadef0. The plugin cannot save: the migrated value lives in the plugin, and the next load migrates again. The effect is the same as the C# `Save(true)`. Log it as part of the U2 finding text.
  - Do not port the token `Console.WriteLine`.
- [ ] **Step 4: Checkout and cites**
  - Move `target/upstream/AirScout` to `fbadef0`.
  - Re-check and fix every cite into the three changed files: `grep -rn 'OpenSky.cs\|ADSBOne.cs\|PublicBinCraft.cs' docs/logic-findings.md crates`.
  - Confirm the golden statement from the Global Constraints.
- [ ] **Step 5: Run the gates** (macOS, the full workspace). Expected: green.
- [ ] **Step 6: Commit** `feat(as-feeds): upstream fbadef0 feed defaults, OpenSky URL migration`

### Task 2: Docs, findings, gates

**Files:**
- Modify: `README.md` (feed table near `:505-512`, PublicBinCraft row; any OpenSky or ADSBOne URL mention; the upstream base line, if any), `docs/logic-findings.md` (header `:7`, URL table row `:357`, new rows from #339)

- [ ] **Step 1: README**
  - The PublicBinCraft row: the host is configurable, and the default is `http://localhost`, a self-hosted readsb/tar1090. `globe.airplanes.live` and `adsb.lol` are examples the user must choose.
  - The OpenSky URL, wherever it is shown.
  - The upstream base V1.4.5.1 / `fbadef0`, wherever the README names the upstream version.
- [ ] **Step 2: Findings**
  - Header (`:7`): `fbadef0f`, with the sentence from the Global Constraints.
  - URL table `:357`: the OpenSky default is now `https://opensky-network.org/api/states/all`. Note that this fixes the expired-certificate host found on 2026-11-23.
  - New rows from #339, in this order:
    - U1, **Ported 1:1**;
    - U2, **Ported 1:1**, with the note that the migration runs on each load instead of saving;
    - U3, **Ported 1:1** (default host and disclaimer; our own disclaimer text is replaced);
    - U4, **Not ported** (bearer token written to the console; credential leak; report upstream).
  - No row for U5.
- [ ] **Step 3: Gates**
  - The full gates: macOS, the hamlib pair, the Linux docker gate (`LINUX_OK`) and the Windows mingw gate (`WINGNU_OK`).
  - Afterwards: `rm -rf target/linux target/wingnu` and `docker system prune -f`.
- [ ] **Step 4: Commit** `docs: upstream sync fbadef0, findings`
