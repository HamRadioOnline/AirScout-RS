# GUI 1:1 per-window ports — outline

Input for the per-window plans. Window order and phase grouping come from the spec
[2026-11-26-gui-1to1-design.md](../specs/2026-11-26-gui-1to1-design.md), Design §3. The
`rows / ok / missing / not ported` counts and the "missing-logic themes" come from
[`docs/gui-reference/gaps.md`](../../gui-reference/gaps.md) (pinned upstream
`91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`). "Missing" means the upstream control or a
subscribed event has no Rust logic behind it yet; these become the per-window port tasks.

Grand totals (from the `gaps.md` Summary): **887 rows — 769 ok, 114 missing, 4 not ported**
across 24 windows plus `PercentageControl` (25 reference files).

## Window order

Phase 1 (this phase) already delivered the reference extraction, the `ui/forms` layout
module, and the fidelity harness. The remaining windows port in this order:

- **Phase 2 — main window:** `MapDlg`.
- **Phase 3 — Options dialog:** `OptionsDlg`.
- **Phase 4 — first-run / startup:** `FirstRunWizard`, `Splash`.
- **Phase 5 — remaining dialogs:** `AircraftDatabaseMaintenanceDlg`, `CleanupDlg`,
  `CrossingHistoryDlg`, `DatabaseEditDlg`, `DatabaseMaintenanceDlg`, `DeleteSingleStationDlg`,
  `ElevationCopyrightDlg`, `HistoryFromURLDlg`, `HorizonDlg`, `LicenseDlg`,
  `LocalObstructionsDlg`, `MapProviderDlg`, `MapStationDlg`, `PlaneFeedDisclaimerDlg`,
  `PlaneFeedSettingsDlg`, `PlaneHistoryDlg`, `ScoutBaseDatabaseMaintenanceDlg`, `SetTimeDlg`,
  `TrafficDlg`, `WatchlistDlg`, plus `PercentageControl` as a reusable widget.

## Phase 2 — MapDlg

| Window | rows | ok | missing | not ported |
|---|---|---|---|---|
| MapDlg | 115 | 115 | 0 | 0 |

Phase 2 completed station callsign and locator auto-fill, the News tab, the nearest-plane
map overlay, screenshot saving, and the zoom readout. The whole-window fidelity test
covers all MapDlg tab selections.

## Phase 3 — OptionsDlg: 19 tabs

| Window | rows | ok | missing | not ported |
|---|---|---|---|---|
| OptionsDlg | 541 | 477 | 61 | 3 |

Phase 3A ported the General, Stations, Planes and Map pages (the Designer frame and tab order,
the covered-area map, station positions, QRV lifecycle, feed import/export, the Planes groups,
the Map overlay toggles, gauges, multi-path marks, user-supplied overlay and the Info Window
group). The Phase 3A fidelity test compares the frame and those four complete pages. Of the 61
missing rows, 59 belong to Phase 3B (the remaining tabs). The other two are deliberate
omissions on the Map page: `groupBox39` (Decision 10, D-021) and `btn_Options_SelectFont`
(Decision 9, D-024).

Missing-logic themes at the start of Phase 3 (grouped from the Notes; ~144 rows):

1. **User-supplied overlay** — image file + opacity + lat/lon bounds + "show" toggle are not
   ported (15 rows).
2. **"Open directory" actions** — the Tmp/Log/Program "open in Explorer" buttons and the 9
   database-file-name click labels (`Process.Start`) are not ported (13 rows).
3. **Elevation-model tab content** — the per-model description/info groups are not rendered and
   the raw tile-cache (`EnableCache`) toggles are absent, for SRTM1/SRTM3/GLOBE/ASTER1/ASTER3
   (23 rows).
4. **Planes tab** — feed import/export, feed poll interval, extended plausibility check,
   keep-positions-for-N-days/lifetime, and the Debug group; the live filters exist only in the
   main window, not the Planes tab (26 rows).
5. **Stations tab** — callsign `TextChanged` auto-fill (`LocationFind`), ground elevation
   (m asl) vs. antenna height, QRV last-updated timestamp, band stepper/field, locator
   digit/auto-length/subsquares settings, and the station-map button not storing the location
   (`LocationInsertOrUpdateIfNewer`) (24 rows).
6. **Map/Misc/Info chrome** — map overlay toggles (small markers, distance circles, locator
   grid, watchlist, airports), gauges width/forecolor, font/tooltip, info-window visibility
   modes, multi-path, Map Data DB row, news feed/Misc tab, QRZ lookup, donate, callsign import,
   path CSV export/stepwidth, track refresh-rate, CAT poll interval, and position tracing
   (43 rows).

## Phase 4 — FirstRunWizard, Splash

| Window | rows | ok | missing | not ported |
|---|---|---|---|---|
| FirstRunWizard | 67 | 35 | 32 | 0 |
| Splash | 1 | 0 | 1 | 0 |

### FirstRunWizard

Missing-logic themes:

1. **Per-model elevation download pages** — `wp_GLOBE`/`wp_SRTM3`/`wp_SRTM1`/`wp_ASTER3`/
   `wp_ASTER1` with their status/description labels and tile-cache toggles (≈20 rows).
2. **Coverage map** — `gm_Coverage` is absent; the covered area is entered numerically (1 row).
3. **User-details map** — `gm_Callsign` plus the map-zoom readout/buttons are absent (5 rows).
4. **Callsign auto-fill** — `tb_Callsign` `TextChanged` station-DB lookup is missing (1 row).
5. **QRZ lookup** — `btn_QRZ` is not ported (1 row).
6. **Plane feeds 2/3** — the wizard offers only feed 1 (4 rows).

### Splash

Missing-logic themes:

1. **Splash screen** — `pb_Main` is not ported (1 row).

## Phase 5 — remaining dialogs

| Window | rows | ok | missing | not ported |
|---|---|---|---|---|
| AircraftDatabaseMaintenanceDlg | 7 | 7 | 0 | 0 |
| CleanupDlg | 10 | 7 | 2 | 1 |
| CrossingHistoryDlg | 8 | 8 | 0 | 0 |
| DatabaseEditDlg | 5 | 4 | 1 | 0 |
| DatabaseMaintenanceDlg | 8 | 8 | 0 | 0 |
| DeleteSingleStationDlg | 7 | 7 | 0 | 0 |
| ElevationCopyrightDlg | 0 | 0 | 0 | 0 |
| HistoryFromURLDlg | 12 | 11 | 1 | 0 |
| HorizonDlg | 28 | 26 | 2 | 0 |
| LicenseDlg | 1 | 1 | 0 | 0 |
| LocalObstructionsDlg | 4 | 4 | 0 | 0 |
| MapProviderDlg | 2 | 0 | 2 | 0 |
| MapStationDlg | 21 | 14 | 7 | 0 |
| PlaneFeedDisclaimerDlg | 2 | 2 | 0 | 0 |
| PlaneFeedSettingsDlg | 5 | 1 | 4 | 0 |
| PlaneHistoryDlg | 9 | 9 | 0 | 0 |
| ScoutBaseDatabaseMaintenanceDlg | 7 | 7 | 0 | 0 |
| SetTimeDlg | 4 | 4 | 0 | 0 |
| TrafficDlg | 10 | 10 | 0 | 0 |
| WatchlistDlg | 13 | 12 | 1 | 0 |
| PercentageControl | 0 | 0 | 0 | 0 |

Missing-logic themes, for the windows that have any:

- **CleanupDlg** (2 missing): the Log directory choice is not offered (no Log dir); the legacy
  elevation directory is not part of the port.
- **DatabaseEditDlg** (1 missing): `dgv_Main` is read-only — cell edit and validation are not
  reproduced.
- **HistoryFromURLDlg** (1 missing): the directory is typed; there is no browse dialog.
- **HorizonDlg** (2 missing): the map view of the horizon (`rb_Horizon_Plot_Map`, `gm_Horizon`)
  is not ported.
- **MapProviderDlg** (2 missing): the map-provider terms dialog (Accept/Decline) is not ported.
- **MapStationDlg** (7 missing): no zoom readout/buttons (the map zooms with the mouse); no
  elevation readout next to the station.
- **PlaneFeedSettingsDlg** (4 missing): no Info/Version groups or labels — the editor is a
  generic key/value table.
- **WatchlistDlg** (1 missing): no status strip; the lists load inline.

## Recommendation: splitting OptionsDlg and phase 5

- **OptionsDlg — split into two plans.** It is the largest window by far (541 rows, 144
  missing, ~73% of the phase-3–5 missing rows, across 19 tabs). Two plans keep each reviewable.
  Suggested split by tab weight: **A) Stations (24) + Planes (26) + Map (~21) + General incl.
  the user-supplied overlay (~16)** ≈ 87 missing; **B) the remaining tabs** — Path, SRTM1/3,
  GLOBE, ASTER1/3, Database, Alarm, Network, Spectrum, Track, CAT, Watchlist, Misc, Info
  (≈57 missing). The exact tab assignment is the per-plan writer's call (spec §5, "Left to the
  builder: Splitting `OptionsDlg` into several plan tasks").
- **Phase 5 — keep as one plan.** Twenty windows but only 20 missing rows total, thinly spread:
  most dialogs have 0–2 missing rows. The two outliers are `MapStationDlg` (7) and
  `PlaneFeedSettingsDlg` (4), still small. If review granularity matters, only those two could
  be peeled into a small second plan; on missing-count grounds a single plan is fine.
