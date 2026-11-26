# Desktop app

Start the desktop app and use the first-run wizard to set your coverage area,
elevation models, station and plane feed.

```sh
cargo run -p airscout --release
```

- The live map shows paths, profiles and nearby aircraft. MULTI mode follows
  checked stations in the watchlist. HISTORY pauses the live feed and replays
  recorded positions. History downloads have no default URL. They start after
  you accept the disclaimer and press Start.
- Options controls up to three simultaneous plane feeds (see
  [plane-feeds.md](plane-feeds.md)), elevation downloads, alerts, map
  preloading, tracking and CAT.
- Tracking can send UDP, write `<data directory>/Tmp/azel.dat` or drive a
  GS-232A rotator. Windows also supports HRD DDE and OmniRig. These hardware
  integrations have yet to be tested with real devices.
- `airscout --clean` opens a dialog for resetting settings and deleting
  databases or caches. It leaves the offline tile store in place.

## Local files and offline maps

- Settings are in `settings.toml` in the OS config directory. On macOS that
  directory is `~/Library/Application Support/eu.airscout.AirScout-RS/`. On
  Linux it is `~/.config/airscout-rs/`. Unix settings files use mode `0600`.
- The data directory holds the databases and elevation data. It is the same
  directory on macOS, `~/.local/share/airscout-rs` (or
  `$XDG_DATA_HOME/airscout-rs`) on Linux, and `./data` if the OS supplies no
  data directory.
- The app looks in `<data directory>/maptiles.gmdb` before fetching tiles.
  Populate it with `maptile-gen import` or the map preloader. Browsed tiles
  are cached in `<data directory>/tiles` according to their HTTP headers.
- Map preloading is off by default. When enabled, it uses the mirror at
  `http://www.airscout.eu/downloads/ScoutBase/1/MapData/OpenStreetMaps/`.
- The elevation downloader can run at startup, periodically or when you
  press Play. Turn off "Download missing elevation data" in Options → Elevation
  to keep elevation lookups local.

## CAT

The default CAT engine connects to `rigctld`. Use Hamlib 4.0 or newer and
start it with `rigctld -m <model> -r <device>`. Leave the `--vfo` flag off.

See the [build instructions](../README.md#build-and-test) for native Hamlib.
If the linker cannot find Hamlib, set `RUSTFLAGS="-L <libdir>"`
(`RUSTFLAGS="-L/opt/homebrew/lib"` for Homebrew on Apple Silicon).
