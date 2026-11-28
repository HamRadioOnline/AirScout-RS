# GUI 1:1 with the original AirScout — design

Status: draft for owner approval (2026-11-26).
Supersedes, for the desktop GUI only, two lines of
[2026-11-23-airscout-rs-design.md](2026-11-23-airscout-rs-design.md): the non-goal
"Pixel-identical WinForms UI" and "Layout follows the original's information architecture,
not its look". The owner decided on 2026-11-26 that the GUI must look like the original.

## Intent

The owner tested alpha-v0.1.0-2 and wants the desktop GUI to look 1:1 like AirScout by
DL2ALF: same windows, same tabs, same group boxes, same labels, same field order, same
arrangement. Somebody who knows AirScout must find every control where it was. The look may
be "a bit more modern": rendering stays egui (flat, crisp on HiDPI), not a WinForms skin.

## Goals

- Every window of the original whose function is ported exists in the Rust app with the same
  control tree: window → tabs → group boxes/panels → controls, in the same order, with the
  same visible texts.
- Proportions and arrangement follow the WinForms Designer values (`Location`, `Size`, `Dock`,
  `Anchor`, `SplitterDistance`), scaled to the display.
- The 1:1 claim is checked automatically, per window, and cannot erode silently.
- Small improvements (owner option C) are allowed only one by one, each approved by the owner
  and recorded in a deviations log.
- **Every control works (owner decision 2026-11-26).** Where the original has a control or a
  whole dialog whose logic the Rust app lacks, that logic is ported too, 1:1 from the C#
  source at the pinned commit, with the stage-6 fixes applied where they touch it. No
  disabled shells, no no-op buttons. Exception: features we deliberately do not port (see
  non-goals).
- Before planning, a gap inventory lists, per window, every original control and whether Rust
  logic exists (`docs/gui-reference/gaps.md`). Logic gaps become their own plan tasks, ported
  test-first like the original port (golden harness where the C# computes values).

## Non-goals

- A grey WinForms theme, Windows system fonts or pixel-exact rendering.
- Changing existing prediction, feed, database or network behaviour as merged in stage 6.
- Windows that belong to features we deliberately do not port (getkey.php, session keys,
  register, upload, plugin download, U4 token print #342). Their menu entries and buttons
  stay absent.
- `ReflectionDlg`: never instantiated upstream (dead code).

## Anti-goals (failure even if it "works")

- A window that matches the reference test but is unusable at small window sizes or on HiDPI.
- Tests that pass because the reference was edited to match the Rust UI.
- Regressions in existing behaviour or in the 1,593 existing tests.
- Personal data or the upstream author's LAN details copied from Designer defaults into code,
  tests or reference files.

## Constraints (carried over, still binding)

- egui/eframe as today; no new crates; `Cargo.lock` unchanged; never `cargo update`.
- `tests/golden/*.json` and `upstream_commit` unchanged.
- Upstream pinned at `91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5` (`tools/fetch-upstream.sh`).
- GPL-3.0-only. Texts copied from upstream Designer files are GPL like the rest; NOTICE stays
  accurate.
- Synthetic test values only (TE1ST, JO50AA, 3C0001, localhost).
- Gates as before: fmt, workspace clippy `-D warnings`, workspace tests, hamlib clippy, Linux and
  windows-gnu Docker gates, hosted CI green.

## Approaches

1. **Hand port per window with a small WinForms layout module.** Readable, improvements per
   spot, but 1:1 rests on reviewer diligence across 26 windows.
2. **Generate the UI from the Designer files.** Layout fidelity almost free, but parsing
   WinForms quirks (Anchor, AutoSize, bindings) is a project of its own and fights option-C
   improvements.
3. **Hand port as in 1, plus an extracted reference and an automatic fidelity test.**
   About one extra day for extractor and tests; in return 1:1 is provable per window.

**Chosen by the owner: 3**, with look option **C** (structure 1:1, modern egui rendering,
approved improvements only).

## Design

### 1. Reference extraction

- An extractor reads the upstream `*.Designer.cs` of the pinned commit and writes one
  reference file per window to `docs/gui-reference/<Window>.json`, committed.
- Per control: `name`, WinForms type, parent, child order (as added to `Controls`, which
  defines z-order/dock order), visible `Text`, `Location`, `Size`, `Dock`, `Anchor`,
  `TabIndex`, `SplitterDistance`/`Orientation` for split containers, column headers for list
  views, items of combo boxes when set in the Designer, menu/status-strip items.
- It parses only the statement shapes the Designer emits (`this.x = new T();`,
  `this.x.Prop = value;`, `this.x.Controls.Add(this.y);`, `Items.AddRange(...)`). Unknown
  statements are reported, not guessed.
- The reference files are regenerated only by the extractor; hand edits are forbidden
  (anti-goal above). Justified differences go into the allowlist (section 4).
- Placement: no new workspace crate (Cargo.lock must not change). Builder's call between an
  `#[ignore]`d generator test or an example in an existing crate, using serde/serde_json
  already in the tree.

### 2. Layout module `crates/airscout/src/ui/forms`

- Reproduces the WinForms layout model in egui: `Dock` Top/Bottom/Left/Right/Fill in child
  order, `SplitContainer` with Designer splitter distance (draggable), `GroupBox` with title,
  `TabControl`, `Panel`, absolute placement from `Location`/`Size` inside a container, and
  `Anchor` for resize behaviour.
- One scale factor maps Designer pixels (96 dpi) to egui points so proportions hold; egui's
  `pixels_per_point` handles HiDPI.
- Widgets get stable ids/labels so egui_kittest can find them by their Designer text.

### 3. Window ports

All windows with ported function, in this order (the main window first, because the owner
looks at it most):

1. `MapDlg` (main window: map/news/profile/spectrum/analysis tabs, right control panel with
   Single/Multi/Opt tabs, alarms, zoom, filter, info, control buttons, status strip with
   database LEDs, TRK/ROT/CAT).
2. `OptionsDlg` (18 tabs: Planes, Path, SRTM1, SRTM3, GLOBE, Map, Stations, General,
   Database, ASTER3, Alarm, Network, Spectrum, Track, CAT, Watchlist, Misc., Info; plus
   ASTER1 if present in the Designer).
3. `FirstRunWizard`, `Splash`.
4. Remaining dialogs: AircraftDatabaseMaintenance, Cleanup, CrossingHistory, DatabaseEdit,
   DatabaseMaintenance, DeleteSingleStation, ElevationCopyright, HistoryFromURL, Horizon,
   License, LocalObstructions, MapProvider, MapStation, PlaneFeedDisclaimer,
   PlaneFeedSettings, PlaneHistory, ScoutBaseDatabaseMaintenance, SetTime, Traffic,
   Watchlist; `PercentageControl` as a reusable widget.

- **Rust-only extras (owner decision 2026-11-26)** — three feed slots, "Download data for the
  area…", Rust-only Options pages — sit next to their nearest original: inside the matching
  original tab or group box, after the original controls (e.g. Feed 2/3 below Feed 1 in
  Options → Planes; the download entry in the File menu after the original items). Each is
  listed in `docs/gui-deviations.md`. A Rust-only Options page with no original tab becomes
  part of the nearest original tab if its fields fit there, else an extra tab after the
  original 18.
- The existing renderers (`map.rs`, `profile.rs`, `spectrum.rs`, `analysis.rs`, `planes.rs`)
  move into the docked panes unchanged; their content is not text-checked (my call).
- `gm_Cache` (hidden tile-cache map in MapDlg) is dropped and allowlisted: Rust caches tiles
  without a hidden map control (my call).
- `PlaneFeedSettingsDlg` keeps the original single-feed form and is opened per slot (my call).
- Each window is one reviewable unit: port, missing logic, fidelity test, snapshot.

### Sizing and text (my calls)

- Designer `Font` is ignored; egui default font. Control sizes may grow to fit the egui text,
  never shrink below the Designer size; relative arrangement stays.
- Windows are resizable. Below the scaled Designer size, containers scroll instead of
  clipping. Target: main window and Options fully usable at 1366×768.
- Designer strings are English; `\r\n` is normalised to `\n` before comparison.

### 4. Fidelity test and deviations

- An egui_kittest test per window loads its reference file and asserts that every visible
  text control (label, button, tab, group box title, check box, radio, column header, menu
  item, status item) exists in the Rust window, under the same container, in the same
  order. Mnemonic `&` is stripped. Rust-only extras are allowed only after the original
  controls of a container and only if listed in `docs/gui-deviations.md`.
- Geometry is checked too, within tolerance (my call): split ratios of `SplitContainer`s and
  the relative order/side of docked children. Absolute pixel positions are not asserted.
- Controls with `Visible = false` in the Designer are excluded; controls hidden at runtime
  count.
- Texts the original sets at runtime (Designer placeholders such as `label1`, `No Messages.`)
  are listed in a per-window allowlist with a reason. The allowlist, not the reference, is
  where differences live.
- `docs/gui-deviations.md` lists every option-C improvement and every allowlisted difference:
  window, what differs, why, owner approval date. The test fails on any difference that is
  not listed there.
- Snapshot images (egui_kittest) per window and tab go into the repo for visual review,
  total budget 10 MB, updated with every UI change of that window. Builder's call whether
  they run on all CI hosts or only on one (rendering backend availability).
- Screenshots of the original (taken 2026-11-26 under Mono, V1.4.5.1) are a local visual
  reference only, in the git-ignored `.superpowers/` workspace. They never go into the repo
  (owner decision 2026-11-26).

### 5. Process

- Plan via writing-plans, executed per window: DeepSeek V4 Pro implements, Gemini 4 Neon
  reviews against the reference file and the Designer source (blocking), Claude only for
  small tasks.
- Work on a feature branch; merge per window or per group after gates and review, as in
  stage 6. Release as the next alpha when the owner says so.

## Left to the builder

- Extractor placement and exact JSON schema.
- Internal API of `ui/forms`, scale factor value (start from Designer 96 dpi → 1 pt).
- How Anchor maps onto egui resize; snapshot host choice.
- Splitting `OptionsDlg` into several plan tasks.
