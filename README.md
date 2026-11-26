# AirScout-RS

A Rust rewrite of [AirScout](https://github.com/dl2alf/AirScout) by DL2ALF, the aircraft
scatter prediction tool for radio amateurs, for macOS, Linux and Windows.

The port follows the original 1:1, including its known bugs. Every suspected bug is
listed in [`docs/logic-findings.md`](docs/logic-findings.md) with what the port does about
it. The prediction logic will be improved in a later stage.

## Status

| Stage | Content | State |
|-------|---------|-------|
| 1 | Geodesy, elevation data, propagation path and horizon, settings | done |
| 2 | Aircraft database, plane cache, crossing prediction, ADS-B decoder, plane feeds, signal levels | done |
| 3a | Desktop app: live map, path profile, plane list, nearest plane | done |
| 3b | Desktop app: dialogs, multi-path, station and watchlist handling | done |
| 3c | Desktop app: HISTORY mode and replay, analysis dialogs, database maintenance, coverage maps, `--clean`, tracking display | done |
| 4a | Win-Test UDP (receiver, ASNEAREST), track outputs (UDP Win-Test/AirScout, azel.dat), Network and Track tabs | done |
| 4b | CAT over rigctld (optional native hamlib), Doppler push, CAT field and tab | done |
| 4c | GS-232A rotator, HRD DDE (Windows), OmniRig CAT (Windows), tracking on the Track worker | done; the hosted Windows CI job (`windows-latest`) runs the Windows-only tests; not tested with a real OmniRig, HRD or GS-232 device |
| 4d | Web server (as-web): embedded web UI and JSON API | done; not tested in a browser by CI |
| 5a | Developer tools: airscout-cli wintest-sim and horizon-gen, catcheck | done; catcheck not tested with a real rig |
| 5b | Elevation tools: elevation-tile-gen, airscout-cli elevation-coverage | done; not tested with real SRTM or GLOBE files |
| 5c | Offline maps: the `as-maps` tile store, maptile-gen, the map preloader for the airscout.eu mirror | done; not tested against the live mirror or with a real C# `Data.gmdb` |
| 5d | Plane server: `airscout-planeserver`, a headless daemon or a window with a tray icon that collects plane feeds and serves `/planes.json` | done; not tested with a real tray icon, on GNOME or Wayland, or against a live feed |

## Build

You need a stable Rust toolchain (see `rust-toolchain.toml`).

```sh
cargo build --release
cargo test --workspace
```

This builds the binaries `airscout` (the app), `airscout-cli`, `catcheck`,
`elevation-tile-gen`, `maptile-gen` and `airscout-planeserver`.

On Linux, `airscout-planeserver` (and so `cargo build --workspace`) needs the GTK 3 development
files for its tray icon: `libgtk-3-dev` (Debian and Ubuntu name). That is all a build needs. The
CI also installs `libayatana-appindicator3-dev` and `libxdo-dev`; they are harmless extras. The
tray icon needs an AppIndicator library at run time (see the plane server section). The
plane server also needs the GTK 3 runtime on Linux in `--headless` mode: on a machine without a
desktop (Raspberry Pi OS Lite, a server image) install it with `sudo apt install libgtk-3-0`.

Native hamlib (optional, off by default, Linux and macOS only; Windows uses rigctld):

```sh
cargo build -p airscout --release --features hamlib
cargo build -p catcheck --release --features hamlib
```

This needs libhamlib 4.0 or newer with its development files (for example Debian
`libhamlib-dev`, Homebrew `hamlib`). If the linker does not find it, add
`RUSTFLAGS="-L <libdir>"` (on Apple Silicon Homebrew: `RUSTFLAGS="-L/opt/homebrew/lib"`). The
hamlib binding is hand-written for hamlib 4.x and was checked only against the 4.x headers from
memory, and CI does not build it. The ignored test `crates/as-cat/tests/hamlib_native.rs` talks to
hamlib's Dummy rig and needs a machine with libhamlib
(`cargo test -p as-cat --features hamlib -- --ignored`). The default build and the CI need no
hamlib.

The tests need no network and no .NET: the reference results of the original C# code are
committed under `tests/golden/`. `tools/golden-harness/run.sh` (Docker) regenerates them.

## Desktop app

```sh
cargo run -p airscout --release
```

Stage 3a shows the live map (OpenStreetMap tiles, distance rings, locator grid, the path
and its profile, beam wedges), the plane list and the nearest plane for the selected path.

Stage 3b adds:

- A first-run wizard (terms, coverage area, elevation models, your station, plane feed).
- Options: stations (QRV data, station map, horizon, local obstructions), plane feeds,
  elevation models, map, alarm, background work and time. Settings from stage 3a load unchanged;
  their `elevation_model` becomes the only enabled elevation model.
- MULTI mode: a path for every checked station of the watchlist, which you edit in its own
  dialog. Stations farther away than `Path_MaxLength` get no path.
- Background work that never blocks the window: path pre-calculation, elevation download and
  database maintenance. The pre-calculation stores the paths from your station to every known
  station in range once the elevation data of the area is complete. LEDs show the state of each. A failure of the stations, aircraft or
  elevation job turns its LED red with the reason on hover. A failure of the position
  database maintenance shows in the status line only; its LED stays green.
- Spectrum Lab: the Spectrum tab polls Spectrum Lab, plots the level and stores it.
- Alarm sound and an offline time (see below).

Stage 3c adds:

- HISTORY mode (Analysis tab, ON/OFF): loads the recorded plane positions from the position
  database off the UI thread and replays them with the player (Rewind, Back, Pause, Forward,
  Fast Forward, a scrollbar with report markers, Min and Max pickers). The live feed and the
  background workers stop in HISTORY (Options OK does not restart them there). Positions can be
  saved to and loaded from files in `<data directory>/Tmp/` (`*.json`, `*.csv`, `*.hist`; no file
  dialogs, the status line shows the full path) and cleared.
- Analysis dialogs, all of them jobs that never block the window. Each dialog can cancel its job,
  except Load History from URL: OFF stops that download. While a database maintenance job runs,
  ON and these jobs are disabled:
  - Load History from URL: downloads one day of an ADS-B history archive of your choice into the
    position database. There is **no default URL**: it stays off until you enter a URL and accept
    the disclaimer, it checks for 50 GiB of free disk space (the `fs4` crate) and it runs only when
    you press Start.
  - Export Path: the profile of the SINGLE path as CSV (`;` separated).
  - Plane History: export the positions of one hex or call sign, with distance and average speed.
  - Crossing History: the planes that crossed the SINGLE path over a time range, a bar chart and
    a CSV export, with the original's ambiguity check.
  - Show All Traffic: every plane route in a time range on a map.
- Database maintenance (Options, Database): size, path and "Database Maintenance" of all 14
  databases (view the tables read only, delete all rows of a table after a confirmation), delete
  all elevation and propagation paths, delete all map tiles (only `<data directory>/tiles`; the
  tile store `<data directory>/maptiles.gmdb` stays) and delete a single station.
- Elevation coverage maps: the Elevation tab shows which tiles of your area are present, after you
  press "Show coverage". The station map can overlay an elevation grid (zoom 17 and closer).
- Tracking display (Options, Track): azimuth, elevation and Doppler for the selected plane, the
  TRK and ROT status fields and a readout on the map. Stage 4a adds the outputs (below), 4b the
  radio control and 4c the serial and DDE rotators.
- `airscout --clean` (also `-clean`, `/clean`) opens the cleanup dialog before anything else:
  reset the settings, delete the databases, the tile cache or the `Tmp` files. Cancel starts the
  app without deleting, closing the window exits. It deletes only the named files below the data
  directory. The tile cache is `<data directory>/tiles`; the tile store `maptiles.gmdb` stays.

Stage 4a adds (Options, Network and Track tabs):

- Win-Test receiver (Network tab, "Activate Network Server"): a UDP server on
  `0.0.0.0:<port>` (default 9872) that answers or applies the messages Win-Test and wtKST send to
  the server name (default from the original). It runs on its own thread, restarts when the
  Options dialog closes (OK or Cancel) and stops while Options is open. The checksum and the sender are not checked, as in
  the original. Messages:
  - `ASSETPATH` (a request with band, calls and locators): computes the path and the nearest planes
    and answers with an `ASNEAREST` broadcast to `255.255.255.255:<port>` (calls, locators, plane
    calls, categories, distances, potentials and minutes). While it computes, no further datagram
    is received.
  - `ASSHOWPATH`: shows the path of two stations in SINGLE mode and plays (not in HISTORY) and
    brings the window to the front. Invalid messages change nothing.
  - `ASWATCHLIST`: replaces the watchlist by the entries plus the checked items; only while "Keep
    in sync with KST user list" is on.
  - `ASADDWATCH` and `ASREMOVEWATCH`: add or remove watchlist entries; only while the sync is off.
    No message adds more than `watchlist_max_count` entries.
- KST sync (Watchlist tab): the sync box needs the activated server. With the sync on, the
  watchlist follows `ASWATCHLIST` and "Clear watchlist" is disabled. Turn the sync off to edit the
  list by hand.
- Track outputs (Track tab), active while tracking and in the original's order:
  - UDP, Win-Test format: `SETAZIMUTH` to `255.255.255.255:<Win-Test port>` (default 9871).
  - UDP, AirScout format: `SETAZIMUTH`, `SETELEVATION` and `SETAZEL` to
    `255.255.255.255:<AirScout port>` (default 9872). The messages carry the server name and the
    azimuth and elevation.
  - File `<data directory>/Tmp/azel.dat` (`Tmp` is created when missing), rewritten in place every
    second in one of two formats: native (`<az>,<el>` with one decimal and a newline) or WSJT-X
    (four lines: Moon, Sun, Source with the azimuth and elevation, and the Doppler line).
  - The ROT field of the status bar shows the output state: STOPPED, TRACKING or ERROR. A UDP
    output that fails more than 10 times is switched off (1:1), the file output never counts. Each
    output is isolated: an error in one does not skip the others.
  - Serial and DDE rotators are described under stage 4c below, the web server under stage 4d.
  - Tracking runs on the Track worker: outputs and the Doppler push continue while the window is
    minimised.
- The Track tab refuses OK when the Win-Test and AirScout ports are equal. Options warns, without
  blocking OK, when two enabled sockets use one port. With the defaults the
  UDP server and the AirScout tracking output share port 9872.

Stage 4d adds the web server (see below).

Stage 4b adds CAT rig control (Options, CAT tab) and the Doppler push:

- Engines: **rigctld** (default) talks to a running Hamlib `rigctld` over TCP; **hamlib** (only
  in a build with `--features hamlib`, see Build) opens the device itself through libhamlib. The
  original's `[Serial]` engine, rig database and rig download are not ported (OmniRig is
  added in stage 4c, Windows only).
- Start rigctld yourself, for example `rigctld -m <model> -r <device>` (`rigctld -l` lists the
  models). Its default port is 4532, which is the default of "Port" (host default
  `localhost`; both are in the CAT tab). Do not start rigctld with `--vfo`: the client sends
  the plain commands without a VFO argument. Hamlib 4.0 or newer is needed (older versions
  reject the mode commands). The rigctld engine needs no serial settings; the serial settings
  of the tab apply to native hamlib.
- The worker runs on its own thread (`cat`) from "Activate CAT Interface" on. It polls the rig
  every "CAT Refresh [ms]:" (100 to 10000) and no socket or FFI call runs on the window thread.
  The CAT field of the status bar shows the state (ONLINE white on dark green, OFFLINE white
  on orange, NOCAT, NOPORT, NORIG, NOTSUITABLE and errors yellow on red, stopped grey). The tooltip shows the
  engine, the rig and the state; without a rig it reads "CAT error!". The rig text is kept after
  the worker stops, as in the original. The status line shows the rig reports (RX, TX, mode,
  RIT, split) and the errors.
- The dial frequency (Doppler base) follows the rig's RX frequency, except while TRACKING. A
  frequency of 0 never replaces it.
- Doppler strategies A to D (Track tab, enabled only with CAT and Track active) push RX and TX
  to the rig while TRACKING; the latest values are re-sent on every CAT refresh. The rig
  is switched to split first and restored (mode, split, RIT, RX, TX, then simplex) when tracking
  ends, when Options opens, in HISTORY and on exit. On exit the app waits up to 2 s for the
  restore; with Options open it does not wait. Nothing is sent without a saved rig state and
  no frequency of 0 Hz or less is ever written to a rig.
- Modes outside CW, CW-R, LSB, USB, DIG, DIG-R, AM and FM are read as none and not restored.
  The hamlib names are `CW`, `CWR`, `LSB`, `USB`, `PKTLSB`, `PKTUSB`, `AM` and `FM`.
- The numbers of the tab ("CAT Refresh [ms]:", timeouts) use drag steps of 100 because egui has
  no NumericUpDown increment.

Stage 4c adds the serial and DDE rotators, OmniRig and tracking on its own worker:

- GS-232A Az and Az/El (Track tab, serial output): the port name is `COM13` on Windows or
  `/dev/ttyUSB0` style on Linux and macOS; the baud rate defaults to 4800, 8N1, no handshake,
  1 s timeouts. Once a second the output sends `C` (Az) or `C2` (Az/El), expects a reply that
  starts with `+0` and then sends `M123` (Az) or `W123 045` (Az/El) without a reply. Values are
  rounded to three digits, so 359.6 gives `M360`, and a negative elevation gives `-005` (1:1).
- A format error of the reply counts twice (1:1), so the serial output switches itself off with
  the 6th format error. A timeout or I/O error is shown, not counted, and the port is reopened on
  the next step. An open failure is shown as "Track error: ..." and retried every step.
- HRD DDE (Track tab, Windows only): pokes `SET-AZ:` and `SET-EL:` (one decimal) to the DDE
  service `HRDRotator`, topic `Position`, item `PositionData`. The group is not shown on Linux
  and macOS. A missing HRD is counted as an error and switches the output off after the 11th.
- OmniRig (CAT tab, Windows only): engine "OmniRig" for OmniRig V1.19 (Rig 1 to 2) and V2.x
  (Rig 1 to 4). OmniRig owns the serial port and its own timeouts, so the serial fields of the
  tab do not apply. The rig list is fixed ("[OmniRig] Rig 1" to "[OmniRig] Rig 4"), without
  model names; the tooltip names the rig. Frequencies above 32 bit are never written.
- Tracking runs on the Track worker (also with a minimised window); the CAT worker is unchanged.
- Not tested against a real OmniRig, HRD or GS-232 controller. The hosted Windows CI job
  (`windows-latest`) builds and runs the Windows-only tests, which check that a missing HRD or
  OmniRig is handled. The owner tests the real devices.

Stage 4d adds the web server (Options, Network, "Activate Network Server" and "AirScout HTTP
Server Port:"):

- Open `http://<this computer>:9880/` in a browser (on this computer: `http://localhost:9880/`).
  The page is the original's web UI, embedded in the program. Options shows whether the server
  listens.
- The server runs while "Activate Network Server" is on, and also in HISTORY mode, as in the
  original. It restarts when the port or the switch changes and the Options dialog closes. The
  HTTP port must differ from the UDP server port; Options refuses OK otherwise.
- The browser loads the map tiles and the VRS planes itself, as in the original. AirScout
  proxies nothing and starts no download for a request: the lookups use stored data only.
- The page connects to wtKST at `ws://localhost:9881/AirScout` by default. To change it, edit
  `web.kst_websocket` in the settings file; there is no Options field.
- `/settings.json` serves a curated subset of the settings (the members the page reads), not the
  whole settings object, and saves nothing.
- The API answers errors as `Error: …` texts with HTTP 200, as the original. A request that the
  original never answers gets such a text too.
- GET requests can store station locations and computed paths in the databases, as in the
  original.
- `planes.json` (the planes for `/planes.json`) is written to `<data directory>/tmp/` once a
  minute while the server runs.
- CI tests the routes, the assets and the server, but not the page in a browser.

Not ported from the original: QRZ lookup and upload, the news feed, the map provider
acceptance and the second map of the Spectrum tab. See `docs/logic-findings.md`, #88 to #105.
The map preloader came in stage 5c (below). Stage 3c does not port the
FlightRadar24 lookup of `.hist` files (it needs a spoofed browser header), the three unused
maintenance dialogs, or the original's default history URL. Its findings are #106 to
#193. Stage 4a's are #194 to #215. Stage 4b's are #216 to #234. Stage 4c's are #235 to #253.
Stage 4d's are #254 to #277. Stage 5a's are #278 to #290. Stage 5b's are #291 to #305.
Stage 5c's are #306 to #324. Stage 5d's are #325 to #338.

- Settings: `Settings::default_path()`, which is `settings.toml` in the OS config directory
  of `eu.airscout.AirScout-RS` (macOS: `~/Library/Application Support/eu.airscout.AirScout-RS/`;
  Linux: `~/.config/airscout-rs/`, the `directories` crate lowercases the app name there).
- Data directory: the OS data directory of the same name (the same folder on macOS; on Linux
  `~/.local/share/airscout-rs`, or `$XDG_DATA_HOME/airscout-rs`), or `./data` if the OS has none. It holds the
  databases and elevation data.
- The alarm plays a 880 Hz, 300 ms beep when planes start to alarm (once per update), if
  `alarm_play_sound` is set (Options, Alarm). Without an audio output, or with the cargo feature
  `sound` off, the alarm is silent. On Linux the build needs `libasound2-dev` and `pkg-config`,
  or build with `--no-default-features` (no sound).
- Time: double-click the UTC field while paused to set the time source (online, or an offline
  UTC time). The field is salmon online and plum offline. With the offline time, the live view
  shows no plane updates, as in the original.
- Tile cache: `<data directory>/tiles` (the HTTP disk cache of the map view).
- Tile store: `<data directory>/maptiles.gmdb`, a GMap tile database. The app creates it empty
  at the first start. Every map layer reads it first and loads a tile over HTTP only when the
  store does not have it. Browsing never writes the store: only `maptile-gen import` and the
  map preloader fill it. "Delete All Map Tiles" and `--clean` leave it in place; delete the
  file yourself to drop it.
- Elevation data is downloaded from airscout.eu by the background job for the whole coverage
  area of every enabled model (on startup or periodically, Options → Database) and for the
  path when you press Play. "Download missing elevation data" (Options → Elevation,
  `gui.elevation_download = false`) turns off both; the LEDs then show only which squares are
  missing.
- The plane feed receives the centre of the configured area, never your station position.
- Map tiles come from the tile store first, then from the OpenStreetMap tile servers (or
  OpenTopoMap, or Thunderforest with your key). Mind the
  [tile usage policy](https://operations.osmfoundation.org/policies/tiles/): the app caches
  tiles on disk, sends the honest User-Agent `AirScout-RS/<version>` and shows the
  attribution on the map. Do not use it to bulk-download tiles.
- Offline maps: fill the tile store with `maptile-gen import` (for example from the GMap cache
  of the C# AirScout) or with the map preloader. Tiles you have browsed are also kept in the
  HTTP disk cache for as long as their cache headers allow.

Map preloader (Options, Map, "Enable Map Preloading"; stage 5c):

- Off by default (`gui.map_preloader_enabled = false`). As in the original, it runs only with
  the OpenStreetMap provider and an enabled GLOBE, SRTM3 or SRTM1 model, on startup or
  periodically like the other background jobs (Options, Database). With another provider it
  shows a notice and loads nothing.
- It downloads every tile of the settings area that is not in the tile store yet, zoom 5 down to
  the level you choose ("down to Zoom Level", default and maximum 11; a level below 5 loads
  nothing). The source is the airscout.eu mirror
  (`http://www.airscout.eu/downloads/ScoutBase/1/MapData/OpenStreetMaps/`), over plain HTTP. It
  loads one tile at a time with 100 ms between tiles.
- For the default area (35° N to 60° N, 15° W to 30° E) a run covers 74,916 tiles (zoom 5
  to 11; 56,026 of them at zoom 11 alone). The pauses alone take over two hours, so a first run
  takes several hours. Options shows the count for your area. A later run loads only the tiles
  still missing.
- It never loads from `tile.openstreetmap.org` or another provider's server. A mirror URL
  (`background.map_mirror_url` in `settings.toml`) on the domain of a map provider is refused
  with a notice, and a redirect is followed only on the mirror's own host.
- A tile is stored only when the answer is a 200 with at most 4 MiB of PNG or JPEG.
- The mirror's tiles are derived from OpenStreetMap, so the
  [ODbL](https://www.openstreetmap.org/copyright) attribution applies. Their age is unknown.

## Command line

`airscout-cli` is a smoke tool for the core and holds the developer tools. Every command prints
JSON on stdout.
Positions are Maidenhead locators (`JO50AA`) or `lat,lon`.

### Propagation (stage 1)

```sh
# path between two stations, downloading missing elevation squares
airscout-cli path --from JO50AA --to JO61UF --band 144 --data ./data --download

# radio horizon, 360 azimuths
airscout-cli horizon --loc JO50AA --h 10 --dist 100 --data ./data --download
```

### Aircraft data and feeds (stage 2)

```sh
# list the plane feeds: name, info, enabled by default, disclaimer
airscout-cli feeds

# download and merge the aircraft database (aircrafts.db3 in --data)
airscout-cli aircraft-update --data ./data

# run a feed over an area (minlat,minlon,maxlat,maxlon) for 3 requests
airscout-cli planes --feed OpenSky --area 50,10,51,12 --data ./data --cycles 3

# feed settings are key=value; the value is TOML, plain text counts as a string
airscout-cli planes --feed Readsb --area 50,10,51,12 --data ./data \
    --feed-setting url=http://192.168.1.10/tar1090/data/aircraft.json

# crossing prediction for a list of planes (PlaneJSON, as AirScout serves planes.json)
airscout-cli nearest --from JO50AA --to JO61UF --planes planes.json --data ./data
```

`planes` prints the planes that passed the original's checks (area, altitude window
`Planes_MinAlt`..`Planes_MaxAlt`, plausibility), the newest report per hex. `nearest` prints
the potential (0, 50, 75, 100), the distance to the crossing point and the minutes until
the plane gets there.

### Developer tools (stage 5a)

```sh
# ask a running AirScout for a path (144.3 MHz in 100 Hz units, call, locator, DX call,
# DX locator), then list the datagrams on the port, the ASNEAREST answer included, for 5 seconds
airscout-cli wintest-sim --msg ASSETPATH --data "1443000,TE1ST,JO50AA,TE2ND,JO61AA" --listen 5

# one horizon CSV per station of stations.db3 in --data
airscout-cli horizon-gen --data ./data --stations all --out ./horizons

# CAT test bench over the app's CAT settings
catcheck --engine rigctld --host localhost --port 4532 status
catcheck set-freq 144300000
catcheck doppler-test --shift-hz 500
```

- `wintest-sim` sends one UDP datagram per run, by default as a broadcast to port 9872. The
  message name is a Win-Test message name in any case, or its decimal value. An unknown name
  is an error, and nothing is sent.
- `wintest-sim --listen <s>` then lists the datagrams that arrive on the port for 1 to 3600
  seconds (at most 1000). The message is sent from that listening socket. Its own echo is
  skipped by source port and bytes only, so an identical datagram from another host with the
  source port `--port` is skipped too. Use `--listen` with the default broadcast address. With a
  unicast `--addr` on Linux, address reuse can hand the datagram to the simulator itself (it is
  then dropped as its own echo) instead of to a running AirScout.
- `horizon-gen` is offline: it reads `stations.db3` and the elevation store and downloads
  nothing. It writes one CSV per station, `<CALL>_<LOC>.csv` in `--out`: the bearing and 12
  band columns, separated by `;`, with 8 decimals and a `.` as the decimal point. A station
  whose area misses elevation data is listed under `incomplete`. The flags `--model`,
  `--height`, `--dist`, `--k` and `--max-elv` default to the original's fixed values. The F1
  clearance comes from the default band table, or from the original's thresholds with
  `--legacy-f1` (see #281).
- `catcheck` reads the CAT settings of the app (`--settings <file>` for another file). The flags
  `--engine`, `--host`, `--port`, `--model`, `--serial-port`, `--baud`, `--rig` and `--timeout-ms`
  override them. The subcommands are `status`, `set-freq` (`--tx` for TX), `set-mode`, `split`,
  `rit`, `doppler-test` and `list-ports`. Every frequency, each Doppler step included, must lie
  between 1 Hz and 100 GHz, or nothing is sent.
- `doppler-test` prints the saved state first (`saved: rx=… tx=… mode=… split=… rit=…`). It
  then shifts RX up and TX down in steps through the app's CAT worker, restores the rig and checks
  the restore. After a Ctrl-C during the test the rig is not restored. Restore it by hand, using
  the values of the `saved:` line, in this order. Order matters: with split off, `set-freq --tx`
  writes the main VFO.
  - `split=OFF`: `catcheck split off`, then `catcheck set-freq <rx>`.
  - `split=ON`: `catcheck split on`, then `catcheck set-freq <rx>`, then
    `catcheck set-freq --tx <tx>`.
  - In both cases, then `catcheck set-mode <mode>` and `catcheck rit on` or `catcheck rit off`
    if they changed.

### Elevation tools (stage 5b)

```sh
# .loc tiles from SRTM3 files, one model per run and per tree
elevation-tile-gen tiles ./srtm3/*.hgt --out ./www/ElevationData/SRTM3

# one zip per square, <out>/AB/ABCD.zip
elevation-tile-gen zip --out ./www/ElevationData/SRTM3

# the tile catalogue, <out>/files.cat and <out>/files.zip
elevation-tile-gen catalogue --out ./www/ElevationData/SRTM3

# which SRTM3 tiles the server has, and which of them are missing in ./data
airscout-cli elevation-coverage --download --model srtm3 --data ./data --geojson coverage.json
```

Host your own elevation data:

- Serve `./www` with any static web server. Then set `gui.elevation_base_url` in
  `settings.toml` (or `--base-url` of `airscout-cli`) to `http://<host>/ElevationData`.
- The model directory names are `GLOBE`, `SRTM3` and `SRTM1`.
- The inputs are SRTM `.hgt` files (1201² or 3601² samples) and the GLOBE tiles `A10G` …
  `P10G`. Each run takes one model.
- The licences of the source data apply. The repository contains no elevation data.

What the generator does:

- The tiles match the original generator 1:1, its quirks included (#293, #294).
- `tiles` keeps existing `.loc` files unless `--overwrite`. It writes every tile with one
  `LastUpdated` time per run (`--time`, default now).
- `zip` keeps the `.loc` files unless `--remove-loc`. It reads each new zip back and compares it
  with its sources before it replaces the old zip. With `--remove-loc` it deletes the `.loc`
  files only after that check.
- `zip` also keeps the tiles of the old square zip that have no `.loc` file in the folder (#304).
  A `.loc` file on disk replaces the old entry of the same name. To drop a tile, delete the
  square zip and zip the folder again.
- `catalogue` takes the times from the tiles (`LastUpdated`), not from the files.
- `compare <A> <B> --out <DIFF>` writes the difference `A - B` of two SRTM files (little-endian,
  as the original) and prints its statistics. `--histogram <CSV>` also writes the histogram to
  that file.
- `downsample <SRTM1.hgt> --out <SRTM3.hgt>` makes an SRTM3 file from an SRTM1 file. A block
  with a void gives a void (#299).
- `preview <ZIP or DIR> --out <PNG>` draws one square from its zip or from a folder of `.loc`
  files, in the colours of the original.
- `compare`, `downsample` and `preview` refuse an existing output file (`--out`, and the
  `--histogram` file of `compare`) unless `--overwrite`. The original writes fixed file names
  only, never a path you typed, so a typo here must not replace a source file or zip.
- `compare` and `downsample` refuse an input file larger than 129 600 000 bytes (the largest
  source file, a GLOBE file). `zip`, `catalogue` and `preview` refuse a `.loc` file or entry
  larger than 16 MiB.
- `preview` refuses a tile that appears twice, tiles of more than one square or of mixed
  shapes, and any shape the generator does not write. It does not follow a symbolic link to a
  directory or to a `.loc` file.
- Every file is written as `<file>.part` first and then renamed, so an interrupted run leaves
  no truncated file.

What `elevation-coverage` does:

- It needs a local catalogue (`--catalogue <files.zip or files.cat>`) or `--download`.
  Without either it is an error, and nothing is sent.
- `--download` makes one HTTP GET of `<--base-url>/<MODEL>/files.zip` into a fresh temp
  folder, which is removed afterwards. The request times out after 300 s.
- `files.cat` may be at most 1 GiB. A larger one is an error, never a cut list. The size of the
  real catalogues was not measured.
- `--area minlat,minlon,maxlat,maxlon` keeps every tile whose cell overlaps the area (#303).
- `--data` compares the catalogue with the elevation store. If `<model>.db3` is missing, every
  tile counts as missing, and no file is created. If it exists, the store is opened as the app
  opens it. That creates the empty `.db3` files of the other models if they are missing.
- `--geojson` writes one polygon per square (`--level square`, default) or per tile
  (`--level tile`).

### Map tiles (stage 5c)

```sh
# copy the tiles of a GMap cache (or a <z>/<x>/<y>.png tree) into the app's tile store
maptile-gen import --db maptiles.gmdb ~/old/Data.gmdb

# write the OpenStreetMap tiles of a store as <out>/<zoom>/<x>/<y>.png
maptile-gen export --db maptiles.gmdb --out tiles/ --type OpenStreetMap

# put the PNG files of one folder side by side into one PNG
maptile-gen combine tiles/11/1100 --out row.png
```

- `maptile-gen` reads and writes local files only. It makes no network request.
- The app's tile store is `<data directory>/maptiles.gmdb`. Point `--db` at it to fill it. A
  running app may show the new tiles only after a restart.
- The cache of the C# AirScout is a GMap database too: on Windows
  `%LOCALAPPDATA%\GMap.NET\TileDBv5\en\Data.gmdb`. `import` reads it directly. Other GMap
  `.gmdb` files work as well.
- `--type` takes a provider name (`OpenStreetMap`, `OpenTopoMap`, `OCM Landscape`) or a GMap
  `DbId`. `export` needs it when the store holds tiles of more than one provider, because
  their trees would overwrite each other (#309).
- `export` dates each file by the tile's cache time and skips a file that already has that
  time. A cache time outside the years 1601 to 9999 counts as none: the file is written
  without setting its time, and rewritten on every run.
- `import` takes only PNG and JPEG tiles of at most 4 MiB with a valid key (zoom up to 30, x
  and y below 2^zoom). It keeps a tile the store already has. In a tree it does not follow
  symbolic links and skips names that are not numbers.
- `combine` takes the `*.png` files of one folder (not its subfolders) in name order. It refuses
  an output wider or taller than 65,535 px or larger than 256 MiB, and an existing `--out`
  unless `--overwrite`.
- Every file is written as `<file>.part` first and then renamed.

## Plane feeds

| Feed | Source | On by default |
|------|--------|---------------|
| OpenSky | OpenSky Network REST API (default URL `https://opensky-network.org/api/states/all`), anonymous or with your OAuth client | yes |
| ADSBOne | ADSB.one API | yes |
| Readsb | your readsb / tar1090 `aircraft.json` | yes |
| Dump1090 | your dump1090 `aircraft.json` | yes |
| AirScoutServer | another AirScout instance | yes |
| VRSWebServer | Virtual Radar Server | yes |
| BinCraft | binCraft over AMPRNet | no |
| PublicBinCraft | binCraft of a readsb/tar1090 server. The host is configurable; the default is `http://localhost` (a self-hosted readsb/tar1090). `https://globe.airplanes.live` and `https://adsb.lol` are examples you must choose yourself (not a documented public API) | no |
| ADSBSupport | ADSB-Support (needs credentials) | no |
| RTL1090 | your receiver (Beast or AVR over TCP) | yes |
| FlexJSON | any JSON, columns guessed | yes |

Feeds that need credentials or use endpoints that are not public APIs are off by default
and show a disclaimer.

## Plane server (airscout-planeserver)

`airscout-planeserver` is a small program for a machine that has the plane feeds (for example a
Raspberry Pi next to your ADS-B receiver) and nothing else. It collects up to three plane feeds
into one list and serves it as `planes.json` on the LAN. The port of the original
`AirScoutPlaneServer` follows its intent and its wire format, because the original is
unfinished (findings #325 to #338).

**Security: the server listens on all interfaces (`0.0.0.0`), has no authentication and is on by
default. Everyone who can reach the port can read the list of planes. Bind it to the local
machine if the LAN is not trusted: set `bind = "127.0.0.1"` in `planeserver.toml`, or start with
`--bind 127.0.0.1`.** The OS firewall may ask on the first start.

Two modes, one binary:

```sh
airscout-planeserver --headless        # daemon: no window, stops on Ctrl-C or SIGTERM
airscout-planeserver                   # a window with a tray icon
```

Other options: `--config <path>`, `--bind <ip>`, `--port <n>` and `--data <dir>`. They apply to
this run only and are not saved. In the window, Apply saves the config and restarts the
service. There is no autorun key: use a service unit (below).

The config file is `planeserver.toml`, next to the app's `settings.toml` (Linux:
`~/.config/airscout-rs/planeserver.toml`, macOS:
`~/Library/Application Support/eu.airscout.AirScout-RS/planeserver.toml`). A missing file gives
the defaults. A file that cannot be parsed is an error and is not overwritten. A value out of
range is corrected on load with one warning line (finding #336). Keys and defaults:

| Key | Default | Meaning |
|-----|---------|---------|
| `server_active` | `true` | serve `/planes.json` |
| `bind` | `"0.0.0.0"` | listen address (default IPv4 `0.0.0.0`) |
| `port` | `9872` | TCP port |
| `min_lat`, `max_lat` | `35.0`, `60.0` | area for the feeds, degrees |
| `min_lon`, `max_lon` | `-15.0`, `30.0` | area for the feeds, degrees |
| `min_alt_m`, `max_alt_m` | `5000`, `20000` | altitude band, metres |
| `lifetime_min` | `5` | a plane stays in the list this many minutes (1 to 1440) |
| `interval_s` | `60` | poll cycle of the feeds, seconds (at least 1) |
| `feeds` | `[]` | up to three feed names from the table above, for example `["OpenSky"]` |
| `feed_settings` | none | per feed, `[feed_settings.<FeedName>]`, with the same keys as in the app's `settings.toml` |
| `feeds_disclaimer_accepted` | `[]` | feeds whose disclaimer you accepted (below) |
| `extended_plausibility` | `true` | the extended plausibility check of the feed worker |
| `max_error_dist` | `30.0` | largest accepted position jump, km per minute since the last report, as in the app |
| `data_dir` | the app's data directory | holds the aircraft database used for the type codes |
| `start_in_tray` | `false` | window mode: start with the window hidden |

`feeds` is empty by default, so the daemon says `no feed configured` and serves an empty list
until you name one.

**Restricted feeds.** A feed that is off by default (BinCraft, PublicBinCraft, ADSBSupport) or
that shows a disclaimer (PublicBinCraft, ADSBSupport, FlexJSON) starts only if its name is in
`feeds_disclaimer_accepted`. Otherwise its status says `disclaimer not accepted (<name>)`. The
window shows the disclaimer text and adds the name when you accept. Headless mode never accepts
for you: read the disclaimer in the app or in the window, then add the name to the file yourself.
This is stricter than the app, which asks only for feeds that have a disclaimer text (finding
#334).

**Serving.** `GET /planes.json` gives the list in the layout of the AirScout web server (the
FR24 layout), built from memory for each request. `GET /` gives a welcome page. Every other path
is 404. A query string is ignored. To use one plane server as the feed of another AirScout, choose
the feed `AirScoutServer` and set its URL to `http://<host>:9872/planes.json`. The default URL of
that feed is `http://localhost:9880/planes.json`: 9880 is the port of AirScout's own web server,
not of the plane server (finding #329). The server's TCP port 9872 does not collide with the
Win-Test UDP receiver on UDP 9872: the protocols differ.

**GTK 3 runtime (Linux).** The binary links GTK 3 for the tray icon, so it needs the GTK 3
runtime even in `--headless` mode. Without it the program fails to start with `error while
loading shared libraries: libgtk-3.so.0`, and a systemd unit with `Restart=on-failure` keeps
restarting it. On Debian, Ubuntu and Raspberry Pi OS install it with `sudo apt install libgtk-3-0`.

**Tray icon.**

- On Linux the tray icon needs an AppIndicator library at run time (`libayatana-appindicator3`
  or `libappindicator3`).
- On GNOME the tray icon is invisible unless the "AppIndicator and KStatusNotifierItem Support"
  extension is installed and enabled. Without it, closing the window to the tray, or
  `start_in_tray = true`, leaves the program running with no way to reach it except through its
  process. Install the extension, or do not use `start_in_tray` there (or use `--headless`).
- On Wayland a program cannot hide its window. Closing the window quits the server even when a
  tray icon exists, and `start_in_tray` starts the window minimised instead of hidden.

**Run as a service.** Each example runs the daemon. Replace the placeholder paths. Feed
credentials stay in the config file, so run the service as the user that owns it.

systemd user unit, `~/.config/systemd/user/airscout-planeserver.service`, then
`systemctl --user enable --now airscout-planeserver` (`loginctl enable-linger $USER` keeps it
running without a login):

```ini
[Unit]
Description=AirScout plane server
After=network-online.target

[Service]
ExecStart=%h/bin/airscout-planeserver --headless
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
```

launchd agent, `~/Library/LaunchAgents/eu.airscout.planeserver.plist`, then
`launchctl load ~/Library/LaunchAgents/eu.airscout.planeserver.plist`. A plist does not expand
`$HOME`: write your home directory in its place.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>eu.airscout.planeserver</string>
  <key>ProgramArguments</key>
  <array>
    <string>$HOME/bin/airscout-planeserver</string>
    <string>--headless</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict>
</plist>
```

Windows Task Scheduler (one line, runs at logon of the current user):

```bat
schtasks /Create /TN "AirScout PlaneServer" /SC ONLOGON /TR "\"%USERPROFILE%\bin\airscout-planeserver.exe\" --headless"
```

The binary is a console program, so this task opens a console window. Closing that window stops the daemon: leave it open, or minimise it.

## Network sockets

The app opens these UDP sockets, one TCP connection (CAT) and one TCP listener (web server) and
no other (the standalone binaries follow below; there is no other outbound traffic besides the HTTP requests of the earlier stages and
the map preloader's).
None is authenticated, and the checksum of a received datagram is not checked, as in the original.

| Socket | Address and port | Direction | Active when |
|--------|------------------|-----------|-------------|
| Win-Test receiver | `0.0.0.0:<server port>` (default 9872), bound with address reuse (and `SO_REUSEPORT` on unix) so other programs can share the port | inbound | Options, Network, "Activate Network Server" is on |
| ASNEAREST answer | `255.255.255.255:<server port>` (default 9872) from an ephemeral port | outbound broadcast | only as the answer to an `ASSETPATH` request |
| Track output, Win-Test format | `255.255.255.255:<Win-Test port>` (default 9871) from an ephemeral port | outbound broadcast | tracking is on and "UDP Broadcast (Win-Test) Az only" is selected |
| Track output, AirScout format | `255.255.255.255:<AirScout port>` (default 9872) from an ephemeral port | outbound broadcast | tracking is on and "UDP Broadcast (AirScout) Az/El" is selected |
| CAT rigctld | `<rigctld host>:<rigctld port>` (default `localhost:4532`), TCP | outbound | Options, CAT, "Activate CAT Interface" is on with the rigctld engine |
| GS-232A | `<track.serial_port>`, serial | open | Track tab serial output not None |
| HRD | DDE service `HRDRotator`/`Position` (local) | connect | Windows, Track tab DDE output |
| OmniRig | COM server `OmniRig.OmniRigX` (local) | connect | Windows, CAT engine OmniRig |
| Web server | `0.0.0.0:<HTTP port>` (default 9880), TCP, plain HTTP | inbound | Options, Network, "Activate Network Server" is on |
| Map preloader | `www.airscout.eu:80` (the host of `background.map_mirror_url`), HTTP GET `<mirror>/<z>/<x>/<y>.png`, User-Agent `AirScout-RS/<version>`, one request at a time (#323) | outbound | Options, Map, "Enable Map Preloading" is on, with the OpenStreetMap provider and GLOBE, SRTM3 or SRTM1 enabled; off by default |

Ports 9872 and 9871 are visible to the whole LAN while they are enabled. Win-Test messages carry
the server name and the azimuth and elevation; the `ASNEAREST` answer carries the calls and
locators of the request and the nearby planes (see above). With the default ports the receiver
and the AirScout tracking output share port 9872, and Options warns about it.

The web server port (default 9880) is visible to the whole LAN while the server runs. It has no
authentication and no TLS. Anyone who reaches it can read your call, position, DX station,
watchlist and plane feed URL, and can make AirScout compute and store paths. The OS firewall may
ask on the first start.

The serial, DDE and COM devices are local, not network sockets: none carries calls, locators
or positions, and the rotator receives azimuth and elevation only.

The developer tools and the plane server open these sockets and no other. Besides the plane
server's feeds, only `elevation-coverage --download` makes an HTTP request.

| Standalone binaries | Address and port | Direction | Active when |
|---------------------|------------------|-----------|-------------|
| `wintest-sim` send | `<--addr>:<--port>` (default `255.255.255.255:9872`), UDP, from an ephemeral port on `--bind` (with `--listen` from the listening socket) | outbound broadcast | each run |
| `wintest-sim --listen` | `<--bind>:<--port>` (default `0.0.0.0:9872`), UDP, address reuse | inbound | only for the `--listen` seconds |
| `catcheck` rigctld | `<host>:<port>` (default `localhost:4532`), TCP | outbound | engine rigctld |
| `catcheck` hamlib / OmniRig | local device / local COM server | open, connect | engine hamlib or OmniRig |
| `horizon-gen` | none | - | never |
| `elevation-coverage --download` | `<--base-url>/<MODEL>/files.zip` (default `http://www.airscout.eu/downloads/ScoutBase/1/ElevationData`), HTTP GET, User-Agent `AirScout-RS/<version>` | outbound | only with `--download`, one request per run |
| `elevation-tile-gen` | none | - | never |
| `maptile-gen` | none | - | never |
| `airscout-planeserver` web server | `<bind>:<port>` (default `0.0.0.0:9872`), TCP, plain HTTP, `GET /planes.json` and `/` only. Not the UDP 9872 of Win-Test: a different protocol, no conflict | inbound | `server_active` is on (the default) |
| `airscout-planeserver` feeds | the hosts of the configured feeds (HTTP GET (OpenSky: also a POST to its token URL) with User-Agent `AirScout-RS/<version>`, or TCP for RTL1090); the area centre is sent, never a station | outbound | each started feed |

The CAT connection is plain text without authentication (rigctld has none) and carries only
frequency, mode, split and RIT commands, never calls, locators or positions. Every reply is
treated as untrusted: a line is at most 256 bytes, and after a timeout or a protocol error the
connection is closed. The native hamlib engine opens the configured device itself; AirScout's
own code opens no socket for it, but libhamlib may for network rig models.

## Privacy and secrets

- The repository contains no passwords, keys or tokens. Credentials come only from your
  settings and are never logged or shown in error messages.
- All requests send the honest User-Agent `AirScout-RS/<version>`; no browser headers are
  faked.
- The Win-Test receiver accepts commands from any host on the LAN: they can show a path, change
  the watchlist and ask for the nearest planes. Leave "Activate Network Server" off on an
  untrusted network or filter the port.
- The `ASNEAREST` broadcast puts the calls and locators of the request and the calls of nearby
  planes on the LAN. The track output broadcasts the server name and the azimuth and elevation.
- The web server shows your call, position, DX station, watchlist and plane feed URL to anyone
  on the LAN who reaches its port, and lets them compute and store paths. Leave "Activate
  Network Server" off on an untrusted network or filter the port. It serves no credential and
  no API key: passwords, feed keys and the Thunderforest key are left out, and a user name and
  password in a URL are removed.
- The plane server's port is open to the whole LAN by default, with no authentication (see
  its section). `planes.json` shows every plane the server collected, with position,
  altitude and hex code, to anyone who reaches the port. It serves no config value, path or
  credential.
- Plane feed credentials live only in `feed_settings` of `planeserver.toml` (plane server) or
  `settings.toml` (app). Both files are written owner-only (mode 0600) on Unix. Credentials are
  never logged or shown in an error text, a status or the User-Agent.
- The serial, HRD and OmniRig devices send no personal data; the rotator receives azimuth and
  elevation only.
- CAT sends no personal data. A remote rigctld host receives the frequency and mode commands in
  plain text; point it only at a host you trust, or use an SSH tunnel.
- `wintest-sim` puts the given From, To and Data on the LAN. `--listen` prints every datagram
  that reaches the port, from any host.
- `catcheck` shows no setting except the rig ident (`[rigctld] host:port`, `[OmniRig] Rig n` or
  the hamlib model). A broken settings file is reported by its path, line and column only,
  never by its text.
- The catalogue request of `elevation-coverage --download` carries no call, locator or area.
  The area filter and the store check run locally.
- The map preloader's requests carry no call, locator or position. The tiles it asks for show
  `www.airscout.eu` your settings area, as they show the tile servers the part of the map you
  browse.
- The original's key server call (`getkey.php`, which sends call sign and position to
  airscout.eu) is not ported.

## License

AirScout-RS is a port of [AirScout](https://github.com/dl2alf/AirScout) by DL2ALF, which is
licensed under the GNU General Public License, Version 3.0 (confirmed by DL2ALF). As a derived
work, AirScout-RS is distributed under the same terms: **GPL-3.0-only**. See [LICENSE](LICENSE)
and [NOTICE](NOTICE).

The embedded web UI keeps its MIT, BSD-2-Clause and CC BY 4.0 licences. The licence texts and the
list of components are in `crates/as-web/assets/LICENSES/`; the server also serves them under
`LICENSES/`.
