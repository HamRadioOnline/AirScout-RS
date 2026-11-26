# AirScout-RS

AirScout-RS is a Rust port of the C# [AirScout](https://github.com/dl2alf/AirScout)
aircraft scatter prediction app for amateur radio. It runs on macOS, Linux and
Windows and follows upstream `dl2alf/AirScout` at `fbadef0f` (V1.4.5.1).
Prediction logic fixes are in progress. See the [logic findings](docs/logic-findings.md).

## Build and test

Use the stable Rust toolchain in `rust-toolchain.toml`.

- **macOS** needs Xcode or its Command Line Tools.
- **Windows** needs the Microsoft C++ build tools for the MSVC target.
- **Linux** needs a C compiler (`build-essential` on Debian and Ubuntu),
  `pkg-config`, `libgtk-3-dev` for the plane server, and `libasound2-dev` for
  desktop sound. Linux CI also installs `libayatana-appindicator3-dev` and
  `libxdo-dev`. To build the desktop app without sound, use `--no-default-features`.

On Debian, Ubuntu and Raspberry Pi OS, run `sudo apt install libgtk-3-0` if
the GTK 3 runtime is missing. The Linux plane server requires it even with `--headless`.

```sh
cargo build --release
cargo test --workspace
```

For native Hamlib 4.x on Linux or macOS, install `libhamlib-dev` on Debian or
`hamlib` from Homebrew and build with the `hamlib` feature.

```sh
cargo build -p airscout --release --features hamlib
cargo build -p catcheck --release --features hamlib
```

## Run

The release binaries are in `target/release`. The examples use binaries on
your PATH. Use `--help` for tool and subcommand options.

Start the desktop app and follow the first-run wizard.

```sh
cargo run -p airscout --release
```

Use `airscout-cli` for propagation and aircraft queries or to list plane feeds.

```sh
airscout-cli feeds
```

Run the plane server without a window after [configuring the feeds and listener](docs/plane-server.md).

```sh
airscout-planeserver --headless
```

Check the rig through `rigctld`.

```sh
catcheck --engine rigctld --host localhost --port 4532 status
```

Generate elevation tiles from local SRTM data.

```sh
elevation-tile-gen tiles ./srtm3/*.hgt --out ./www/ElevationData/SRTM3
```

Import local map tiles into a tile store.

```sh
maptile-gen import --db maptiles.gmdb ~/old/Data.gmdb
```

## Safety and privacy

- Credentials come only from your settings and are never logged or shown in errors.
  The desktop web server strips usernames, passwords and query strings from the
  plane feed and KST websocket URLs it serves. A credential placed in a URL path
  is served as configured, so do not put tokens in URL paths.
- AirScout-RS identifies itself with the User-Agent `AirScout-RS/<version>`.
  The planned RainScout port has a browser-like User-Agent and Referer exception for radar sources.
- Restricted feeds are off by default and require disclaimer acceptance in the plane server.
- Bulk history and map downloads require explicit user action.
- Tile browsing uses caching and attribution under the
  [OpenStreetMap tile usage policy](https://operations.osmfoundation.org/policies/tiles/).
  Browsers load their own tiles. The server never proxies them.
- The desktop network server and plane server HTTP listener are unauthenticated.
  On untrusted networks, keep the desktop network server off and disable the plane
  server HTTP listener or bind it to `127.0.0.1`.

## Documentation

- [Desktop features, settings, offline maps and CAT](docs/desktop.md)
- [Plane feeds and disclaimers](docs/plane-feeds.md)
- [Plane server configuration and tray](docs/plane-server.md)
- [Network sockets and ports](docs/network.md)
- [CLI examples, utilities and golden tests](docs/tools.md)

## Logic fixes and upstream

The port first reproduced upstream behaviour 1:1, including its defects
([docs/logic-findings.md](docs/logic-findings.md)). Stage 6 then fixed the approved
defects in this port ([docs/logic-fixes.md](docs/logic-fixes.md)). Each fix
records the C# location at `fbadef0f`, the equivalent C# change and the proving
test. These fixes are not yet in upstream AirScout. They will be offered to
DL2ALF later, one by one, as pull requests from a fork of `dl2alf/AirScout`.

## License

AirScout-RS is a port of [AirScout](https://github.com/dl2alf/AirScout) by DL2ALF,
licensed under the GNU General Public License, Version 3.0.
As a derived work, AirScout-RS is distributed under
the same terms (**GPL-3.0-only**). See [LICENSE](LICENSE) and [NOTICE](NOTICE).

The embedded web UI keeps its MIT, BSD-2-Clause and CC BY 4.0 licences. The
licence texts and component list are in [crates/as-web/assets/LICENSES/](crates/as-web/assets/LICENSES/).
The server also serves them under `LICENSES/`.
