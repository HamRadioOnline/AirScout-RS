# GUI deviations log

Every deliberate difference between the Rust GUI and the original AirScout
(DL2ALF) is recorded here. The 1:1 fidelity tests (see
`docs/superpowers/specs/2026-11-26-gui-1to1-design.md`, section 4) fail on any
difference that is not listed in this file and approved by the owner.

Each entry is a `## D-NNN <title>` heading followed by `Window:`, `Differs:`,
`Why:` and `Approved:` lines.

## D-001 Three feed slots

Window: OptionsDlg
Differs: Two extra feed slots (Feed 2 and Feed 3) below Feed 1 in Options → Planes.
Why: Rust supports three feed slots; the original has one. The extra slots sit next to their nearest original (after Feed 1).
Approved: 2026-11-26

## D-002 "Download data for the area…" entry

Window: MapDlg
Differs: A Rust-only "Download data for the area…" entry in the File menu, after the original items.
Why: The upstream window has no menu strip; the entry is a Rust-only convenience that must not displace any original control.
Approved: 2026-11-26

## D-003 `gm_Cache` dropped

Window: MapDlg
Differs: The hidden tile-cache map control `gm_Cache` is not present.
Why: Rust caches tiles without a hidden map control.
Approved: 2026-11-26

## D-004 egui font instead of Designer fonts

Window: all windows
Differs: The egui default font replaces the Designer `Font`; controls may grow to fit the egui text (never shrink below the Designer size).
Why: Rendering stays egui (flat, crisp on HiDPI), not a WinForms skin; the Designer `Font` is ignored by design.
Approved: 2026-11-26

## D-005 Resizable windows, scroll below Designer size

Window: all windows
Differs: Windows are resizable; below the scaled Designer size containers scroll instead of clipping.
Why: Owner decision: the main window and Options must stay usable at 1366×768.
Approved: 2026-11-26
