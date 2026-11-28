# AirScout WinForms GUI survey (for the Rust/egui port)

Base directory for all refs: `/Users/philipp/Projects/hamradioomline/AirScout-RS/target/upstream/AirScout/AirScout/`.
Refs use the form `File.cs:line`. "Core" means the crates already ported (as-geo, as-elevation, as-propagation, as-stations, as-aircraft, as-feeds, as-signals, as-settings).
Out of scope (hooks only): Webserver, WinTest, CAT, DDE/OmniRig.

Mode enums (AirScout.Core/Enums.cs):
- AIRSCOUTPATHMODE: NONE, SINGLE, MULTI.
- AIRSCOUTLIFEMODE: NONE, LIFE, HISTORY.
- AIRSCOUTPLAYMODE: NONE, PAUSE, FASTBACK, BACK, FORWARD, FASTFORWARD.
- AIRSCOUTTRACKMODE: NONE, SINGLE, TRACK, ERROR=128.

GUI enums (Enums.cs):
- TRACKSTATUS: NONE, STOPPED, SINGLE, TRACKING, ERROR.
- ROTSTATUS.

---

## 1. MapDlg (MapDlg.cs, 9977 lines; MapDlg.Designer.cs)

### 1.1 Layout (Designer)

Splitters
- `sc_Main` (vertical): left is the map area, right is `tc_Control`.
- `sc_Map` (horizontal): top is `tc_Map`, bottom is `tc_Main`.
- Splitter distances are persisted in MainSplitter_Distance / MapSplitter_Distance (MapDlg.cs:342-376), restored in FinishStartup at :2091 and saved on close.

`tc_Map` tabs
- `tp_Map` "Map":
  - `gm_Main`: the main GMapControl.
  - `gm_Cache`: a hidden GMapControl used by the preloader.
  - `pa_Rig`: rig panel.
  - `ag_Azimuth` / `ag_Elevation`: AquaGauge tracking gauges.
- `tp_News` "Latest News": WebBrowser `wb_News`.

`tc_Main` tabs (bottom)
- `tp_Elevation` "Path Profile" (title becomes "Pathinfo [model]"): two OxyPlot views stacked at half height each (:6499).
- `tp_Spectrum` "Spectrum":
  - `gb_Spectrum_NearestInfo`: lbl_Nearest_Call / Type / Cat / Alt / Angle / Dist.
  - `gb_Spectrum_Status`: tb_Spectrum_Status.
  - `gb_NearestPlaneMap`: gm_Nearest, a second GMapControl.
  - `gb_Spectrum`: pm_Spectrum plot.
- `tp_Analysis` "Analysis":
  - gb_Analysis_Mode: ON / OFF radio buttons.
  - gb_Analysis_Database: Load Positions, Save Positions, Clear Positions, Load History from URL, tb_Analysis_Status.
  - gb_Analysis_Functions: Show All Traffic, Export Path, Crossing History, Export Plane History.
  - gb_Analysis_Time: tb_Analysis_Time, tb_Analysis_Stepwidth.
  - gb_Analysis_Controls: Rewind, Back, Pause, Forward, FastForward.
  - gb_Analysis_Player: dtp_Analysis_MinValue, dtp_Analysis_MaxValue, sb_Analysis_Play (CustomScrollBar.ScrollBarEx with markers).

`tc_Control` tabs (right; owner-drawn by tc_Control_DrawItem :5730)
- `tp_Control_Single` "Single": gb_Map_Info with cb_MyCall, cb_MyLoc, cb_DXCall, cb_DXLoc, tb_QTF, tb_QRB.
- `tp_Control_Multi` "Multi": lv_Control_Watchlist (columns "   Call", "Loc"; checkboxes; owner-drawn) and btn_Control_Manage_Watchlist.
- `tp_Control_Options` "Opt" (header turns Plum when a filter is active):
  - gb_Map_Zoom: zoom +/- buttons, tb_Zoom, cb_AutoCenter.
  - gb_Map_Filter / pa_Planes_Filter: tb_Planes_Filter_Min_Alt, tb_Planes_Filter_Max_Circumcircle, cb_Planes_Filter_Min_Cat.
  - gb_Map_Alarms: cb_Alarms_Activate.
  - Each group box collapses; collapse state is in Map_ShowInfoBox / ZoomBox / FilterBox / AlarmBox.

Other panels
- `pa_CommonInfo`: tb_UTC (double-click opens SetTimeDlg) and cb_Band.
- `gb_Map_Buttons` "Control": btn_Map_PlayPause, btn_Map_Save, btn_Options.

Status strip `ss_Main`
- Text: tsl_Status, tsl_Calculations, tsl_Database.
- LEDs: Aircraft, Stations, Rig, GLOBE, SRTM3, SRTM1, ASTER3, ASTER1.
- tsl_Track "TRK", tsl_Rot "ROT", tsl_CAT "CAT".

Designer data bindings
- Alarm_Activate, Map_AutoCenter, Planes_Filter_Min_Alt, Planes_Filter_Max_Circumcircle, Locator_SmallLettersForSubsquares.

What the main window does NOT have
- No plane list or grid. Per-plane info lives only in marker tooltips plus the Spectrum "nearest" box.
- No horizon or visibility-zone overlay on the main map. The horizon exists only in HorizonDlg. "Visibility" is the magenta segment on the path.

### 1.2 Run modes
- Startup: PathMode=SINGLE, LifeMode=LIFE, PlayMode=PAUSE (Load, ~:2060).
- Play (:3998-4061):
  - Sets PlayMode=FORWARD and selects the tab matching PathMode.
  - Calls CreateDistances, ClearAllPathsInMap, UpdatePaths, and clears the spectrum.
  - Disables the band, options, call, loc, filter and analysis inputs.
- Pause (:4063-4103): re-enables those inputs and sets TrackMode=NONE.
- Tab selection is cancelled while playing (:9922-9944).
- SINGLE vs MULTI is chosen by the active tc_Control tab (Single :5759, Multi :6046).
- HISTORY mode is entered via the Analysis ON radio (:6515ff):
  - Sets LifeMode=HISTORY and PlayMode=PAUSE.
  - Calls StopAllBackgroundWorkers, then runs bw_Analysis_DataGetter.
  - Analysis OFF clears AllPositions, restarts workers, and returns to LIFE.
- Offline time:
  - Time_Mode_Online / Time_Offline are set via SetTimeDlg (tb_UTC double-click :3529, only when paused).
  - tb_UTC is LightSalmon when online and Plum when offline (:3400).
- Track modes:
  - Right-click the DX marker: TrackMode=SINGLE, with Track_SetAz=QTF and Track_SetEl=0 (:5429-5554).
  - Clicking a plane while playing with Track_Activate: TrackMode=TRACK.

### 1.3 Timers and workers

Timers
- `ti_Progress`: the main tick. Designer interval is 1000 ms; it is set to Map_Update*1000 after Options OK (:3958).
- `ti_Startup`: 5000 ms, one-shot (:2956).
- `ti_ShowLegends`: 5000 ms; hides the chart legends.

Worker fields (:466-497)
- bw_PlaneFeed1..3: MEF plugins.
- Elevation DB updaters: GLOBE, SRTM3, SRTM1, ASTER3, ASTER1.
- StationDatabaseUpdater, AircraftDatabaseUpdater, AircraftPositionDatabaseMaintainer.
- PathCalculator x5.
- CATUpdater and CAT: hooks only.
- MapPreloader.

Designer workers
- bw_WinTestReceive and bw_Webserver: out of scope.
- bw_SpecLab_Receive.
- bw_Track: rotator, plus DDE/serial/UDP outputs.
- bw_JSONWriter, bw_NewsFeed, bw_HistoryDownloader.
- bw_Analysis_DataGetter, bw_Analysis_FileSaver, bw_Analysis_FileLoader.
- bw_AirportMapper.

StartAllBackgroundWorkers (:2166-2455)
- Always started: AirportMapper, JSONWriter, NewsFeed, AircraftDatabaseMaintainer.
- Started when Background_Update_OnStartup (RUNONCE) or Background_Update_Periodically (RUNPERIODICALLY) is set:
  - Station and aircraft updaters.
  - Elevation updaters, per model with Elevation_*_Enabled.
  - PathCalculators.
  - MapPreloader, only if Map_Preloader_Enabled.
- PlaneFeeds 1-3: one worker per selected plugin, with args built by CreatePlaneFeedWorkEventArgs (:2129-2164).
- Server_Activate starts bw_WinTestReceive and bw_Webserver. This is the WinTest/Webserver hook (:2404-2414).
- SpecLab_Enabled starts bw_SpecLab_Receive.
- Track_Activate starts bw_Track.
- CAT_Activate starts bw_CATUpdater and bw_CAT. This is the CAT hook (:2428-2453).

Stop and cancel
- StopAllBackgroundWorkers (:2477) busy-waits with Application.DoEvents.
- CancelAllBackgroundWorkers (:2505).

### 1.4 Per-tick pipeline

ti_Progress_Tick (:5353-5400)
- LIFE + FORWARD + online: CurrentTime = UtcNow, then UpdatePlanes.
- HISTORY and not PAUSE:
  - Time_Offline += Time_Offline_Interval, clamped to the dtp min/max.
  - Updates sb_Analysis_Play.
  - Planes.Clear(), then refills from AllPositions where Time is in [min, CurrentTime].
  - Calls UpdatePlanes.

UpdatePlanes (:5093-5229)
1. UpdateStatus (:3400): UTC, My/DX call and locator, QTF/QRB via LatLon.Bearing / LatLon.Distance. Fields turn PaleGreen when the locator is precise.
2. If WatchListUpdated, call UpdatePaths.
3. The filter panel turns Plum when a filter is active.
4. `allplanes = Planes.GetAll(CurrentTime, Planes_Position_TTL)` (PlaneInfoCache).
5. For each PropagationPath:
   `AircraftData.Database.GetNearestPlanes(DateTime.UtcNow, ppath, allplanes, Planes_Filter_Max_Circumcircle, band.MaxDistance, Planes_MaxAlt, Planes_MaxDelta)`.
   Note: this uses UtcNow even in HISTORY mode.
6. Merge into ActivePlanes: higher Potential wins; on a tie, smaller IntQRB wins.
7. NearestPlane = highest potential, then nearest, within MaxDistance.
8. MULTI: writes the highest potential into the watchlist item's ToolTipText, which drives row colors.
9. Clears gmo_Planes, gmo_Routes and the chart plane points, then calls DrawPlanes.

UpdatePaths (:4313-4596), run on Play, band/call changes, or watchlist changes
- Sets Background_Calculations_ThreadWait=1000 during the update and 0 after, to throttle the PathCalculators.
- Maintains the MyCalls MRU list (max 10).
- myloc = LocationFindOrUpdateOrCreate. Then QRVFindOrCreateDefault, with default height, gain and power.
- SINGLE:
  - LocalObstructionFind.
  - ElevationPathFindOrCreateFromLatLon(stepwidth, model).
  - PropagationPathFindOrCreateFromLatLon(h1 = elev + antenna height, h2, GHz, R*K_Factor, F1_Clearance, stepwidth, model, obstruction).
  - Inserts DX into the watchlist. It is marked OOR if distance > Path_MaxLength. Capped at Watchlist_MaxCount.
- MULTI: the same per checked watchlist item, via LocationFindOrCreate.
- Then DrawPath per path, UpdateCharts (SINGLE only), SetZoomToFitRect, and Map_AutoCenter.

### 1.5 Map overlays (InitializeMaps :1335-1417)

gm_Main
- Zoom 0-20, start zoom 6.
- Provider: `GMapProviders.Find(Map_Provider)`.
- Map_Opacity applies.

Overlay z-order, bottom to top
- Locators, Distances, Airports, PropagationPaths, Objects, Routes, Planes, Callsigns, CallsignDetails, UserSupplied.
- NearestPaths and NearestPlanes belong to gm_Nearest.

Distances (CreateDistances :1317)
- Rings every 100 km out to 1000 km, DarkGray.

Locators (InitializeLocators :1577-1682, CreateLocators :1419)
- A GMapLocatorPolygon grid over the visible area, rebuilt on zoom change. Hidden while dragging.
- Grid by zoom:
  - Zoom 1-5: 10x20 deg.
  - Zoom 6-9: 1x2 deg.
  - Zoom 10-13: 1x2 deg plus 1/24.
  - Zoom 14-17: 1/24 plus 1/240.
  - Zoom 18-20: 1/240 plus 1/5760.
- Locator_SmallLettersForSubsquares controls the label case.

Airports (UpdateAirports :2929)
- Markers with tooltip "name\nIATA/ICAO".
- Data from bw_AirportMapper (:9873, AircraftData.AirportGetAll).

UserSupplied (:1512-1575)
- An image overlay stretched to the MinLat/MaxLat/MinLon/MaxLon bbox, with its own opacity.

DrawPath (:4122-4311)
- Markers:
  - MyLoc: red.
  - DX: yellow, with tooltip call / lat / lon / loc / elevation / bearing / km.
  - Midpoint: blue.
  - Three small markers (red, blue, yellow) at the first, middle and last "hot" points, where max(H1,H2) < Planes_MaxAlt.
- Full path route: black if Valid, red otherwise, width 3. Also drawn on gm_Nearest.
- Mutual-visible segment: magenta, width 3, over points with 0 < max(H1,H2) < Planes_MaxAlt (:4196-4219).
- Beam wedges (Map_ShowBeamWidth): `SupportFunctions.GetBeamWidthFromGain`, extending to the path length, gray alpha 50 (:4222-4305).

Watchlist on the map (UpdateWatchlistInMap :5235)
- Marker icon: green_small if FROMUSER, else white_small.
- The label is shown Always when MULTI, Map_LabelCalls and the item is checked.

Selected plane
- A thin black line to IntPoint in gmo_Routes.

Map events
- OnMapZoomChanged (:5414): autocenter, tb_Zoom, locators, user overlay.
- OnMarkerClick (:5429-5554):
  - Callsign marker, left click: MULTI toggles the watchlist check; SINGLE sets DX.
  - Plane marker: toggles selection. Selection is single if Track_Activate; sets TRACK when playing.
  - DX marker, right click: TrackMode SINGLE.
- Drag handlers: :5556-5599.
- OnTileLoadComplete: :5703.

### 1.6 Aircraft drawing

Icons
- InitializeIcons (:2640): image lists L / M / H / S, sized by Planes_IconSize_*.
- Built icons: 101 gradient icons (GetColor :2609, HSV hue = potential*0.3), plus gray, lightgreen, darkorange, red, magenta.
- CreatePlaneIcon (:2562) recolors plane.png. Icons are also saved as PNG to IconDirectory.

Markers: CreatePlaneSimple (:4751-4820) / CreatePlaneDetailed (:4822-4954)
- Icon by Potential:
  - 100: magenta, tooltip Plum.
  - 75: red.
  - 50: dark orange.
  - Otherwise: gray, tooltip white alpha 180.
- Size by Category: SUPERHEAVY uses S, HEAVY uses H, MEDIUM uses M, LIGHT uses L.
- Rotation: Track.
- Tooltip lines, gated by InfoWin_*:
  - Position.
  - Alt (metric or imperial, plus AltDiff).
  - Track, Speed, Type.
  - If Potential > 0: Dist (IntQRB), Time (IntQRB / kmh * 60 min), Angle, Eps1 <> Eps2, Squint.
  - If TrackMode==TRACK: My/DX azimuth, elevation, Doppler.
- Selected planes show their tooltip Always.

DrawPlanes (:4956-5091)
- Filter: Alt_m >= Planes_MinAlt and Category >= Planes_Filter_Min_Category.
- Alarm (:3457): fires for Potential 100/75 with IntQRB < Alarm_Distance.
  - gb_Map_Alarms turns Plum.
  - Optional SetForegroundWindow / TopMost and Beep.
- tp_Map.Text shows the plane count.
- SINGLE: TooltipDataPoint(dist My-to-IntPoint, Alt_m, call) goes into Planes_Hi (AltDiff > 0) or Planes_Lo, if IntQRB <= band MaxDistance.
- No selection sets TrackMode=NONE.

Tracking gauges (gm_Main_Paint :5602-5662)
- Drawn when Map_TrackingGaugesShow, FORWARD and TRACK.
- Shows ag_Azimuth, ag_Elevation and the pa_Rig panel (MyDop, DXDop, Dial, RX, TX).
- Layout comes from Map_TrackingGaugeWidth (SizeChanged :5664).

### 1.7 Elevation / path chart

InitializeCharts (:2748-2897)

pm_Path
- Axes: X 0-1000 km, Y 0-20000 m.
- Series:
  - Path_Elevation.
  - Min_H1: red.
  - Min_H2: gold.
  - Max_H: dark blue, dotted.
  - Min_H: magenta area, saturation 0.4.
  - Planes_Hi: magenta squares.
  - Planes_Lo: gray squares.

pm_Elevation
- Elevation (area) and LOS (black).

UpdateCharts (:4602-4658)
- Uses epath / ppath.GetInfoPoints.
- eps_los = Propagation.EpsilonFromHeights.
- Per point: elevation, H1, H2, Max_H = Planes_MaxAlt.
- Min_H area spans MaxAlt to min(MaxAlt, max(H1,H2)).
- LOS = HeightFromEpsilon.
- Elevation Y max = max + 10%.
- Legends are shown for 5 s.

pv_Path_Paint (:4705)
- Draws the My and DX callsigns vertically at the chart ends.

### 1.8 Spectrum (SpecLab) :7630-7833
- Polls `SpecLab_URL/FileName?f1&f2` every SpecLab_Update s (min 1 s). Only in LIFE + FORWARD.
  - Bug: it uses "?f2" where "&f2" is needed.
- Parses the JSON FFT and takes the max.
- Saves SignalLevelDesignator via `SignalData.Database.SignalLevelBulkInsertOrUpdateIfNewer` every 60 samples, only while NearestPlane != null.
- pm_Spectrum:
  - X 0-600 (SpectrumMaxPoints=600), Y -120..0.
  - Spectrum: goldenrod spline, width 3.
  - SpectrumRecord: magenta, shown as a background band while a plane is near.
  - Y autoscales.
- The Nearest box shows the labels and centers gm_Nearest at zoom 10.

### 1.9 Watchlist UI
- Model: Watchlist.cs.
- lv_Control_Watchlist (:6059-6332):
  - ItemCheck sets WatchListUpdated when playing.
  - ItemChecked syncs the settings.
  - Column click checks all non-OOR items.
  - DrawItem background by potential: >0 orange, >50 red, >75 magenta.
  - Selecting an item sets DX.
  - Hover tooltip shows QRB/QTF.
- AddListViewItem (:5261): OOR items get gray text; FROMUSER items get PaleGreen.
- RefreshWatchlistView (:5287): sorted with checked items first; preserves scroll position.
- Manage (:6242) opens WatchlistDlg.
- Out-of-scope hooks: WinTest ADDWATCH / REMOVEWATCH / ASWATCHLIST also edit it.

### 1.10 History / analysis (tp_Analysis :6515-6948)
- bw_Analysis_DataGetter (:9341-9499):
  - Reads AircraftPositionOldestEntry / YoungestEntry / Count / GetAll.
  - Enriches with AircraftFindByHex and AircraftTypeFindByICAO.
  - Builds AllPositions and AllLastUpdated (:553-555).
  - On completion: enables controls, sets Time_Offline, calls UpdatePaths.
- Save JSON/CSV: bw_Analysis_FileSaver (:9506).
- Load json / csv / hist: bw_Analysis_FileLoader (:9609-9866), then reruns DataGetter.
- Clear: AircraftPositionDeleteAll.
- History from URL:
  - HistoryFromURLDlg, then bw_HistoryDownloader (:8655-8920).
  - Downloads `Analysis_History_URL/<date>.zip`. Requires 50 GB free disk.
  - Extracts and imports the JSON into the aircraft-position DB.
- Show traffic: TrafficDlg.
- Export Path CSV (:6694-6825):
  - Columns: Distance, Lat, Lon, Elevation, Min_h1, Min_h2, Min_h, Max_h, F1, eps1, eps2, eps1_min, eps2_min.
  - Uses ppath.GetInfoPoint(dist).
- Crossing history: `CrossingHistoryDlg(min, max, 60, ref AllPositions)`.
- Plane history: PlaneHistoryDlg.
- Player:
  - Rewind -10, Back -1, Pause 0, Forward +1, FastForward +10, in seconds per tick. Clamped to 1..3600.
  - Bug: Back and Rewind clamp the step to >= 1, so they still advance.
  - Scrollbar scroll sets Time_Offline and calls UpdatePlanes.
  - Scrollbar markers come from AllLastUpdated.

### 1.11 Other MapDlg pieces

Startup
- Constructor (:589-686):
  - InvariantCulture, TLS 1.2.
  - InitializeSettings (:1085), InitializeCharts, InitializeWebbrowser.
  - `/CLEAN` shows CleanupDlg.
- InitializeSession (:751-809):
  - Builds a DeviceId (WMI on Windows).
  - Registers via AirScout_Register_URL.
  - SessionKey = OpenSSLDecrypt. Used by plane feeds and uploads.
- LoadPlugins (:915-1027):
  - Copies plugins and downloads planefeeds.zip from Plugins_Update_URL.
  - MEF compose and a version check.
- CheckSettings (:1160-1245):
  - Validates window bounds.
  - Resets My/DX if they are not in the station DB.
- CheckDirectories (:811): cleans the PlanePositions directory.
- MapDlg_Load (:1711-2089):
  - Order: splash, plugins, icons, band/category combos, gauges, PlaneFeed workers.
  - FirstRun branch: Upgrade, CheckInternet (:1247), FirstRunWizard; exits on abort.
  - Then InitializeSession, StartAllBackgroundWorkers, InitializeMaps, UpdateAirports, watchlist, modes, ti_Startup.

Feed and status handlers
- bw_PlaneFeed_ProgressChanged (:6978): PROGRESS.PLANES calls `Planes.BulkInsertOrUpdateIfNewer`.
- Status helpers (:3274-3398):
  - Say writes tsl_Status.
  - SayDatabase, SayCalculations, SayAnalysis write their own fields.
  - SayTrack / SayRot / SayCAT set colors.
- LED updates:
  - ElevationDatabaseUpdater progress (:8927) drives the LED colors (DatabaseStatus.cs).
  - PathCalculator progress (:9018) writes tsl_Calculations.
  - Station updater (:9029); on completion it reruns UpdateAirports.
  - Aircraft updater (:9084), maintainer (:9144).

Periodic workers
- bw_JSONWriter (:8463): writes Tmp/planes.json, FR24-like, every 60 s.
- bw_NewsFeed (:8552):
  - Checks every News_Interval s via GetWebCreationTimeUtc.
  - Shows a MessageBox and switches to tp_News.
- bw_Track (:7839-8459), rotator / Doppler:
  - Loop every Track_Update ms.
  - Gets the plane with `Planes.Get(Track_CurrentPlane, now + Track_Offset, TTL)`.
  - Computes LatLon.Bearing / Distance, Propagation.SlantRangeFromHeights, EpsilonFromHeights(R*K), DopplerShift(Bands.ToHz).
  - Doppler strategies A-D produce RX/TX.
  - ReportProgress(3, TrackValues).
  - Outputs: DDE HRD (out of scope), UDP WinTest / AirScout broadcast, serial GS-232A, azel.dat files (native and WSJT format).
  - The UI forwards values to bw_CAT (hook).
- MapPreloader progress (:9184): drives gm_Cache Zoom / Position / ReloadMap on the UI thread.

Elevation helpers
- GetElevation (:3649): fallback order ASTER1, ASTER3, SRTM1, SRTM3, GLOBE. A value <= TileMissingFlag gives 0.
- GetMinMaxElevationLoc (:3672).
- Path_BestCaseElevation substitutes the tile max (:3544-3642).

Options and saving
- ShowOptionsDlg (:3882-3992):
  - Stops workers and saves settings before opening the dialog.
  - On OK: re-inits charts, clears path caches, updates the map provider/font, recreates feeds, sets the ti_Progress interval, InitializeDatabase.
  - On Cancel: LoadUserSettings.
  - Always: InitializeMaps and StartAllBackgroundWorkers.
- MapSave (:3490): screenshot via CopyFromScreen, saved as .jpg in Tmp.

Settings persistence and shutdown
- LoadUserSettings / SaveUserSettings (:2999-3068, :3082):
  - Windows: user.config Reload.
  - Mono: Reset plus one JSON file per settings class.
  - Settings classes: ScoutBase.Elevation / Stations / Propagation / CAT, AirScout.Aircrafts / PlaneFeeds / CAT, AirScout.Properties.
- FormClosing (:3131-3260):
  - Pause, save the window, cancel workers, save splitters.
  - Rig LeaveDoppler (CAT hook).
  - Back up the in-memory DBs (StationData IsInMemory / BackupDatabase), SaveUserSettings.

---

## 2. PathCalculator, MapPreloader, MapProvider

### PathCalculator.cs (nested class in MapDlg, :34-304)
- One BackgroundWorker per elevation model, with a progress text going to tsl_Calculations.
- Loop, only when Background_Calculations_Enable:
  - Waits in 10 s sleeps until the elevation and station DB status are UPTODATE and the station DB file timestamp != StationsDatabase_TimeStamp.
  - Calls `StationData.LocationGetAll(bbox)`.
  - For each location within Path_MaxLength, for each band:
    - PropagationPathFind; else PropagationPathCreateFromLatLon.
    - Uses BestCase elevation via ElevationTileFindMinMaxInfo, QRVFind height, and LocalObstruction.
  - Sleeps Background_Calculations_ThreadWait per existing path and 100 ms per location.
  - Saves the timestamp.
- Then sleeps Background_Update_Period*60 s, if periodic.
- Bug: Name / StepWidth are only set for GLOBE / SRTM3 / SRTM1. ASTER3 and ASTER1 have no name or stepwidth.
- Port note: this is pure core logic (as-propagation plus as-stations). The GUI needs only status text and a cancel token.

### MapPreloader.cs (:44-264)
- Model = the first enabled of GLOBE / SRTM3 / SRTM1. If none is enabled, it returns.
- Tile set: zoom 5-11 over the Min/Max Lat/Lon bbox, shuffled.
  - Bug: Map_Preloader_MinZoom / MaxZoom are NOT used.
- OSM provider: LoadOSM fetches z/x/y.png from the ScoutBase.Maps Database_UpdateURL and enqueues it into the GMap.NET cache. Other providers use Manager.GetImageFrom.
- Sleeps 100 ms per tile.
- Every 100 tiles it waits for the cache queue with Application.DoEvents. This runs on a worker thread, which is wrong.
- Periodic sleep: Background_Update_Period.
- Port note: in egui (walkers or a custom slippy map), replace this with a tile-cache prefetch task.

### MapProvider.cs
- `MapProviders.GetAll()` returns:
  - OpenStreetMap: `https://tile.openstreetmap.org/{z}/{x}/{y}.png`.
  - OpenTopoMap: `https://{s}.tile.opentopomap.org/...`.
  - OCM Landscape: Thunderforest, with an API key.
- Each entry has attribution text, MinZoom 1, MaxZoom 19.
- It only fills the Map_Providers setting (consumed by the webserver map). gm_Main uses `GMapProviders.Find(Map_Provider)`.
- Port note: this is the natural seed for the egui tile-source list (URL template, attribution, zoom range).

### GMapLocatorPolygon.cs
- A GMapPolygon subclass.
- OnRender draws the Tag (locator text) centered, in Arial, font size = width/10, in the stroke color.

### TrackValues.cs
- POD: Timestamp, Hex, MyAzimuth, MyElevation, MyDistance, MySlantRange, MyDoppler, DXAzimuth, DXElevation, DXDistance, DXSlantRange, DXDoppler, RXFrequency, TXFrequency.

### Watchlist.cs
- WatchlistItem fields: Call, Loc, Checked, Selected, OutOfRange, Remove.
- CompareTo sorts by Call, then Loc.
- `Watchlist : List<WatchlistItem>` with IndexOf(call) and IndexOf(call, loc).
- Persisted in the Watchlist setting.

### Settings.cs
- An empty partial class.
- The real settings are Properties/Settings.settings (242 entries), plus Planes_MaxDelta, which exists only in Settings.Designer.cs (default 1000). See section 4.

### Small support files
- DatabaseStatus.cs: LED colors.
  - ERROR red, UPTODATE green, COMPLETE blue, UPDATING gold, EMPTY black, default plum.
  - Also provides status text.
- TooltipDataPoint.cs: an OxyPlot point with a tooltip.
- Program.cs: AttachConsole P/Invoke, then Run(MapDlg).

---

## 3. Dialogs

### OptionsDlg (OptionsDlg.cs, 3371 lines; Designer)

Tabs
- General, Database, Stations, Map, GLOBE, SRTM3, SRTM1, ASTER3, ASTER1, Path, Planes, Alarm, Network, SpecLab, Track, CAT, Watchlist, Misc, Info.

Load (:71-184)
- dgv_BandSettings is bound to Path_Band_Settings.
- Columns: Band, K-Factor, F1-Clearance, Ground Clearance, Max Distance, Max Squint, Max Elevation.

General
- Min/Max Lat/Lon, with a coverage polygon preview map (:256).

Database (:296-520)
- DB sizes and locations.
- Update mode radio buttons: Never / OnStartup / Periodically, with a period.
- Delete all elevation paths / propagation paths per model.
- Delete map tiles: MapData.TileDeleteAll.
- DeleteSingleStationDlg.
- DatabaseMaintenanceDlg per DB.
- Explorer links.

Stations (:525-1190)
- My/DX call, lat, lon, loc; height.
- QRZ lookup: scrapes cs_lat / cs_lon from QRZ_URL_Database.
- Opens HorizonDlg.
- QRV load / save: QRVFind / Delete / InsertOrUpdateIfNewer.
- Opens MapStationDlg.
- Opens LocalObstructionsDlg: LocalObstructionFindOrCreateDefault / InsertOrUpdateIfNewer.
- Band up/down buttons.
- Saving happens on Leave.
- bw_StationDataUpdater (:3301): uploads location/QRV, encrypted with SessionKey, to AirScout_UploadLocation_URL / AirScout_UploadQRV_URL.
- Import callsigns from TXT (:3273): LocationFromTXT.

Map (:1194-1352)
- Provider combo. The full GMapProviders.List appears only after MapProviderDlg acceptance.
  - The acceptance ID uses the registry ProductId plus WindowsIdentity. Windows-only.
- Font dialog, gauge color.
- Other map settings: opacity, icon sizes, beam width, label calls, preloader, user overlay.

GLOBE / SRTM3 / SRTM1 / ASTER3 / ASTER1 tabs (:1356-1947)
- Enable checkbox.
- A worker runs ElevationCatalogueCreateCheckBoundsAndLastModified plus ElevationTileExists.
- The map draws 1/24 x 2/24 deg tiles: green if present, red if missing.
- Copyright button opens ElevationCopyrightDlg.

Path (:1951-2164)
- Path settings, Path_BestCaseElevation, Path_MaxLength.
- Export elevation path CSV.

Planes (:2168-2575)
- 3 plugin combos.
- Disclaimer: PlaneFeedDisclaimerDlg.
- Settings: PlaneFeedSettingsDlg (PropertyGrid over plugin GetSettings / SaveSettings).
- Import / Export / Default.
- Min/Max alt, TTL, minimum category, InfoWin_* checkboxes.

Alarm
- Alarm_* settings.

Network
- Server_* and Webserver_Port (out of scope: bindings only).

SpecLab
- SpecLab_* settings.

Track (:2620-2742)
- Radio groups mapping to Track_* / Doppler_* bools.
- UDP/serial port conflict check.

CAT (:2746-3085)
- Hook only.

Watchlist (:3163-3221)
- Manage (WatchlistDlg) and clear.

Misc (:3225-3256)
- Process.Start of directories; donate link.

Info (:3089)
- Version and license.

### FirstRunWizard (AeroWizard)
- Pages:
  - TermsAndConditions: reads the TERMSANDCONDITIONS file.
  - GeneralCoverage: Min/Max Lat/Lon.
  - ElevationModel: Elevation_*_Enabled.
  - GLOBE / SRTM3 / SRTM1 / ASTER3 / ASTER1: tile coverage maps, same worker as Options.
  - UserDetails: call, lat, lon, loc, draggable map marker, QRZ lookup. Writes MyCall / MyLat / MyLon.
  - PlaneFeeds: plugin combos.
  - Finish.
- Closing waits for its workers.
- Abort makes MapDlg exit.
- Port: an egui multi-step modal.

### HorizonDlg
- Constructor: (call, lat, lon, LocalObstructionDesignator). CalculationDistance is 500 km.
- Calculates `PropagationHorizonFindOrCreate(lat, lon, elev + height, 500, GHz, R*K, F1, stepwidth, model, obstruction)` on a worker.
- Progress 0..359 draws the horizon live; 360 delivers the full designator.
- Plots:
  - Elevation angle (Epsmin) and distance, each as polar or cartesian (Horizon_Plot_* radios).
  - A map route of DestinationPoint(az, dist).
- Export CSV columns: Bearing, Eps_Min, Distance, Elevation.
- Map mouse-down tooltip: lat, lon, elevation, QRB, QTF.

### MapStationDlg
- Edits a LocationDesignator: call, lat, lon, loc, elevation.
- Draggable marker plus a locator polygon.
- StationsMap_OverlayElevation: at zoom >= 17, bw_Elevationgrid draws an elevation grid colored by GetElevationColor, with GMapTextPolygon labels.

### CrossingHistoryDlg
- Worker:
  - Computes the PPath.
  - Preselects AllPositions within PPath.Distance/2 of the midpoint.
  - Steps time by Stepwidth (60 s from MapDlg), using a PlaneInfoCache window of the TTL.
  - Calls GetNearestPlanes.
  - A crossing is IntQRB <= MaxDistance, plus SignalLevelFind (filter: Analysis_CrossingHistory_WithSignalLevel).
- Chart: DataVisualization column chart. Count_Hi (potential 100) is red; Count_Lo is blue.
- CSV export with ambiguity detection (Analysis_CrossingHistory_AmbigousGap).
  - Bug: the header has no separators (:349-369).

### WatchlistDlg
- Two DataGridViews:
  - All callsigns: LocationGetAll(bbox), filtered by Location_RestrictToAreaOfInterest.
  - Selected: the current Watchlist.
- Add, Remove, RemoveAll; prefix filters.
- The caller rebuilds the Watchlist from dgv_Watchlist_Selected.

### PlaneHistoryDlg
- Worker: AircraftPositionGetAllHex / GetAllCalls(from, to).
- Exports CSV per hex or call (AircraftPositionGetAllByHex / ByCall).
- CSV columns: time, hex, call, lat, lon, alt m, track, speed kmh, dist, av_speed.

### Brief dialogs
- AircraftDatabaseMaintenanceDlg: listview with a Delete button.
  - Copy-paste bug: it deletes the station Location / QRV tables.
- ScoutBaseDatabaseMaintenanceDlg: Location / QRV counts, Delete All.
- DatabaseMaintenanceDlg(ScoutBaseDatabase, model):
  - Table list with description and row count.
  - "Delete All" calls ClearTable.
  - "View" runs SELECT on non-blob columns and opens DatabaseEditDlg.
- DatabaseEditDlg(DataTable):
  - Editable grid with Find Next (F3).
  - CellValidating stamps LastUpdated with the current UNIX time.
- CleanupDlg (`/CLEAN` argument):
  - Checkboxes: reset all settings classes; delete the aircraft / stations / elevation / propagation DB directories; delete the Log / Elevation / Tmp / Plugin directories.
  - Retry on failure.
- DeleteSingleStationDlg: call/locator combos from LocationGetAll("%x%"), plus btn_Delete.
- ElevationCopyrightDlg: rich-text display.
- HistoryFromURLDlg: bound to Analysis_History_URL, Analysis_History_Directory (folder picker), Analysis_History_Date, Analysis_History_Stepwidth.
- LicenseDlg: shows the license file.
- LocalObstructionsDlg: DataTableLocalObstructions grid (Distance / Height per bearing); Clear All.
- MapProviderDlg: "Map Provider - Term of Use", Accept / Decline. Gates the full provider list (Map_Provider_Accepted).
- PlaneFeedDisclaimerDlg: tb_DisclaimerText, Accept.
- PlaneFeedSettingsDlg: pg_Main PropertyGrid, lbl_Info, lbl_Version.
- ReflectionDlg: "Log Reflection"; UTC, Call, Lat, Lon, Alt labels; Yes / No. Designer-only, apparently unused.
- SetTimeDlg: bound to Time_Online (checkbox) and Time_Offline (dtp_SetTimeDlg_Start).
- Splash: borderless; status label; fades in via ti_Animation (+0.01 opacity); ti_Close; click hides it. Version is drawn in pb_Main_Paint.
- TrafficDlg:
  - Own GMapControl with a magenta coverage polygon.
  - Start/Stop default to the oldest/youngest position entries.
  - Worker: AircraftPositionGetAllHex, then GetAllByHex per hex.
  - Draws black routes with ud_Opacity alpha. A route splits on a jump >= 100 km.
- PercentageControl: NumericUpDown that displays a percentage.

---

## 4. Settings groups (names only)

Source: Properties/Settings.settings (242 entries) plus Planes_MaxDelta, which is in Designer.cs only. (GUI) marks GUI-only state.

Station / identity
- MyCall, MyLat, MyLon, MyHeight, MyElevation, MyCalls (MRU).
- DXCall, DXLat, DXLon, DXHeight, DXElevation.
- Band, Path_Band_Settings.
- Location_RestrictToAreaOfInterest.

Area
- MinLat, MaxLat, MinLon, MaxLon.

Window (GUI)
- General_WindowLocation, General_WindowSize, General_WindowState.
- MainSplitter_Distance, MapSplitter_Distance.
- Map_ShowInfoBox, Map_ShowZoomBox, Map_ShowFilterBox, Map_ShowAlarmBox.

Map (32 entries)
- Map_Provider, Map_Providers, Map_Provider_Accepted (GUI), Map_Opacity, Map_Update, Map_AutoCenter.
- Map_LabelCalls, Map_ShowBeamWidth, Map_TrackingGaugesShow, Map_TrackingGaugeWidth.
- Map_Preloader_Enabled, Map_Preloader_MinZoom / MaxZoom.
- Map font and gauge color.
- User-supplied overlay: file, opacity, bbox.
- Locator_* (Locator_SmallLettersForSubsquares).

Planes (23 entries)
- Planes_Update, Planes_Interval, Planes_Position_TTL, Planes_MinAlt, Planes_MaxAlt, Planes_MaxDelta.
- Planes_Filter_Min_Alt, Planes_Filter_Max_Circumcircle, Planes_Filter_Min_Category.
- Planes_IconSize_L / M / H / S.
- Plugin selections 1-3, KeepHistory, plausibility settings.

InfoWin (18 entries)
- Tooltip field toggles, metric / imperial.

Alarm (4 entries)
- Alarm_Activate, Alarm_Distance, BringWindowToFront, PlaySound.

Path (7 entries)
- Path_MaxLength, Path_BestCaseElevation, step width, export settings.

Elevation (21 entries)
- Elevation_GLOBE / SRTM3 / SRTM1 / ASTER3 / ASTER1_Enabled, ElevationModel, per-model DB status.

Background (5 entries)
- Background_Update_OnStartup, Background_Update_Periodically, Background_Update_Period.
- Background_Calculations_Enable, Background_Calculations_ThreadWait (GUI runtime).

Databases
- StationsDatabase_TimeStamp and the DB status fields (GUI runtime).
- AircraftDatabase_* (5 entries), including AircraftDatabase_MaxDaysLifetime.
- Directories: Aircraft / Stations / Elevation / PropagationDatabase_Directory.

Watchlist (4 entries)
- Watchlist, Watchlist_MaxCount, sync / remove options.

Horizon (4 entries)
- Horizon_Plot_* (GUI).

Track (21 entries)
- Track_Activate, Track_Update, Track_Offset.
- Track_CurrentPlane, Track_SetAz, Track_SetEl (GUI runtime).
- Output selection: DDE, UDP, serial, file.

Doppler (13 entries)
- Strategy A-D, Doppler_Dial* (GUI runtime).

SpecLab (6 entries)
- SpecLab_Enabled, SpecLab_URL, SpecLab_FileName, SpecLab_F1, SpecLab_F2, SpecLab_Update.

Analysis (8 entries)
- Analysis_History_URL / Directory / Date / Stepwidth.
- Analysis_History_LoadFileName (GUI).
- Analysis_CrossingHistory_WithSignalLevel, Analysis_CrossingHistory_AmbigousGap.

Time (GUI)
- Time_Mode_Online / Time_Online, Time_Offline.

News
- News_URL, News_Interval, NewsFeed_Enabled, News_LastUpdate (GUI).

AirScout URLs (6 entries)
- AirScout_Register_URL, upload location / QRV URLs, Plugins_Update_URL, GetKeyURL.
- QRZ_URL_Database.

Out of scope
- Server_*, Webserver_Port, WinTest_Port, KST_*, CAT_*.

Lifecycle
- FirstRun, First_*, Version.

Key defaults
- Map_Update 1, Planes_Update 60, Planes_Interval 60.
- Planes_Position_TTL 5 (min), Planes_MaxAlt 12200, Planes_MaxDelta 1000.
- Alarm_Distance 100.
- Path_MaxLength 1000, Watchlist_MaxCount 1000.
- Track_Update 1000, SpecLab_Update 1, News_Interval 3600, Background_Update_Period 60.

Also used by the GUI (other settings classes)
- ScoutBase.Elevation / Stations / Propagation / Maps.
- AirScout.Aircrafts / PlaneFeeds.

---

## 5. Data flow and threading

Threading model
- Single UI thread.
- About 20 BackgroundWorkers. Each communicates only via ReportProgress, which WinForms marshals onto the UI thread.

Live data flow
```
PlaneFeed plugin worker -(ReportProgress PLANES)-> Planes (PlaneInfoCache, UI thread)
ti_Progress (Map_Update s) -> UpdatePlanes -> Planes.GetAll(now, TTL)
   -> per PropagationPath: AircraftData.GetNearestPlanes -> ActivePlanes / NearestPlane
   -> DrawPlanes (markers, alarm, chart points)
UpdatePaths (on Play / watchlist / band change, UI thread, synchronous!)
   -> ElevationPathFindOrCreate + PropagationPathFindOrCreate (heavy; blocks the UI)
```

Background precompute
- PathCalculator workers fill the propagation DB, so UpdatePaths usually hits the cache.
- Background_Calculations_ThreadWait throttles them while the UI computes.

Shared mutable state
- Settings.Default is read from worker threads.
- Planes is mutated on the UI thread. bw_Track reads it via Planes.Get from its worker: a race.
- AllPositions is passed by ref to CrossingHistoryDlg.

Workers that call UI-only APIs
- MapPreloader calls Application.DoEvents.
- StopAllBackgroundWorkers busy-waits with DoEvents.

Cross-worker pushes via the UI
- bw_Track pushes to bw_CAT.
- bw_Track and Spectrum read NearestPlane / Track_CurrentPlane.

Mode guards
- LIFE vs HISTORY decides the Planes source: feeds vs AllPositions.
- Workers are stopped in HISTORY.

Suggested egui mapping
- tokio or std threads plus channels (crossbeam / flume).
- A shared `Arc<RwLock<PlaneCache>>`.
- `ctx.request_repaint_after(Map_Update)` as the tick.
- Move UpdatePaths to a task with a result channel.

Core functions called by the GUI (to expose from the crates)
- StationData:
  - LocationFind / FindAll / FindOrCreate / FindOrUpdateOrCreate / InsertOrUpdateIfNewer / GetAll / FromTXT / DeleteAll / Count.
  - QRVFind / FindOrCreateDefault / GetDefaultAntennaHeight / GetDefaultAntennaGain / GetDefaultPower / InsertOrUpdateIfNewer / Delete.
  - GetDBLocation / Size / StatusBit; IsInMemory / BackupDatabase.
- ElevationData:
  - indexer [lat, lon, model], GetMaxElvLoc, ElevationTileFindMinMaxInfo.
  - ElevationPathFindOrCreateFromLatLon, GetDefaultStepWidth.
  - LocalObstructionFind / FindOrCreateDefault / InsertOrUpdateIfNewer.
  - ElevationCatalogueCreateCheckBoundsAndLastModified, ElevationTileExists, ElevationPathDeleteAll.
  - GetElevationColor, ElvMissingFlag / TileMissingFlag.
- PropagationData:
  - PropagationPathFindOrCreateFromLatLon / Find / CreateFromLatLon / DeleteAll.
  - PropagationHorizonFindOrCreate.
  - NearFieldSuppression.
- PropagationPathDesignator:
  - GetInfoPoints / GetInfoPoint / GetMidPoint.
  - Fields: Valid, Distance, Bearing12 / Bearing21, Eps1_Min / Eps2_Min, h1, h2, Radius.
- Propagation: EpsilonFromHeights, HeightFromEpsilon, SlantRangeFromHeights, DopplerShift.
- AircraftData: GetNearestPlanes, AircraftFindByHex, AircraftTypeFindByICAO, AirportGetAll.
- AircraftPositionData: OldestEntry, YoungestEntry, Count, GetAll, GetAllByHex / ByCall / AllHex / AllCalls, DeleteAll, ClearTable.
- PlaneInfoCache: GetAll, Get, BulkInsertOrUpdateIfNewer, InsertOrUpdateIfNewer, Clear.
- SignalData: SignalLevelBulkInsertOrUpdateIfNewer, SignalLevelFind.
- MapData: TileExists, TileDeleteAll, GetDBLocation / Size.
- Geometry and locators:
  - LatLon: Bearing, Distance, DestinationPoint, MidPoint, Earth.Radius.
  - MaidenheadLocator: LocFromLatLon, Check, IsPrecise, LatLonFromLoc, Convert.
- Others: Callsign.Check, GeographicalPoint.Check, Bands (ToGHz / ToHz), PlaneCategories, UnitConverter, SupportFunctions.GetBeamWidthFromGain / GetCSVSeparator.
- Database maintenance: ScoutBaseDatabase GetTableList / GetTableDescription / GetTableRowCount / ClearTable / GetDBConnection.

---

## 6. Platform-specific / Windows-only items

P/Invoke and OS APIs
- user32 SetForegroundWindow / ShowWindow (MapDlg.cs:3802-3808); kernel32 AttachConsole (Program.cs).
- DeviceId via WMI (processor, motherboard, MAC) in InitializeSession :751. Skipped on Mono.
- Registry ProductId plus WindowsIdentity for map provider acceptance (OptionsDlg Map tab).
- System.Media.SystemSounds.Beep (alarm).
- Process.Start of directories (Explorer) in the Options Misc and Database tabs.
- Graphics.CopyFromScreen screenshot (MapSave :3490).

UI controls with no direct egui equivalent
- WebBrowser (IE) for News (InitializeWebbrowser :2899). Skipped on Mono.
- AquaGauge controls (tracking gauges).
- CustomScrollBar.ScrollBarEx.
- AeroWizard.
- PropertyGrid (plugin settings).
- DataVisualization Chart (CrossingHistory).
- OxyPlot WinForms.
- GMap.NET with its SQLite tile cache.

Out-of-scope integrations (hooks only)
- NDde (HRD DDE) and SerialPort GS-232A in bw_Track.
- OmniRig / CAT via bw_CAT.

Settings and plugins
- Settings via ApplicationSettings user.config. The Mono path uses JSON per class (:2999-3082). Recommend JSON / TOML via as-settings.
- MEF plugin DLL loading for plane feeds (:915-1027). The Rust port needs as-feeds providers instead.

Runtime quirks
- Application.DoEvents busy-waits (:2477, MapPreloader).
- Many `SupportFunctions.IsMono` branches: window maximized on Mono, no web browser, settings JSON, Splash border.

Upstream bugs to not replicate
- PathCalculator: no ASTER name or stepwidth.
- MapPreloader ignores Map_Preloader_Min/MaxZoom.
- CrossingHistory CSV header has no separators.
- CancelAllBackgroundWorkers checks bw_GLOBEPathCalculator but cancels bw_GLOBEUpdater.
- InitializeSettings sets ElevationDatabase_Directory from StationsDatabase_Directory.
- SpecLab URL uses "?f2" where "&f2" is needed.
- Back / Rewind still advance time.
- AircraftDatabaseMaintenanceDlg deletes station tables.
- UpdatePlanes passes UtcNow to GetNearestPlanes even in HISTORY mode.
