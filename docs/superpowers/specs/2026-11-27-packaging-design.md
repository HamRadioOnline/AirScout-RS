# Packaging and distribution — design

Date: 2026-11-27. Status: approved in conversation (brainstorming), pending the owner's read of this document.

## Intent

AirScout-RS ships today as plain, unsigned archives (`release.yml`: `.tar.gz`/`.zip` with six binaries for
linux-x86_64, windows-x86_64, macos-arm64, macos-x86_64). The repository went public on 2026-11-27.
The owner wants every platform packaged properly: real installers, signed where the platform expects it,
installable and updatable through package sources the project controls, on more architectures. There are no
existing user expectations for channels; the KST4Contest repository (HamRadioOnline/kst4contest) already does
most of this for a Java app and is the reference for the macOS signing, AUR and Flatpak flows.

Success: a user on Windows, macOS or Linux (x86_64, arm64, armhf, i686 where the platform supports it)
installs AirScout-RS from a signed installer or a package source, gets a menu/dock entry for the GUI and the
command-line tools on the PATH, and receives updates through that source. A headless PlaneServer can run as a
service or a container.

## Goals

- One package per platform/architecture containing all six programs: `airscout` (GUI),
  `airscout-planeserver`, `airscout-cli`, `catcheck`, `elevation-tile-gen`, `maptile-gen`.
  Only the GUI gets a desktop/start-menu/dock entry; the others go on the PATH.
- Architectures (only where the table says yes):

  | Target | x86_64 | arm64 | armhf | i686 |
  |---|---|---|---|---|
  | Linux AppImage, `.deb`, Arch package | yes | yes | yes | yes |
  | Linux `.rpm` | yes | yes | no (Fedora dropped armhf) | no (Fedora dropped i686) |
  | Linux Flatpak | yes | yes | no (runtimes have no 32-bit) | no |
  | Windows MSI + ZIP | yes | yes | no | yes |
  | macOS DMG | yes | yes | – | – |

- Signing:
  - macOS: Developer ID signing + notarization + stapling, same method as KST4Contest
    (`packaging/macos/build-signed-dmg.sh`, `ci-import-cert.sh` there). One DMG per architecture
    (no universal binary).
  - Windows: Certum Open Source Code Signing on SimplySign (cloud), permanently. One certificate on the
    owner's person covers all of the owner's open-source projects. Sign the `.exe` files and the MSI.
    SignPath is not used.
  - Linux: packages themselves unsigned; the release carries `SHA256SUMS` plus a detached GPG signature.
    The Flatpak OSTree repo is GPG-signed (as in KST4Contest).
- Channels (all in repositories the organization controls, plus winget):

  | Channel | Where | Stable (`vX.Y.Z`) | Beta (`beta-vX.Y.Z-<suffix>`) |
  |---|---|---|---|
  | GitHub Release | AirScout-RS | release | pre-release, with installers and signatures |
  | Homebrew | new repo `HamRadioOnline/homebrew-tap` | cask `airscout-rs` | cask `airscout-rs@beta` |
  | Flatpak | branch `flatpak-repo` of AirScout-RS (KST4Contest pattern) | OSTree branch `stable` | OSTree branch `beta`, own `.flatpakref` |
  | AUR | packages `airscout-rs-bin`, `airscout-rs`, `airscout-rs-git` | `-bin` and source package | none (`-git` covers bleeding edge) |
  | winget | PR to `microsoft/winget-pkgs`, id `HamRadioOnline.AirScout-RS` | yes | no |
  | Container | `ghcr.io/hamradioonline/airscout-planeserver` | `X.Y.Z`, `latest` | `beta` |

  Alpha tags (`alpha-v…`) produce only the GitHub pre-release (installers and signatures included); they feed
  no package source.
- PlaneServer headless:
  - `.deb`, `.rpm` and the Arch package ship a systemd unit `airscout-planeserver.service`, disabled by default
    (`systemctl enable --now airscout-planeserver`), running `airscout-planeserver --headless`
    as a dedicated system user.
  - Docker image for linux/amd64, linux/arm64, linux/arm/v7, port 9872 (`DEFAULT_PORT`), config and data on a
    volume, `--headless`.
- Desktop integration on Linux: `.desktop` file, hicolor icons in several sizes, AppStream metainfo.
- Icon: own artwork (see "Own icons" below), converted to the needed sizes and formats (`.ico`, `.icns`,
  PNGs). Source and license go into `NOTICE`.
- The homebrew tap is built so KST4Contest can be added as a second cask later (not part of this work).

## Non-goals

- No own APT/DNF repository ("ein bissle zu viel"), no Flathub, no Chocolatey, no Scoop, no Snap.
- No Windows armhf, no rpm/Flatpak on 32-bit.
- No universal macOS binary.
- No in-app auto-updater; updates come from the package sources.
- No beta channel in winget or AUR.
- Adding KST4Contest to the tap.

## Anti-goals

- A package that installs but fails to start on its target (missing shared libraries on armhf/i686,
  unsigned/quarantined app on macOS). Every package gets a smoke test in CI where a runner can execute it.
- Secrets in the repository, in logs or in artifacts. Signing material lives only in GitHub secrets;
  the Certum TOTP seed and the Apple/Flatpak/AUR keys go into an environment that requires the owner's
  approval before a job can read them.
- A release that publishes to package sources while a signing or build job failed.

## Constraints (binding project rules)

- No new crates, dependencies or cargo features; `Cargo.lock` unchanged by this work; never `cargo update`.
  Packaging tools (nfpm, WiX, appimagetool, flatpak-builder, cross, ssign/osslsigncode, docker buildx) are
  CI-only downloads, pinned to exact versions and checksums; nothing is installed on the owner's machine
  without asking.
  `[package.metadata.*]` tables are allowed only if they do not change `Cargo.lock`.
- New Rust targets come only through `rustup target add` in CI (and `rust-toolchain.toml` targets if needed).
- The PlaneServer binary links eframe, tray-icon and (on Linux) GTK even when headless, and no cargo feature
  may be added; the systemd service and the container must therefore carry the GTK/appindicator/xdo runtime
  libraries. `--headless` must not need a display.
- GPL-3.0-only; LICENSE and NOTICE in every package; "AirScout by DL2ALF" credit kept.
- GitHub-hosted runners only (the self-hosted runner stays in the org and is not used here).
- Signed commits, linear history, push only when the owner asks.
- Tags and version check as in today's `release.yml` (X.Y.Z must equal the workspace version).
- Privacy: no personal data in package metadata beyond what the owner chose; the maintainer field uses the
  organization, not an email address, unless the owner provides one.

## Approaches (decided)

- A — cargo-packager: one Rust tool for app/DMG, MSI/NSIS, deb, AppImage, pacman. Rejected: thin armhf/i686
  and cross support; rpm, Flatpak, signing details and the service still need own scripts.
- B — one tool per format, hand-scripted like KST4Contest (cargo-deb, cargo-generate-rpm, WiX, appimagetool,
  …). Rejected: many tools, deb and rpm metadata maintained twice.
- **C — build once, package many (chosen).** Each architecture produces a staging directory of release
  binaries; packaging only wraps it:
  - `nfpm` (one YAML) builds `.deb`, `.rpm` and the Arch package for all Linux architectures, including the
    systemd unit, `.desktop`, icons and metainfo.
  - appimagetool per architecture.
  - WiX for the MSI, then Certum signing (ssign; osslsigncode for MSI if ssign cannot sign MSI).
  - The KST4Contest DMG scripts adapted for a native `.app` (simpler: no embedded runtime/JARs).
  - flatpak-builder for the Flatpak, from the prebuilt binaries.
  - `docker buildx` for the PlaneServer image.
  Why: new architectures cost little, package metadata lives in one place.

## Build matrix (decided)

- x86_64 and arm64: native GitHub runners (`ubuntu-22.04`, `ubuntu-24.04-arm`, `windows-latest`,
  `windows-11-arm`, `macos-latest`).
- armhf (`armv7-unknown-linux-gnueabihf`) and i686 (`i686-unknown-linux-gnu`): `cross` in Docker with the
  target's GTK/ALSA/xdo libraries; Windows i686 via `i686-pc-windows-msvc` on `windows-latest`.
- glibc floor stays at 2.35 for x86_64 (ubuntu-22.04); the builder picks the floor for the others and states
  it in the release notes.

## Secrets (owner action)

The owner adds these as repository secrets of AirScout-RS; the values cannot be copied by an agent:
`MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`, `MACOS_NOTARY_KEY`, `MACOS_NOTARY_KEY_ID`, `MACOS_NOTARY_ISSUER`,
`FLATPAK_GPG_PRIVATE_KEY`, `AUR_SSH_PRIVATE_KEY`, plus new: the Certum SimplySign credentials/TOTP seed, a
release GPG key for `SHA256SUMS`, a token that can push to `HamRadioOnline/homebrew-tap`, and a token for the
winget PR. The owner buys the Certum Open Source certificate (about 49 EUR/year, 459-day validity cap).
Until a secret exists, the matching step is skipped and the artifact ships unsigned (and is marked so in the
release notes); publishing to a package source requires its signing step to have succeeded.

## Open (owner, at the laptop)

- Certum order: identity check, utility bill, project PDF (KST4Contest repository page showing the owner as
  developer).
- Adding the secrets listed above as **repository secrets** of AirScout-RS (owner decision 2026-11-27: repo
  based, not organization secrets).

## Decided after the builder check (owner, 2026-11-27)

- **Application ID: `io.github.hamradioonline.AirScoutRS`** (Flatpak app-id, macOS `CFBundleIdentifier`,
  AppStream/desktop IDs; permanent). The data directory (`ProjectDirs` "eu", "airscout", "AirScout-RS")
  stays unchanged.

- **Own icons (owner, 2026-11-27), replacing the upstream art:** the upstream `AirScout/Main.ico` and
  `AirScoutPlaneServer/PlaneServer.ico` show a "Red Baron" toy plane by The IconBlock Ltd., licensed "Free for
  personal use" (icon-icons.com/icon/plane/76537); it must not be redistributed and is removed from the
  packaging. New artwork drawn for this project (GPL-3.0-only, HamRadioOnline), SVG sources in
  `packaging/icons/src/`: `airscout.svg` (two Yagi stations, dashed path reflected by a red plane — aircraft
  scatter), `planeserver.svg` (red plane, waves, server box), and simplified `airscout-small.svg` /
  `planeserver-small.svg` used for 16 and 32 px. All other formats are generated from these SVGs.
- **Rust logo badge:** `logo/rust-logo.svg` from `rust-lang/rust-artwork` (black, unmodified, CC-BY 4.0),
  opaque, thin white outline of 1 % of the icon edge (at least 1 px), width 46 % of the icon edge. AirScout
  icon: bottom right; PlaneServer icon: top left. Sizes 16 and 32 px use the small variants without badge.
  NOTICE credits the logo artwork (CC-BY 4.0) and states that Rust and the Rust logo are trademarks of the
  Rust Foundation, used unmodified to indicate the program is written in Rust.

## Builder-check rulings (Claude's calls, 2026-11-27)

- One GitHub environment `release` with the owner as required reviewer; only jobs that read signing secrets
  run in it. A missing secret skips only that artifact's signing and the channels that depend on it
  (e.g. no Certum: unsigned MSI, no winget); the rest publishes.
- Certum signing on a Linux runner with `ssign` (installed in CI with `cargo install --locked` at a pinned
  version); MSI via `osslsigncode` with Certum's PKCS#11 module if `ssign` cannot sign MSI. Fallback:
  Windows runner with SimplySign Desktop and `signtool`. The builder documents the exact secrets.
- Dependency-free Rust edits are in scope: `#![windows_subsystem = "windows"]` for the `airscout` GUI only
  (the PlaneServer keeps its console because it also runs as a service); icons embedded in the Windows
  `.exe` files and used as window and tray icons (the tray today draws a placeholder); `airscout --version`
  and `--help` for smoke tests. No new crates (Windows resources via the SDK's `rc.exe`/`windres` from
  `build.rs` or a prebuilt `.res`, builder's choice).
- MSI pre-releases: WiX `MajorUpgrade` with `AllowSameVersionUpgrades="yes"`, so a beta of X.Y.Z is
  replaced by the final X.Y.Z; one fixed UpgradeCode.
- The `hamlib` cargo feature stays off in all packages: CAT via `rigctld` works without it, the native FFI
  was never tested against a real libhamlib, and enabling it would force bundling libhamlib everywhere.

## Left to the builder

- Workflow file layout (one `release.yml` with reusable jobs, or split workflows).
- Exact pinned tool versions and checksums.
- Install paths per format (e.g. `/usr/bin`, `/usr/share/applications`, `C:\Program Files\AirScout-RS`,
  `/Applications/AirScout-RS.app` with CLI symlinks via the cask).
- The system user name and hardening options of the systemd unit.
- Flatpak runtime choice and sandbox permissions (network, serial ports for CAT/rotator, audio).
- The MSI upgrade code (fixed GUID), per-machine install, PATH entry.
- macOS entitlements (expected: none beyond hardened runtime and network client; no JIT).
- Smoke tests per artifact (start `--version`/`--help`, `--headless` PlaneServer answering `/planes.json`).
