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

## D-006 Rust-only "Planes" tab

Window: MapDlg
Differs: An extra "Planes" tab after Analysis in `tc_Main` with the plane list.
Why: The plane list is a stage-3 feature with no original counterpart.
Approved: 2026-11-27

## D-007 Glyphs instead of bitmaps

Window: MapDlg
Differs: `btn_Map_PlayPause` displays "Play" or "Pause" in place of the corresponding bitmap, and the five `btn_Analysis_*` player buttons (`Rewind`, `Back`, `Pause`, `Forward`, `Fast forward`) draw `⏮ ◀ ⏸ ▶ ⏭` as painted vector bars and triangles instead of their bitmaps.
Why: The egui button uses text for the same actions, and the default egui font lacks the player glyph codepoints, so they are drawn with the painter.
Approved: 2026-11-27

## D-008 No embedded web browser

Window: MapDlg
Differs: `tp_News` shows the configured news URL as a link and an "Open in browser" button. Answering Yes to the news prompt opens the system browser.
Why: The original embeds a WebBrowser on Windows. egui does not render HTML with the current dependencies.
Approved: 2026-11-27

## D-009 Screenshot as PNG

Window: MapDlg
Differs: `btn_Map_Save` writes a `.png` screenshot in place of the original `.jpg`.
Why: The current dependencies provide no JPEG encoder for the application to use.
Approved: 2026-11-27

## D-010 Rust-only status fields

Window: MapDlg
Differs: After `tsl_CAT` the status bar shows the settings-load error, the plane counts (shown/active) and the feed status.
Why: These carry Rust-only state the original has no single field for.
Approved: 2026-11-27

## D-011 SRTM1 LED tooltip

Window: MapDlg
Differs: `tsl_Database_LED_SRTM1` says "SRTM1 database status LED".
Why: The upstream text says "SRTM3", a copy-paste slip.
Approved: 2026-11-27

## D-012 Runtime placeholder texts

Window: MapDlg
Differs: `tsl_Status`, `tsl_Calculations`, `tsl_Database` and `lbl_Nearest_*` may show runtime values in place of their Designer placeholders ("No Messages.", "Calculations.", "Database status." and "0", respectively).
Why: The original also replaces these placeholders at runtime.
Approved: 2026-11-27

## D-013 News check off by default

Window: MapDlg
Differs: `news_feed_enabled` defaults to false. The original checks airscout.eu every hour by default.
Why: Owner decision on 2026-11-27.
Approved: 2026-11-27

## D-014 Station locations written on outer OK

Window: OptionsDlg
Differs: The station Maps and position fields stage location changes in the Options draft. The local database is updated when Options is confirmed.
Why: Decision 4 preserves the ability to cancel a confirmed child Map edit from Options.
Approved: 2026-11-27

## D-015 Station sharing controls omitted

Window: OptionsDlg
Differs: `btn_Options_MyUpdate` and `btn_Options_DXUpdate` are omitted. The `label52` copy ends with "Your local database is updated."
Why: Decision 5 retains local editing and leaves station uploads outside the port.
Approved: 2026-11-27

## D-016 Station antenna-height rows

Window: OptionsDlg
Differs: The My and DX antenna-height rows follow the original controls in their respective station groups.
Why: The existing Rust antenna-height settings remain editable alongside the ground-elevation readouts.
Approved: 2026-11-27

## D-017 Options runtime status texts

Window: OptionsDlg
Differs: Run-time messages may replace "Status" in `tsl_Options_Status` and "Status is unknown." in `lbl_Options_LocalObstructions`.
Why: The status strip conveys validation, worker results, and local write errors. The obstruction readout shows whether the draft position has a local obstruction for the active model.
Approved: 2026-11-27

## D-018 Stations QRV edits written on outer OK

Window: OptionsDlg
Differs: Both QRV rows stage changes through band, position, call, and tab changes. Options OK applies the station edits in one local transaction. Absent QRV rows display empty fields, and read-time RF defaults still apply. The band field displays the current band and the last-updated labels display stored timestamps or stay empty.
Why: Decision 6 keeps edits cancelable and preserves the distinction between missing data and explicitly entered zero values.
Approved: 2026-11-27

## D-019 Plane feed settings file selection and format

Window: OptionsDlg
Differs: Import and Export use a path field and a listing from the data directory's `Tmp` folder. Each `.feed` file contains one named settings table in AirScout-RS TOML. Import validates the table with the selected feed and the status strip reports the result.
Why: Decision 7 uses the existing in-app file-selection pattern. The TOML format uses the Rust feeds' settings schema and preserves credentials for round trips. Export uses the same private atomic writer as `settings.toml`.
Approved: 2026-11-27

## D-020 Incompatible plane feed imports

Window: OptionsDlg
Differs: AirScout-RS rejects an import if its table name differs from the selected feed or it contains an unknown setting. The status strip reports a fixed error. C# plugins such as Readsb silently skip a foreign XML root and ignore unknown elements.
Why: This prevents overwriting credentials with a foreign feed file.
Approved: 2026-11-28 (Task 5 review)

## D-021 Empty Map placeholder group omitted

Window: OptionsDlg
Differs: `groupBox39` is not drawn. In the original it is an empty group titled "groupBox39" inside the Map tab's `groupBox23`.
Why: Decision 10. The group has no controls and a placeholder title, so drawing it would add an empty shell.
Approved: 2026-11-27

## D-022 User-supplied overlay file selection

Window: OptionsDlg
Differs: "Select" opens a path field and a listing of the image files (`*.jpg`, `*.jpeg`, `*.png`, `*.bmp`, `*.tiff`) in the data directory's `Tmp` folder instead of the native `OpenFileDialog`. The listing starts at the stored file. "Use" stores the file in the draft, and Cancel keeps the draft unchanged.
Why: Decision 7 uses the existing in-app file-selection pattern and adds no dependency.
Approved: 2026-11-27

## D-023 User-supplied overlay drawing and load errors

Window: MapDlg
Differs: The overlay is drawn only while "User Supplied" (`Map_ShowUserSuppliedOverlay`) is checked and the zoom is within 1..=20 (`Map_UserSuppliedOverlay_FromZoomLevel`/`_ToZoomLevel`). It is drawn above the tiles and below the vector layers. The image is decoded off the UI thread. A missing or undecodable file, a file over 64 MiB, an image over 8192 × 8192 pixels, or a reversed or out-of-range bounding box keeps the overlay that was shown before. The main status strip then shows a fixed message without the path. Stored values are never clamped. In the C#, the check box is stored but never read, the overlay is the topmost layer, and `InitializeUserSupplied` clamps the stored settings and only logs load errors.
Why: Plan Task 9 and Review Focus 5. A failed replacement must not remove the committed overlay, and switching the overlay off removes its texture. The size caps keep a pathological file from freezing or exhausting the GUI.
Approved: 2026-11-27 (plan Task 9); owner 2026-11-28

## D-024 Tooltip font picker omitted

Window: OptionsDlg
Differs: `btn_Options_SelectFont` ("Select Font") is not drawn. `label62` "Font:" and the read-only `tb_Options_Map_ToolTipFont` stay in `groupBox7`; the field shows "egui default" instead of the stored `Map_ToolTipFont`.
Why: Decision 9. egui draws one font for every window (D-004), so a font picker would offer a choice that changes nothing.
Approved: 2026-11-27 (plan Decision 9); owner 2026-11-28

## D-025 Plane Speed row inside the crossing column

Window: OptionsDlg
Differs: `cb_Options_InfoWin_Speed` ("Plane Speed") is drawn inside the column box `groupBox33`, at [11, 115] below "Squint Angle". In the Designer it is a child of `groupBox7` at [275, 310], outside the group's 394 × 291 rect, so WinForms clips it.
Why: The check box edits a stored setting and must stay reachable. Speed is a detailed-window row, like the others in `groupBox33`.
Approved: 2026-11-27 (plan Task 10, review fix C4); owner 2026-11-28

## D-026 Rust-only Map tab controls

Window: OptionsDlg
Differs: Below the Designer groups of the Map tab, AirScout-RS shows the preloader tile estimate ("Preloads about N tiles from www.airscout.eu (OpenStreetMap only); off by default.") and the "Thunderforest API key" field with its "show" toggle.
Why: The estimate tells the user how much an explicit preload would download (network off by default). The Thunderforest provider needs an API key; the C# had none to enter. Rust-only controls go after the original controls in their nearest container.
Approved: 2026-11-27 (plan "New deviation entries", I8); owner 2026-11-28
