# AirScout suite tools survey (stage 5: PlaneServer, ViewClient, generators, WinTestSimulator, CATCheck, RainScout, maps)

Base directory for all refs: `/Users/philipp/Projects/hamradioomline/AirScout-RS/target/upstream/AirScout/`.
Refs use the form `Dir/File.cs:line`. Sizes are `ClientSize` in the `*.Designer.cs` file.
Stages 1 to 4d are done. This survey covers the programs outside the main `AirScout` window, plus the map
preloader that moved here from stage 3b (`docs/logic-findings.md` #100,
`docs/superpowers/plans/2026-11-23-stage3b-dialogs-background.md:42`).

Summary of the shape of stage 5:
- Twelve upstream projects. Only `AirScoutViewClient` is in `AirScout.sln` (`AirScout.sln:50`). The others are
  stand-alone WinForms projects with no solution of their own. Most target .NET Framework 4.0. ViewClient,
  RainScout and its libraries target 4.7.2.
- Every tool is a WinForms `WinExe` (no console tool). Most are developer tools with hard-coded values, a few buttons
  and a status line.
- Two tools are dead upstream. `AirScoutPlaneServer` does not compile against the current tree (section 2). RainScout is
  an unfinished 0.9 preview that scrapes weather web sites (section 9).
- Nearly all the maths is already ported: elevation tiles, horizons, propagation paths, Win-Test messages, CAT,
  the web API. Stage 5 is mostly thin binaries on existing crates. The two exceptions are `as-maps` (new) and RainScout
  (new radar layer and scatter logic).
- One of the projects, `Radars/Radars`, is an orphan: nothing references it (section 9.4).

Rough C# line counts (code without designer / designer):
- `AirScoutPlaneServer` 1622 / 1753; `AirScoutViewClient` 1866 / 1360.
- `HorizonGenerator` 170 / 188; `ElevationCoverageMapper` 341 / 219; `ElevationTileGenerator` 1405 / 744;
  `MapTileGenerator` 209 / 338; `AirScout/MapPreloader.cs` 270; `ScoutBase/ScoutBase.Maps` 377.
- `WinTestSimulator` 100 / 351; `CATCheck` 1563 / 1770, plus 147 OmniRig `.ini` rig files in `CATCheck/Rigs`.
- `RainScout` 3810 / 3248; `RainScout.Core` 1473 / 70; `RainScout.Radars` 1773; `Radars` 131.

Next free logic finding number: **#278**. The suspected bugs in section 11 are labelled S1 to S24. They get numbers
from #278 on when they are logged.

---

## 1. Libraries and what uses them

| Library | What it is | Used by (csproj) | Stage 5 relevance |
|---|---|---|---|
| `OrbitTools` (Orbit, Orbit.Core) | C# SGP4/SDP4 satellite orbit propagator. Its licence is "non-commercial purposes only" (`OrbitTools/readme.txt`). | `AirScout/AirScout.csproj` only. The code has only a dead `using Zeptomoby.OrbitTools;` (`AirScout/MapDlg.cs:41`, `AirScout/Webserver.cs:40`). No type is used. | None. Do not port. The licence is not compatible with GPL-3.0. |
| `AquaGauge` | WinForms round gauge control (azimuth and elevation dials, `AirScout/MapDlg.Designer.cs:41-42`). | `AirScout` only. | None. Stage 3 drew it in egui. |
| `CubicSpline` | Cubic spline interpolation plus a test app. | `AirScout`, `RainScout` (`RainScout/RainScout.csproj`). RainScout has only a `Path_Cubic_Spline` setting and no call. | None. |
| `CustomScrollBar` | Owner-drawn scroll bar (HISTORY player). | `AirScout` only. | None (3c). |
| `SerializableGenerics` | XML-serialisable dictionaries for settings. | `AirScout` only. | None (TOML settings). |
| `AeroWizard` | Wizard control (first-run wizard). | `AirScout` only. | None (3b). |
| `GreatMaps` (GMap.NET) | Map control and tile cache (`.gmdb`, SQLite). | PlaneServer, ViewClient, CoverageMapper, TileGenerator, RainScout, RainScout.Radars. | Replaced by `walkers`; the `.gmdb` cache layout matters for `maptile-gen` (section 6). |
| `OxyPlot` | Charts. | `AirScoutViewClient`. | `egui_plot`, as in stage 3. |
| `DotNetZip` (Ionic.Zip) | Zip. | `ElevationTileGenerator`, `RainScout.Core`, `RainScout.Radars`. | `zip` crate (already in the workspace). |
| `CSJ2K` | JPEG 2000 decoder (NuGet). | `RainScout`, `RainScout.Radars`. No `using` anywhere. | None. |
| `NGrib` | GRIB2 reader (NuGet). | `RainScout.Radars`. Only in `GetTemperaturesFromGRIB`, whose call is commented out (`RainScout.Radars/RadarHD_EU.cs:686`). | None (dead code). |
| `DeviceId` | Hardware fingerprint (NuGet). | `RainScout` (`RainScout/MainDlg.cs:1`, `:274`), for the registration call. | Not ported (project rule: no register). |
| `WinTest` | Win-Test message encoder. | `WinTestSimulator`. | `as-wintest` (4a). |
| `ScoutBase.CAT` | OmniRig-compatible INI rig engine (serial). | `CATCheck`. | Dropped in stage 4 (decision D7). See section 8. |
| `ScoutBase.Maps` | SQLite tile table (`Tiles`, `TilesData`). | `MapTileGenerator`, `AirScout` (preloader). | `as-maps` (section 6). |

---

## 2. AirScoutPlaneServer (`airscout-planeserver`)

### 2.1 Purpose and state
- Purpose: a headless-style plane feed collector. It runs up to three web feeds over an area, keeps the planes in a
  database and serves them as `/planes.json` to other AirScout instances. The client side is the AirScoutServer feed
  plugin (`AirScout.PlaneFeeds.Plugin.AirScoutServer/AirScoutServer.cs:40`, default
  `http://localhost:9880/planes.json`).
- **State: dead upstream.** The project references only ScoutBase.Core, ScoutBase.Elevation, GMap and SQLiteDatabase
  (`AirScoutPlaneServer/AirScoutPlaneServer.csproj`). The code uses namespaces and types that no longer exist in the
  tree:
  - `AirScout.PlaneFeeds.Generic` (`MainDlg.cs:19`) and `AirScout.Database.Flightradar` (`:31`);
  - `PlanefeedSettings_WF` (settings `WebFeed1_Settings` to `WebFeed3_Settings`);
  - `AircraftDatabase_old` (`MainDlg.cs:252`, `:1031-1033`, `DatabaseUpdater.cs:100`).
- It cannot have been built from this tree. `Readme.txt` dates it to 2018 (the Mono notes).
- The JSON writer writes an empty table: `DataTable dt = new DataTable();` with the feed call commented out
  (`JSONWriter.cs:25-26`). So `planes.json` is always `{"full_count":0,"version":N}`.

### 2.2 UI
- `MainDlg` 634 x 452 (`MainDlg.Designer.cs:1129`), title "AirScout Plane Server".
- Tabs: General, Planes (a coverage map with the area), ADS-B Feed, Server, Database (`MainDlg.Designer.cs:304-816`).
- It has a tray icon and a context menu, "start minimised" and "start in tray" options, and an autorun checkbox that
  writes `HKCU\...\CurrentVersion\Run` (`MainDlg.cs:471-495`).

### 2.3 Inputs, outputs, network
- Listening socket: `HttpListener` on `http://+:9872/` (`Webserver.cs:19`), so all interfaces, IPv4 and IPv6.
  `Webserver_Active` defaults to **True** (`Properties/Settings.settings`), so the server is on by default.
- Routes: `/planes.json` serves `Tmp/planes.json` (`Webserver.cs:52-66`). Every other path, and a missing file, gets
  `Webserver_Welcome` (an HTML snippet) with status 200 (`:51`).
- One `HttpListener` per request: it is created, started, serves one request and is stopped (`:37-74`). On "access
  denied" it runs `netsh http add urlacl` with `runas` (`:78-91`), the same as AirScout #259.
- Outbound:
  - the selected web feeds (`InitializeWebFeeds`, `MainDlg.cs:255-295`);
  - every 60 s, five database JSON files from `http://www.airscout.eu/downloads/database/*.json`
    (`DatabaseUpdater.cs:249-260`). They answer **404** (checked 2026-11-24, one HEAD request each);
  - `Database_UpdateURL` `http://www.airscout.de/UpdateURL.txt` also answers 404. Nothing reads it.
- Map tiles for the coverage map through GMap with User-Agent `AirScout` (`MainDlg.cs:319-321`).
- Files: `Tmp/planes.json` (written every 60 s, `JSONWriter.cs:24-31`), logs, user settings XML.

### 2.4 What the Rust port would reuse
- `as-feeds` pipeline and worker (`crates/as-feeds/src/pipeline.rs`, `worker.rs`) for the feed loop over an area.
- The `planes.json` writer from 4d (logic finding #267: temp file and rename, escaping, the C# member layout) and the
  `as-web` server core (bounded pool, timeouts, #272), with a route table of one entry.
- `as-net::user_agent()`, `as-settings` for a small TOML file.
- `as-aircraft` for the type lookup the writer does (`JSONWriter.cs:46-51`).

### 2.5 Recommendation
- Port it as a **headless CLI daemon**, not as a GUI. Upstream has no working code to port 1:1, so the port follows the
  intent: feeds go in and `planes.json` comes out in the format the AirScoutServer plugin reads.
- Drop the tray, the autorun registry key and the coverage map. Document service setup instead (systemd unit, launchd
  plist, Windows Task Scheduler) in the README.
- Drop the 60 s database updater (dead URLs). Use the existing aircraft database update (`as-cli aircraft-update`)
  on explicit action or at start.
- Listening socket: the daemon exists to listen. Bind address and port must be explicit and documented (question Q2).
- Feeds: same defaults and disclaimers as the app. Restricted feeds stay off unless the user turns them on in the
  config file, and the config file is the only place for their credentials. Credentials are never logged.
- Effort: **M** (about one feed worker loop, the writer and the server, plus tests against a fake feed).

---

## 3. AirScoutViewClient (`airscout-viewclient`)

### 3.1 Purpose
A remote viewer for a running AirScout. It reads everything from the AirScout web server API (4d) and computes
nothing itself. It shows a map with the two stations, the path, the nearest planes and an elevation and path chart.
`VersionHistory.txt`: "2018-12-16: V1.3.0.4 Initial Version".

### 3.2 UI
- `MapViewDlg` 836 x 540 (`MapViewDlg.Designer.cs:596`): map (GMap), path chart (OxyPlot), call and locator boxes for
  My and DX, a band box, Play/Pause, zoom buttons and a status strip with a connection light.
- `SettingsDlg` 304 x 83 (`SettingsDlg.Designer.cs:85`): server name and port (default `localhost`, `9880`).

### 3.3 Network behaviour (client only, no listening socket)
- `GetServerURL` builds `http://<server>:<port>/<file>?<params>` (`MapViewDlg.cs:574-580`). Plain HTTP only.
- `GetJSONFromURL` sends a fake browser User-Agent (Firefox 37, `MapViewDlg.cs:597`).
- Routes used (all exist in `crates/as-web/src/route.rs:37-48`):
  - `/settings.json` (`:630`);
  - `/location.json?call=..&loc=all` (`:690-695`) and the single-location variants (`:728-805`);
  - `/elevationpath.json` and `/propagationpath.json` with `mycall, myloc, dxcall, dxloc` (`:831-870`);
  - `/nearestplanes.json` (`:898-904`).
- Polling: `ti_Progress` every 1000 ms (`MapViewDlg.Designer.cs:546`). In CONNECTED and FORWARD mode it calls
  `DrawPlanes`, which fetches `/nearestplanes.json` every second (`MapViewDlg.cs:1142`). The paths are fetched again
  when a call or locator changes (`:1253`, `:1263`, `:1649-1711`).
- Map tiles through GMap with UA `AirScout` and the provider named in the server's `Map_Provider` (`:245-247`).

### 3.4 Interop with the Rust web server
- The C# ViewClient reads these members of `/settings.json` (`MapViewDlg.cs:641-674`):
  - `Path_Band_Settings`, `Map_Provider`, the bounds, the My and DX fields and `Band`;
  - the eleven `InfoWin_*` flags;
  - `Locator_AutoLength`, `Locator_SmallLettersForSubsquares` and `Locator_MaxLength`.
- The curated 4d object (`crates/airscout/src/web_settings_json.rs`, D4) has all of them except the three
  `Locator_*` members. `PropertyFromJSON` throws on a missing member (`:617-622`). `SettingsFromJSON` catches it and
  still returns true (`:676-682`). The original ViewClient against AirScout-RS therefore connects and only loses the
  locator display settings.
- Suggestion: add the three members to the curated object in 5e (small `as-web` change, logged as a port deviation).

### 3.5 What the Rust port would reuse
- `walkers` map, overlays, plane icons and the path chart of the `airscout` crate. That needs those widgets to live in
  the library part of `crates/airscout` or in a new shared UI crate (risk R3).
- `as-web` DTOs (`crates/as-web/src/dto.rs`) for deserialising, with `reqwest` (blocking, already a workspace dep)
  and `as_net::user_agent()`.

### 3.6 Recommendation
- Worth porting only as a low-priority item. The 4d web UI already gives a remote view in any browser, and
  ViewClient was an early 2018 tool.
- If ported: an eframe app with a server and port field, the same routes, an honest UA, a 1 s poll for the nearest
  planes and a backoff when the server is down.
- Effort: **M** if the stage 3 widgets can be shared, **L** if they must be copied.

---

## 4. HorizonGenerator (`horizon-gen`)

- Purpose: developer tool. For **every station in the station database** it computes a 360-bearing horizon and writes
  `Horizon.csv` (`HorizonGenerator/MainDlg.cs:36-100`).
- UI: 617 x 85 (`MainDlg.Designer.cs:78`), only Start, Stop and a status line.
- Hard-coded values (`:45-50`): SRTM1, band 1.2 GHz, antenna 40 m, max elevation 4797 m, 500 km, radius
  `R * 1.33`. The F1 clearance is 0.2 up to 144 MHz, 0.4 up to 432 MHz and 0.6 above (`:66-70`), not the band settings.
- Per bearing: `ElevationPathCreateFromBearing` (`ScoutBase/ScoutBase.Elevation/ElevationDatabase.cs:1365`). Then per
  band: `PropagationHorizonDesignator.EpsilonMaxFromPath`
  (`ScoutBase/ScoutBase.Propagation/PropagationHorizon.cs:153`).
- Output: `Horizon.csv` in the working directory, `;` separated, `F8` degrees (`:78-93`). It is **overwritten for
  every station**, so only the last station survives (S10).
- Network: none (it reads the local elevation and station databases).
- Reuse: `as-propagation::horizon::epsilon_max_from_path` (`crates/as-propagation/src/horizon.rs:117`) and
  `as-elevation::path::create_path_from_bearing` (`crates/as-elevation/src/path.rs:258`) already exist and have golden
  tests. `as-cli horizon` (`crates/as-cli/src/main.rs:58-70`) already prints one station's horizon as JSON.
- Recommendation: port as a CLI with real parameters instead of the hard-coded ones:
  - `--model`, `--height`, `--dist`, `--k`;
  - the bands, with F1 clearance from the band settings, or the C# thresholds with `--legacy-f1`;
  - `--stations all|<call>`, and `--out <dir>` with one CSV per station.
  Keep the C# column header text and `F8`. The 1:1 column shift (S10) is reproduced only under `--legacy-columns`, or
  logged and fixed (question Q5). It could be a `horizon-gen` binary or an `as-cli horizon --all` subcommand (Q6).
- Effort: **S**.

---

## 5. Elevation tools

### 5.1 ElevationCoverageMapper (`elevation-coverage`)
- Purpose: draw on a world map which elevation tiles the update server's catalogue offers for one model.
- UI: 1008 x 729 (`MainDlg.Designer.cs:93`), a GMap map at zoom 2 with one translucent polygon per 6-character tile
  (`MainDlg.cs:160-189`).
- Behaviour:
  - It downloads `http://airscout.eu/downloads/ScoutBase/1/ElevationData/GLOBE/files.zip` into the working
    directory (`:87-103`), after it deletes any old `files.zip` and `files.cat` there.
  - It reads `files.cat` and sends each tile to the UI thread (`:105-152`).
  - The model is hard-coded `"GLOBE"` (`:22`). The "found"/"missing" counters are never incremented (`:145-156`), and
    the red "missing" branch is never sent (S11).
- Network: one catalogue download per start (about 1 file; the server answers 200 as of 2026-11-24).
- Reuse: `as-elevation::Catalogue::fetch` (`crates/as-elevation/src/catalogue.rs:61`) and the 3c coverage job
  (`crates/airscout/src/coverage.rs`), which already shows catalogue against local database per tile in the
  Options Elevation tab.
- Recommendation: the app already covers the use case. A CLI is cheap: `elevation-coverage --model GLOBE
  [--area ..] [--data <dir>]`. It prints counts (in catalogue, in local DB, missing) and writes GeoJSON or a PNG of
  the tiles. It runs only on explicit start (one catalogue download). Effort **S**. Low priority (Q7).

### 5.2 ElevationTileGenerator (`elevation-tile-gen`)
- Purpose: the **producer side** of the elevation server. It turns raw elevation data into the `.loc` tiles,
  `{AB}/{ABCD}.zip` archives and the `files.cat` / `files.zip` catalogue that airscout.eu serves and
  `as-elevation` downloads.
- UI: 1001 x 556 (`MainDlg.Designer.cs:511`). It is a button panel: Start, ZIP, CAT, CAT fm ZIP, TestCAT, Picture,
  Pic fm Loc-Tiles, Pic fm ZIP, Delete, Compare, Show Diff, Coverage (`MainDlg.Designer.cs:103-503`). It has source
  and destination folder settings (`Properties/Settings.settings`).
- Inputs (all local; the tool downloads nothing):
  - SRTM3 or SRTM1 `.hgt` (1201 or 3601 squared, big-endian, `IsSRTM3File`/`IsSRTM1File`, `MainDlg.cs:130-197`);
  - GLOBE tiles `A10G` to `P10G` (4800 or 6000 x 10800, little-endian, `IsGLOBEFile`, `:72-128`).
  - The user selects files in a file dialog (`Scan`, `:524-542`).
- Processing (`ProcessTile`, `:199-522`):
  - SRTM: one 1° x 1° file gives 12 x 24 subsquare tiles of 100 x 50 (SRTM3) or 300 x 150 (SRTM1) samples. The
    letter of the first subsquare depends on whether the longitude is even (`:236`).
  - GLOBE: 1/24° x 2/24° tiles over the 40° or 50° x 90° file, and values `<= -500` become the missing flag (`:420-520`).
  - Min and max elevation and their positions per tile. All-void tiles are skipped. Each tile is JSON (`tile.ToJSON()`)
    in `<DestDir>/<AB>/<CD>/<ABCDxy>.loc`, with the file time set to `LastUpdated` (`:278-293`).
- Post-processing:
  - `ZIP` packs each `<AB>/<CD>` folder into `<AB>/<ABCD>.zip`, then **deletes the `.loc` files and the folder**
    (`:544-589`);
  - `CAT` and `CATFromZIP` write `files.cat` (a comment header with the data credits, then `name;yyyy-MM-dd
    HH:mm:ssZ`) and zip it to `files.zip` (`:729-789`).
- Extras:
  - Compare two `.hgt` files and write `.hgtdiff` (`:902-1180`);
  - render pictures (`DrawElevationBitmap`, `:621-631`, writes `Elevation.tif` to the working directory);
  - SRTM1 to SRTM3 by 3 x 3 averaging (`ConvertSRTM1ToSRTM3File`, `:1263-1300`);
  - `Delete`, which empties a chosen folder (`:856-890`).
- Reuse: `as-elevation` already has the tile model, `from_loc_json`, blobs and the catalogue parser
  (`crates/as-elevation/src/tile.rs:105`, `catalogue.rs`), and `as-elevation::palette` for pictures. The `zip` crate is
  in the workspace.
- Value: this is the spec's fallback path. If airscout.eu disappears, users can build the same tile tree from raw
  GLOBE and SRTM data, host it themselves and point `as-elevation` at it. Copernicus GLO-30 (spec, "Calls made by
  Claude") would be a later input format.
- Recommendation: port as a CLI with subcommands:
  - `tiles` (SRTM1, SRTM3, GLOBE to `.loc`);
  - `zip` (without the destructive delete unless `--remove-loc`);
  - `catalogue`, `compare`, `downsample` (SRTM1 to SRTM3, with S13 decided), `preview` (PNG).
  Drop `Delete` and the hard-coded picture area.
- Golden test: a small synthetic `.hgt` and a GLOBE slice go through C# and Rust, and the `.loc` JSON is compared
  (the stage 1 harness has ScoutBase.Elevation).
- Effort: **M**.

---

## 6. Maps: MapTileGenerator, the map preloader and `as-maps`

### 6.1 ScoutBase.Maps (`ScoutBase/ScoutBase.Maps/MapDatabase.cs`)
- An SQLite tile store with two tables:
  - `Tiles(id, X, Y, Zoom, Type, CacheTime)` (`:110`);
  - `TilesData(id REFERENCES Tiles ON DELETE CASCADE, Tile BLOB)` (`:124`).
  This is the GMap.NET `.gmdb` schema, where `Type` is the GMap provider `DbId`.
- API: `TileExists`, `TileFind`, `TileDataFind`, `TileInsert` (`MAX(id)+1`, `:222-253`), `TilesGetAll`, delete.
- Setting `Database_UpdateURL` = `http://www.airscout.eu/downloads/ScoutBase/1/MapData/OpenStreetMaps/`
  (`Properties/Settings.settings`).

### 6.2 MapTileGenerator (`maptile-gen`)
- Purpose: export every tile of a GMap `.gmdb` cache to a static `z/x/y.png` folder tree
  (`MapTileGenerator/MainDlg.cs:52-88`). The file times are set to the tile's cache time. In effect this is how the
  airscout.eu `MapData/OpenStreetMaps/` tree was built from the author's GMap cache of OSM tiles.
- A second button joins all PNGs of a folder side by side into `zoom.png` (`CombineImages`, `:94-142`), a debug aid.
- UI: 666 x 545 (`MainDlg.Designer.cs:182`): database file, root folder, Start, Create PNG and a picture box.
- Network: none.
- Bugs: S17.

### 6.3 Map preloader (`AirScout/MapPreloader.cs`, started at `AirScout/MapDlg.cs:2288`, `:2393`)
- `Map_Preloader_Enabled` defaults to False (`AirScout/Properties/Settings.settings`).
- When enabled, it lists every tile for zoom 5 to 11 over the whole area (`:134-170`), shuffles the list (`:174-184`)
  and downloads every tile not in `MapData.Database`, with 100 ms between tiles (`:214`).
  - For the OSM provider it downloads from the airscout.eu mirror (`LoadOSM`, `:78-104`) and puts the tile into the
    GMap cache queue (`:92`).
  - For any **other provider** it calls `GetImageFrom` on that provider's own server (`:202`). That is a bulk pull
    from OpenTopoMap or Thunderforest.
  - In RUNPERIODICALLY mode it repeats every `Background_Update_Period` (`:235-245`).
- Size: for the default area (35°N to 60°N, 15°W to 30°E) zoom 11 alone is about 260 x 260 tiles. Zoom 5 to 11
  together are on the order of 90,000 tiles per run.
- The airscout.eu mirror still answers (HEAD 200 for a zoom 5 and a zoom 11 tile, 2026-11-24). The age and the licence
  notice of its tiles are unknown. They are OSM-derived, so ODbL attribution applies.
- Bugs: S18 (the existence check reads another database than the one written, so each run downloads again).

### 6.4 Current Rust state
- The app uses `walkers` 0.66 with its HTTP disk cache in `<data directory>/tiles` (`crates/airscout/src/tiles.rs:76-90`).
  The cache sends the honest UA, honours the HTTP cache headers and loads tiles only while the user browses.
- `walkers` stores tiles through `http-cache-reqwest` (a content-addressed `cacache` store, not `z/x/y`), so other
  tools cannot read or prefill it directly (risk R4).
- `as-maps` is listed in the spec (`docs/superpowers/specs/2026-11-23-airscout-rs-design.md:74`) and does not exist yet.

### 6.5 Recommendation
- `as-maps`: a small crate with an SQLite tile store, with an import of the GMap `.gmdb` schema so existing C#
  caches can be reused. It provides a `walkers` tile source that reads the store first and falls back to the normal
  HTTP loader, which writes into the store. Effort **M**.
- `maptile-gen`: offline only.
  - Import: GMap `.gmdb`, a `z/x/y` folder or MBTiles go into the `as-maps` store.
  - Export: the store goes to `z/x/y` or MBTiles.
  - It makes no network calls. Effort **S** on top of `as-maps`.
- Preloader: **do not port a network preloader** (project rule; logic finding #100). Offline users import a tile
  set with `maptile-gen`. Question Q3 asks whether a bounded, explicit-action "download this area" from the
  airscout.eu mirror is wanted. It would never use `tile.openstreetmap.org` and would have a tile cap, a rate limit
  and the honest UA.
- No server (as-web, PlaneServer) serves or proxies tiles; that is already the case and stays so.

---

## 7. WinTestSimulator (`wintest-sim`)

- Purpose: a developer tool. It sends one Win-Test message to the broadcast address to test AirScout's receiver.
- UI: 774 x 146 (`MainDlg.Designer.cs:159`), title "WinTest Message Simulator". Fields Message, From, To, Port and
  Data, and a Send button.
- Defaults: Message `ASADDWATCH`, From `Test`, To `AS`, Port 9872. This matches AirScout's `Server_Name` default `AS`
  and `Server_Port` 9872.
- Code (`MainDlg.cs:20-41`):
  - `Enum.TryParse` of the message name (an unknown name silently sends `NONE`, S19);
  - `HasChecksum = true`;
  - a `UdpClient` with `SO_REUSEADDR` connects to `255.255.255.255:Port` and sends once. It does not set
    `EnableBroadcast`, like L5 of the stage 4 survey.
- Network: one outbound UDP broadcast datagram per Send. No listening socket.
- Reuse: `as-wintest::{WtKind::parse, encode}` (`crates/as-wintest/src/lib.rs:64`, `:112`) and the 4a send code with
  `set_broadcast(true)`.
- Recommendation: port as a CLI:
  - `wintest-sim send --msg ASSETPATH --from .. --to AS --data ".." [--port 9872] [--addr 255.255.255.255]`.
  - An unknown name is an error, not `NONE`.
  - An optional `--listen <secs>` prints the replies (for example `ASNEAREST`). It opens a receive socket only for
    that run.
- Useful to test the 4a receiver by hand on all three OS. Effort **S**.

---

## 8. CATCheck (`catcheck`)

- Purpose: a test bench for the ScoutBase.CAT rig engine. That engine is the OmniRig-compatible INI-file engine over
  a serial port; it is not the OmniRig COM server.
- UI: `MainDlg` 1008 x 561 (`MainDlg.Designer.cs:1200`), title "CAT- Checker Vxxx (c) 2012" plus the author's call.
  - Rig type, serial port, baud, data bits, parity, stop bits, RTS and DTR, poll and timeout settings.
  - Clickable labels for frequency, VFO A and B, mode, TX/RX, split, RIT and XIT, VFO copy and swap.
  - A colour-coded log list.
  - `SetFrequencyDlg` 297 x 53, `FrequencyOfToneDlg` 390 x 90, `AboutDlg` 295 x 157.
  - "Show log" starts `explorer.exe` on the log folder (`MainDlg.cs:676-686`).
- Inputs: the 147 OmniRig `.ini` rig definitions shipped in `CATCheck/Rigs` (`CATCheck.csproj:110-…`) and the serial
  port.
- Network: none.
- State in the port: stage 4 dropped the ScoutBase.CAT INI engine and its rig-definition download (D7). So a 1:1
  CATCheck has nothing to test.
- Reuse: the `as-cat::Rig` trait (`crates/as-cat/src/rig.rs:8-24`) with the rigctld, hamlib (feature) and OmniRig
  (`cfg(windows)`) backends from 4b and 4c.
- Recommendation: port the intent as a CLI `catcheck` over `as-cat`:
  - `status` (poll loop printing RX/TX frequency, mode, split, RIT);
  - `set-freq`, `set-mode`, `split on|off`, `rit ..`;
  - `doppler-test` (enter Doppler, step, leave and restore, as the CAT worker does).
  - The engine and connection come from the same settings as the app, or from flags. `--list-ports` uses the serial
    crate.
  - No INI engine and no shipped `.ini` files.
- Effort: **S** (CLI) or **M** with a small egui panel. Recommend the CLI.

---

## 9. RainScout (`rainscout`, `rainscout-core`, `rainscout-radars`)

### 9.1 Purpose and state
- Purpose: rain scatter planning. The user clicks a scatter point (SCP) on the map. RainScout reads the radar echo
  intensity, cloud tops and lightning at that point from weather radar images. It then marks every station that can
  see the scatter volume, with their horizons from the station and elevation databases.
- State: an unfinished preview.
  - `AssemblyVersion("0.9.0.0")`.
  - Debug files are written to the working directory (S23), and a dead GRIB path is hard-coded to a Windows drive (S23).
  - It has its own copies of `LatLon` (`RainScout/LatLon.cs`, namespace `SphericalEarth`, R = 6371 km) and
    `BandSettings` (`RainScout/BandSettings.cs`).
  - It has an unused callsign prefix checker (`RainScout/WCCheck.cs`, R = 6378.2 km).

### 9.2 UI
- `MainDlg` 1008 x 729 (`RainScout/MainDlg.Designer.cs:1439`): a GMap map with the radar overlay (intensity, cloud
  tops, lightning, horizons, distances, bounds checkboxes), the SCP and My data boxes, band, elevation model and
  radar boxes, and status lines for database, locations and radar.
- `OptionsDlg` 751 x 442 (`RainScout/OptionsDlg.Designer.cs:644`).
- `RainScout.Core/ColorDlg` (colour picker).

### 9.3 Logic
- Workers:
  - `bw_StationDatabaseUpdater` and the GLOBE, SRTM3 and SRTM1 updaters (`MainDlg.cs:333-397`);
  - `bw_Horizons`, which computes the own horizon and then every station's (`:1449-1600`);
  - `bw_Locations` and `bw_Stations` (`:1634-1914`);
  - `bw_Radar`, which polls the current radar every 60 s into a 2048 x 2048 `RadarMap` (`:1745-1805`,
    `RainScout/RadarMap.cs`).
- `GetScpPotential` (`MainDlg.cs:1057-1230`):
  - `DistanceFromEpsilon(h_station, H, eps_horizon[bearing], R*k)` is compared with the distance to the SCP. `H` is
    the cloud top if known, else `Scatter_MinHeight` (8000 m) and `Scatter_MaxHeight` (20000 m).
  - The result is `CLOUDTOP`, `MINCLOUD`, `MAXCLOUD`, `NONELOC` or `NONEUSER`.
  - Without a horizon it builds one propagation path from the bearing instead. Bugs: S20.
- `UpdateDXLocation` draws visibility rings with its own `GetVisibleDist` (`:839-930`).
- Reuse: `as-geo` (`distance_from_epsilon` has golden tests, `crates/as-geo/tests/propagation_golden.rs:35`),
  `as-propagation` horizons and paths from bearing, `as-elevation`, `as-stations`, the stage 3 map and overlays. The
  scatter core is small (about 300 lines of C#).

### 9.4 Radar sources (`RainScout.Radars`; the orphan `Radars/Radars` is an older copy of `Generic.cs` and `RadarEU_CZ.cs`)

| Radar | Source (code) | How | Status 2026-11-24 (one HEAD, honest UA) |
|---|---|---|---|
| `Radar3D DE` (`Radar3D_DE.cs:41-50`, `:395-420`) | kachelmannwetter.com radar and echo-top pages | HTML scrape for the image URL, then a download with a **fake Chrome UA** and copied cookies (`:173-205`) | 403 |
| `RadarEU CZ` (`RadarEU_CZ.cs:24`, `:79-121`) | `http://www.radareu.cz/data/radar/radar.anim.<date>.<time>.0.png` | Direct image by time stamp | 301 (moved) |
| `RadarHD EU` (`RadarHD_EU.cs:53-62`, `:146-240`) | RainViewer tile cache `tilecache.rainviewer.com/v2/radar/<t>/256/5/x/y/6/1_1.png` and `/v2/coverage/…`, zoom 5, x 14 to 20, y 6 to 13 | Tiles | coverage tile 200; terms and current API to verify |
| (cloud tops in `RadarHD EU`) (`:296-420`) | weather.us satellite page | HTML scrape plus fake Chrome UA (`:330`) | 403 |
| Lightning (all three, `RadarEU_CZ.cs:166-209`, `RadarHD_EU.cs:540-580`, `Radar3D_DE.cs:351-390`) | `map.blitzortung.org/GEOjson/getjson.php?f=s&n=00..23` | 24 requests per poll with a **spoofed `Referer: https://map.blitzortung.org/`** | 200 |
| Temperatures (dead) (`RadarHD_EU.cs:588-665`) | `opendata.dwd.de` ICON-EU GRIB2 | Directory listing plus GRIB; call commented out | open data |

- Load: `RadarHD EU` asks for all images on each of the three layer calls (S21). Per 60 s poll that is about
  3 x (56 radar tiles + 56 coverage tiles + 1 page + 1 image + 24 lightning requests), roughly 400 requests a minute.
- Registration: at start RainScout sends the instance id, own call, own position and version to
  `http://www.airscout.eu/usr/register.php` (`MainDlg.cs:262-305`). Its instance id is reset when the `DeviceId`
  hardware fingerprint changes (`:274-283`), and it decrypts a session key from the answer. `getkey.php` is passed to
  the station updater (`:348`). Upload URLs are in the settings. **Not ported** (project rule).
  The station data is not encrypted (logic findings, data sources table), so no key is needed.

### 9.5 Recommendation
- Port the scatter core (`rainscout-core`): the SCP potential with S20 decided, the visibility rings and the radar
  grid model (value colour tables from `RainScout.Core/ValueColorTable.cs`).
- Port `rainscout-radars` with sources that work with honest headers and documented terms:
  - RainViewer through its public API (frames from the documented index, not a guessed time stamp), with
    attribution, a per-run tile budget and one download per layer set;
  - possibly DWD open data radar composites as a later, clean source for Germany.
- Do not port the sources that only work with a spoofed browser UA or a spoofed Referer (kachelmannwetter,
  weather.us, blitzortung). The honest UA rule makes them fail anyway (403), and blitzortung's data terms do not
  allow third-party apps. If the owner wants them, they are restricted sources: off by default with a disclaimer
  (Q8).
- Drop registration, `DeviceId`, WCCheck, the duplicated `LatLon` (use `as-geo`), CSJ2K, NGrib and the debug files.
- Effort: **L** (new UI, new radar layer, external API work). Lowest priority of stage 5 (Q9).

---

## 10. Listening sockets, outbound hosts and platform parts

### 10.1 Sockets and outbound hosts per tool

| Tool | Listens | Outbound | Bulk |
|---|---|---|---|
| PlaneServer | HTTP `+:9872` all interfaces, **on by default** (`Webserver.cs:19`, `Webserver_Active` True) | feeds; airscout.eu database JSON every 60 s (404); map tiles | no (but polls every 60 s) |
| ViewClient | none | AirScout web server (plain HTTP, fake UA); map tiles | no |
| HorizonGenerator | none | none | no |
| ElevationCoverageMapper | none | airscout.eu `files.zip` once | no |
| ElevationTileGenerator | none | none | no |
| MapTileGenerator | none | none | no |
| Map preloader | none | airscout.eu tile mirror, or other providers directly | **yes**: about 90k tiles per run |
| WinTestSimulator | none | UDP broadcast 255.255.255.255:9872 | no |
| CATCheck | none | none (serial) | no |
| RainScout | none | register.php; radar and lightning sites (scrape, spoofed headers); airscout.eu stations and elevation | about 400 requests a minute |

Proposed Rust defaults:
- No tool listens unless it is started for that purpose.
- `airscout-planeserver` binds only to the address given in its config or on its command line (Q2). The default bind
  and port are documented in the README next to the 4d web server.
- `wintest-sim --listen` opens a receive socket only for the given seconds.

### 10.2 Credentials
- PlaneServer feed credentials (`WebFeedN_Settings.URL`, VRS user and password) come only from the user's config
  file. They are never logged. Upstream prints feed URLs to the console (`MainDlg.cs:189-199`); the port does not.
- RainScout: no keys in code. The register and session-key flow is not ported.
- No private LAN addresses appear in the stage 5 projects that were read. Any example in docs uses `localhost`.

### 10.3 Platform-specific parts
- Autorun via `HKCU\...\Run` (PlaneServer `MainDlg.cs:471-495`): document systemd user units (Linux), launchd
  agents (macOS) and Task Scheduler (Windows) instead. No registry code.
- `netsh http add urlacl` (PlaneServer `Webserver.cs:78-91`): not needed with a plain TCP socket (as #259).
- Tray icon (PlaneServer): not ported (headless).
- `explorer.exe` (CATCheck `MainDlg.cs:685`): print the log path instead.
- OmniRig in `catcheck`: `cfg(windows)` through `as-cat`. rigctld and hamlib work on all OS.
- Serial port enumeration (CATCheck `:395`): the `serial2` crate already in the workspace.
- WinForms GDI bitmaps (RainScout radar maps, TileGenerator pictures): the `image` crate (PNG) and egui textures.

---

## 11. Suspected bugs (to be logged as logic findings from #278, not fixed)

S1. `AirScoutPlaneServer/JSONWriter.cs:25-26`; `MainDlg.cs:19`, `:31`, `:252`: the plane query is commented out, so
`planes.json` always has `full_count` 0. The project references types that are not in the tree
(`AirScout.PlaneFeeds.Generic`, `AirScout.Database.Flightradar`, `PlanefeedSettings_WF`, `AircraftDatabase_old`), so it
does not build.
S2. `AirScoutPlaneServer/DatabaseUpdater.cs:70-78` (and `:108-116`, `:144-152`, `:180-188`, `:216-224`): the file is
downloaded, but `JsonConvert.DeserializeObject(json)` parses the empty local string `json`, not the file. Every import
fails, and the loop repeats every 60 s (`:249-260`) against URLs that now answer 404.
S3. `AirScoutPlaneServer/MainDlg.cs:244-245`: `cb_WebFeed2` and `cb_WebFeed3` take their index from
`cb_WebFeed1.FindStringExact`. This is harmless while the three lists are equal.
S4. `AirScoutPlaneServer/Webserver.cs:19`, `:37-74`, `:51`:
- a new `HttpListener` per request, started and stopped each time (requests in between are refused);
- it listens on all interfaces and is on by default;
- every unknown path, and a missing `planes.json`, answers 200 with the welcome HTML.
S5. `AirScoutPlaneServer/Properties/Settings.settings` (`Webserver_Port` 9872) against
`AirScout.PlaneFeeds.Plugin.AirScoutServer/AirScoutServer.cs:40` (default `http://localhost:9880/planes.json`): the
plugin's default points at AirScout's own web server port, not at the PlaneServer.
S6. `AirScoutViewClient/MapViewDlg.cs:658-660`: the comment says "set band only if NONE", but the test is `!=`. A
client with no band never takes the server band, and a client with a band is overwritten on every connect.
S7. `AirScoutViewClient/MapViewDlg.cs:617-622`, `:676-682`: `PropertyFromJSON` throws on a missing member, and
`SettingsFromJSON` catches it and still returns true. The client goes to CONNECTED with partly copied settings
(see 3.4 for the effect against the curated 4d object).
S8. `AirScoutViewClient/MapViewDlg.cs:597`, `:690-695`, `:831-904`: fake Firefox User-Agent, and call signs and
locators go into the query without URL encoding.
S9. `AirScoutViewClient/MapViewDlg.cs:1431-1441`, `:238-273`, `:1735-1742`: after Settings OK, `Restart` runs
`InitializeSettings` again, which adds the map overlays and the band items a second time.
S10. `HorizonGenerator/MainDlg.cs:64-73`, `:78-93`:
- the band loop iterates every `BAND` value including `BNONE` (0) and `BALL` (999999999). That gives 14 columns under
  a 12-band header, so every column is shifted by one (`eps_50M` holds the 0 Hz result);
- `Horizon.csv` is overwritten for each station, so only the last station is kept;
- exceptions are swallowed (`:95-98`).
S11. `ElevationCoverageMapper/MainDlg.cs:22`, `:87-103`, `:145-156`: the model is hard-coded GLOBE; `files.zip` and
`files.cat` are deleted and written in the working directory; `found` and `missing` are never counted (the
summary is always "0 tile(s) found, 0 more …"); the "missing" (red) branch is never sent.
S12. `ElevationTileGenerator/MainDlg.cs:87-91`: `IsGLOBEFile` reads `upperfilename[3]` before the length check, so
selecting a file with a name shorter than 4 characters throws instead of being skipped.
S13. `ElevationTileGenerator/MainDlg.cs:1270-1290` (`ConvertSRTM1ToSRTM3File`): the 3 x 3 sum is a `short`, which
overflows for elevations above about 3640 m, and the void value -32768 is averaged into its neighbours.
S14. `ElevationTileGenerator/MainDlg.cs:856-875`: `ScanDirDelete` recurses with `ScanDirZIP` (`:862`) instead of
itself. `Delete` empties the chosen folder without a confirmation (`:877-885`).
S15. `ElevationTileGenerator/MainDlg.cs:251` against `:460`: the SRTM rows are read from `rows - 2 - …`, the GLOBE rows
from `rows - 1 - …`. This is a suspected one-sample (about 90 m or 30 m) north shift of SRTM tiles. Check it against
a served `.loc` tile before logging.
S16. `ElevationTileGenerator/MainDlg.cs:544-589`: `ZIP` deletes the `.loc` files and the folder once `Save` returns,
without checking the archive. The comment says "sleep 5s", the code sleeps 1 s (`:573-574`).
S17. `MapTileGenerator/MainDlg.cs:39`, `:69`, `:101`:
- the database file path is stored in `ScoutBase.Maps … Database_UpdateURL`, the tile download URL;
- `d != null && !File.Exists(f) || time != CacheTime` has the wrong precedence, so an existing file with another
  time and no data throws a null reference;
- `Path.Combine(Application.ExecutablePath, "zoom.png")` puts the output under the exe file name, so the write fails.
S18. `AirScout/MapPreloader.cs:122`, `:159`, `:82`, `:92`, `:202`:
- the "already present" check reads `MapData.Database` (ScoutBase.Maps), but `LoadOSM` writes into the GMap cache, so
  every run downloads all tiles again;
- the preloader only runs when an elevation model is enabled, which is unrelated;
- the temp file is named after `y` only;
- for non-OSM providers it bulk-downloads from the provider's own server.
S19. `WinTestSimulator/MainDlg.cs:25`, `:35`: an unknown message name silently sends `NONE`. The broadcast is sent
without `EnableBroadcast` (as L5 of the stage 4 survey).
S20. `RainScout/MainDlg.cs:1149`, `:1196`, `:1087-1112`:
- in the "no horizon" cloud-top branch, the path is built but the code reads `le.Horizon.Horizon[...]` (null), so the
  result is always NONE (the exception is caught);
- the `pathmax` path is built with `Scatter_MinHeight` instead of `Scatter_MaxHeight`;
- `(int)dxscpqtf` truncates the bearing (as #8).
S21. `RainScout.Radars/RadarHD_EU.cs:697-704`: `GetRadarLayer` calls `GetRadarImage` on every call, because the
time-stamp check is commented out. `bw_Radar` asks for three layers (`RainScout/MainDlg.cs:1776-1790`), so each poll
downloads everything three times.
S22. `RainScout.Radars/RadarHD_EU.cs:186`, `:221-226`: the coverage tile URL has an empty path segment
(`z + "/" + "/" + x`). The bitmap is `(right-left)*256` by `(bottom-top)*256`, but the loops include `right` and
`bottom`, so the last column and row are cut off.
S23. `RainScout.Radars/RadarEU_CZ.cs:111`, `:281`; `RadarHD_EU.cs:418`, `:646`, `:685-687`, `:791`: debug files
(`RadarEU_CZ.png`, `Intensity_EU.png`, `CloudTops_EU.png`, `LightningValues.csv`) are written to the working
directory on every poll, and a dead GRIB path is hard-coded to a local Windows drive.
S24. `RainScout/MainDlg.cs:262-305`: registration sends own call and position with a hardware-bound instance id over
plain HTTP. The error message box says "AirScout" (`:303`). This is not ported; it is logged as "not reproduced".

---

## 12. Proposed split of stage 5

All sub-stages end with green CI on three OS. Each binary gets a README section with its sockets and network use.

**5a: Developer CLIs on existing crates.**
- `wintest-sim` (section 7), `catcheck` (section 8, rigctld path tested against the 4b fake rigctld) and
  `horizon-gen` (section 4, or `as-cli` subcommands, Q6).
- Rationale: small, no new crates, no new network behaviour; they help to test 4a and 4b by hand.

**5b: Elevation tools.**
- `elevation-tile-gen` (section 5.2) with golden `.loc` vectors from the C# harness, and `elevation-coverage`
  (section 5.1) if wanted (Q7).
- Rationale: offline (except one catalogue download in the coverage tool). It secures the fallback if airscout.eu goes
  away.

**5c: `as-maps` and `maptile-gen`.**
- SQLite tile store with GMap `.gmdb` import, a `walkers` source that reads the store first, and the offline
  import/export CLI (section 6.5). The app gets "Import tiles…" in the Map options.
- No network preloader unless Q3 says otherwise.
- Rationale: the only new library of stage 5; needs care with the `walkers` cache (R4).

**5d: `airscout-planeserver`.**
- Headless feed collector and `/planes.json` server (section 2.5), service-setup docs and a socket section in the
  README.
- Rationale: reuses `as-feeds` and the 4d server; it adds a listening service, so it gets its own review.

**5e: `airscout-viewclient` (optional, Q4).**
- eframe client of the 4d API, plus the three `Locator_*` members in `/settings.json` (3.4).
- Rationale: needs shared UI widgets, possibly a refactor of `crates/airscout` into lib widgets (R3).

**5f: RainScout (optional, Q9).**
- `rainscout-core` (scatter potential, rings), `rainscout-radars` (RainViewer API first), `rainscout` eframe app.
- Rationale: the largest and least finished tool, with external services whose terms must be checked first.

### Risks
- R1. Upstream code that never ran (PlaneServer, parts of RainScout): no golden data, so "1:1" means "same intent and
  formats". Each deviation is logged.
- R2. External services change: RainViewer tile API and terms, radareu.cz moved (301), airscout.eu availability. Every
  network source gets a recorded fixture and an ignored live test, as the stage 2 feeds have.
- R3. ViewClient and RainScout need the stage 3 map and chart widgets outside the `airscout` binary. A shared crate
  (for example `as-ui`) may be needed. That is a refactor of done work.
- R4. `walkers` keeps its own HTTP cache format. `as-maps` must plug in as a tile source without breaking the
  existing cache, cache headers or attribution.
- R5. The PlaneServer is a listening service on a home network. The 4d rules (bounded pool, timeouts, no file paths
  from the URL) must carry over.

### Open decisions (owner)
Q1. PlaneServer scope.
- Options: (a) headless CLI daemon (feeds in, `planes.json` out); (b) a GUI as upstream (tabs, tray, autorun);
  (c) drop, since upstream is dead.
- Recommendation: (a). It is the only form that runs on a headless Raspberry Pi or server, which was the upstream
  intent (`Readme.txt`, Mono on Linux). The GUI extras are Windows-shaped and add little.

Q2. PlaneServer bind address and port defaults.
- Options: (a) `0.0.0.0:9872` as upstream; (b) `127.0.0.1:9872` by default with a documented `--bind 0.0.0.0` for LAN
  use; (c) no default, the address must be given.
- Recommendation: (b). The daemon is opt-in by being started, and the loopback default still keeps it off the LAN
  until the user asks. Keep 9872 for compatibility, and document that the AirScoutServer plugin's default URL uses 9880
  (S5).

Q3. Map preloader.
- Options: (a) none, offline tiles only by import (`maptile-gen`); (b) an explicit "download this area" action
  limited to the airscout.eu mirror, with a cap (for example 5,000 tiles), a rate limit and the honest UA, never
  `tile.openstreetmap.org` and never other providers; (c) port upstream (zoom 5 to 11, periodic).
- Recommendation: (a). Option (b) is possible later if the mirror's operator agrees, but its tile age and licence
  notice are unknown. Option (c) breaks the project rules.

Q4. ViewClient.
- Options: (a) port as an eframe app in 5e; (b) drop and point users to the 4d web UI; (c) defer until after stage 6.
- Recommendation: (c). The web UI covers remote viewing, and the port would first need shared UI widgets (R3). Add the
  three `Locator_*` members to `/settings.json` anyway, so the original C# ViewClient works fully with AirScout-RS.

Q5. HorizonGenerator column shift (S10).
- Options: (a) reproduce 1:1; (b) write the correct band columns and log the deviation; (c) both, with a
  `--legacy-columns` flag.
- Recommendation: (b). The tool is a developer tool with no known consumers of its CSV, and the 1:1 output
  mislabels every column.

Q6. Where the small CLIs live.
- Options: (a) separate binaries as listed in the spec (`horizon-gen`, `wintest-sim`, `catcheck`,
  `elevation-tile-gen`, `elevation-coverage`, `maptile-gen`); (b) subcommands of `as-cli`.
- Recommendation: (a) for `elevation-tile-gen`, `maptile-gen` and `catcheck`, which have their own dependency sets,
  and (b) for `horizon-gen`, `elevation-coverage` and `wintest-sim`, which are a few dozen lines on crates `as-cli`
  already links. This changes the spec's binary list, so it needs a yes.

Q7. ElevationCoverageMapper.
- Options: (a) small CLI (counts plus GeoJSON or PNG); (b) drop, since 3c shows coverage in the app.
- Recommendation: (a) only as an `as-cli` subcommand in 5b; otherwise (b).

Q8. RainScout radar sources.
- Options: (a) only sources with honest headers and documented terms (RainViewer public API, later DWD open data);
  (b) also the scraped sources (kachelmannwetter, weather.us, blitzortung), off by default with a disclaimer;
  (c) all as upstream.
- Recommendation: (a). The scraped sources need a spoofed browser UA or Referer, which the honest UA rule forbids,
  and they already answer 403. Blitzortung's terms do not allow third-party apps.

Q9. RainScout timing.
- Options: (a) 5f as the last sub-stage; (b) after stage 6; (c) drop.
- Recommendation: (b). It is an unfinished 0.9 tool with new external services and a new UI, and it would profit from
  the stage 6 propagation fixes (S20 sits on top of the same horizon maths).

Q10. CATCheck form.
- Options: (a) CLI over `as-cat` (all backends); (b) egui panel; (c) drop.
- Recommendation: (a). The upstream engine it tested was dropped (D7), and a CLI is enough to check a rigctld,
  hamlib or OmniRig setup.

## Decisions (project owner, 2026-11-24)

- **Q1 PlaneServer:** port the intended function (feeds in, `/planes.json` out in the format the AirScoutServer feed
  reads). **One binary, two modes:** `airscout-planeserver --headless` (daemon) or with an egui window and tray icon.
  No autorun registry key; the README documents service setup.
- **Q2 PlaneServer bind:** `0.0.0.0:9872` and **on by default, as upstream** (`Webserver_Active = True`). The README
  warns that the port is open to the whole LAN without authentication. The 4d server rules (bounded pool, timeouts,
  connection cap, no file paths from the URL, no credentials served) apply.
- **Q3 Map preloader:** **1:1 for the airscout.eu mirror only** (OSM provider), **off by default** as upstream
  (`Map_Preloader_Enabled = False`). No bulk pulls from any other provider's server and never from
  `tile.openstreetmap.org`. Honest User-Agent. S18 is logged as a finding (1:1 or fixed is decided in the plan).
- **Q4 ViewClient:** after stage 6. The three `Locator_*` members go into `/settings.json` now (5a or the first
  sub-stage that touches `as-web`).
- **Q5 HorizonGenerator columns:** write the correct columns and log the deviation (S10).
- **Q6 Tool layout:** as proposed — separate binaries `elevation-tile-gen`, `maptile-gen`, `catcheck`; `as-cli`
  subcommands `horizon-gen`, `elevation-coverage`, `wintest-sim`. The spec's binary list changes accordingly.
- **Q7 ElevationCoverageMapper:** `as-cli` subcommand in 5b.
- **Q8 RainScout radar sources:** **all as upstream, including the faked browser User-Agent / Referer** for the
  scraped sources. This is an owner-approved exception to the honest User-Agent rule, limited to these RainScout
  sources and documented in the README. Terms of those sites are the user's responsibility; the README says so.
- **Q9 RainScout:** after stage 6.
- **Q10 CATCheck:** command-line tool over `as-cat`.

Resulting order: 5a (wintest-sim, catcheck, horizon-gen, `Locator_*` members), 5b (elevation-tile-gen,
elevation-coverage), 5c (`as-maps`, maptile-gen, mirror preloader), 5d (airscout-planeserver). ViewClient and RainScout
follow stage 6.
