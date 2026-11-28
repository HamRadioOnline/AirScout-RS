# Packaging, installation and releases

AirScout-RS publishes prebuilt packages for every platform on each
[GitHub release](https://github.com/HamRadioOnline/AirScout-RS/releases),
alongside a signed `SHA256SUMS` manifest. This document lists the install
options, how to verify a release, and how a tagged release is built, signed
and published.

Every package contains the same six programs:

- `airscout` — the desktop application
- `airscout-planeserver` — the aircraft-feed server
- `airscout-cli` — propagation, aircraft and feed queries
- `catcheck` — rig control check
- `elevation-tile-gen` — elevation tile generation
- `maptile-gen` — map tile store import

## Install

### Linux (native packages)

Debian/Ubuntu and RPM packages, plus Arch Linux packages, are attached to each
release. They install the six programs, a systemd unit for the plane server and
a default `planeserver.toml` that binds the HTTP listener to `127.0.0.1` on
port `9872`.

The plane server unit is installed but disabled by default (via a systemd
preset). After configuring the feeds, start it with:

```sh
systemctl enable --now airscout-planeserver
```

The unit runs `airscout-planeserver --headless --config /etc/airscout-planeserver/planeserver.toml --data /var/lib/airscout-planeserver`
as the `airscout-planeserver` user, with `NoNewPrivileges`, `ProtectSystem=strict`
and a private tmp, and uses `StateDirectory=airscout-planeserver` for `/var/lib`.

There is no APT, DNF, or Pacman repository: the `.deb`, `.rpm` and
`.pkg.tar.zst` files are standalone GitHub Release assets. Update by downloading
the newer release asset and installing it over the current one
(`sudo apt install ./<file>.deb`, `sudo dnf install ./<file>.rpm` or
`rpm -U <file>.rpm`, `sudo pacman -U <file>.pkg.tar.zst`). A deb or RPM upgrade
restarts a running PlaneServer service and leaves a stopped service inactive.

The packaged `planeserver.toml` is installed as a configuration file — a Debian
conffile, an RPM `%config(noreplace)`, or an Arch `backup` entry — so your edits
survive upgrades. On upgrade, dpkg prompts when a shipped default changed (or
keeps your version), RPM writes the new default next to yours as a `.rpmnew`
file, and pacman writes a `.pacnew` file. On removal, dpkg keeps conffiles until
`purge`, RPM renames a modified `%config(noreplace)` file to `.rpmsave`, and
pacman renames a modified `backup` file to `.pacsave`.

Service configuration lives in `/etc/airscout-planeserver/planeserver.toml` and
state in `/var/lib/airscout-planeserver`. The desktop application and its
settings use `directories::ProjectDirs::from("eu", "airscout", "AirScout-RS")`
(its config dir holds `settings.toml`); those are separate from the packaged
service paths.

By default the plane server listens on `127.0.0.1` only. To serve it on the
LAN, change `bind = "127.0.0.1"` in `/etc/airscout-planeserver/planeserver.toml`
to an interface address such as `0.0.0.0` and restart the unit; this exposes the
HTTP listener to the network. Keep the packaged `bind = "127.0.0.1"` setting for
localhost-only service. PlaneServer falls back to 0.0.0.0 if the file or bind setting is removed.
The systemd unit reads the conffile so an administrator can choose the listening address.

### Linux (AppImage)

A single-file AppImage runs on any glibc distribution at or above its floor.
The deb, RPM, Arch and AppImage assets for x86_64 and arm64 target glibc 2.35.
Release notes report the measured floor next to each asset.

| Architecture | Build userspace | glibc floor |
| --- | --- | --- |
| x86_64 | Ubuntu 22.04 | 2.35 |
| arm64 | Ubuntu 22.04 | 2.35 |
| armhf | Debian 12 | Measured per asset |
| i686 | Debian 12 | Measured per asset |

Make it executable and run it to open AirScout;
dispatch the other tools with `--tool`:

```sh
./AirScout-RS-vX.Y.Z-linux-x86_64.AppImage --tool airscout-planeserver
./AirScout-RS-vX.Y.Z-linux-x86_64.AppImage --tool airscout-cli feeds
```

On hosts without FUSE, prepend `--appimage-extract-and-run`.

For desktop integration, extract the AppImage (`--appimage-extract`) and install
its `.desktop` file into `~/.local/share/applications/`; for shell access,
install a wrapper named after each command that runs `--tool <binary>`.

Update by downloading the new AppImage and replacing the old AppImage; if you
installed desktop integration, re-extract (`--appimage-extract`) and reinstall
the `.desktop` file and the command wrappers from the new AppImage.

### Linux (Flatpak)

The Flatpak runs against the Freedesktop Platform/SDK 25.08 and is published as
an OSTree repository served from `https://hamradioonline.github.io/AirScout-RS/`.
The `airscout-rs.flatpakrepo` and the `.flatpakref` descriptors are attached to
a release (and served from that Pages URL) only on signed, non-alpha releases;
when the Flatpak key is absent, no Flatpak repository is published or deployed
for that release (the channel stays closed) and the descriptors are unavailable.
Add the repository, then
install a channel from its reference:

```sh
flatpak remote-add --if-not-exists airscout-rs airscout-rs.flatpakrepo
flatpak install --from io.github.hamradioonline.AirScoutRS.flatpakref        # stable
flatpak install --from io.github.hamradioonline.AirScoutRS.beta.flatpakref   # beta
```

Update an installed channel with `flatpak update`.

Serial devices (CAT rig control, rotators over USB) are not granted by default.
Opt in for your user with:

```sh
flatpak override --user --device=all io.github.hamradioonline.AirScoutRS
```

The six binaries are all inside the Flatpak; run a non-GUI tool with
`flatpak run --command=airscout-planeserver io.github.hamradioonline.AirScoutRS`
(and similarly for `airscout-cli`, `catcheck`, `elevation-tile-gen`, `maptile-gen`).

### Windows

The release ships a machine-scope MSI (plus a portable ZIP) for x64, arm64 and
x86. The MSI registers an entry on `PATH` and a Start-menu shortcut. When the
Certum secrets are configured, the EXEs inside the MSIs and the ZIPs are
Certum-signed too. Stable releases whose Windows binaries were signed are also
submitted to WinGet under the identifier `HamRadioOnline.AirScout-RS`:

```powershell
winget install HamRadioOnline.AirScout-RS
```

The portable ZIP does not register itself: add the extracted directory to `PATH`
to run the six `.exe` tools from any shell. Update an MSI install by running the
newer MSI — its `MajorUpgrade` rule replaces the previous version in place — or
with `winget upgrade`.

### macOS

When the Apple signing secrets are configured, the release ships a signed,
notarized and stapled DMG for Apple silicon and Intel; otherwise the DMGs are
unsigned and unnotarized. Mount it and drag `AirScout-RS.app` to `/Applications`.
The app bundle identifier is `io.github.hamradioonline.AirScoutRS`; the six
binaries are inside `AirScout-RS.app/Contents/MacOS/`. When the Apple signing
secrets are configured, stable and beta releases are also published as Homebrew
casks from the `HamRadioOnline/homebrew-tap` tap:

```sh
brew install --cask HamRadioOnline/homebrew-tap/airscout-rs        # stable
brew install --cask HamRadioOnline/homebrew-tap/airscout-rs@beta   # beta
```

The cask links the six binaries onto `PATH` (the `binary` stanzas in
`packaging/homebrew/airscout-rs.rb.in`); DMG-only users reach the CLI at
`AirScout-RS.app/Contents/MacOS/`. Update a DMG install by dragging the new app
over the old, or with `brew upgrade --cask`.

### Container

The plane server is also published as a multi-architecture OCI image at
`ghcr.io/hamradioonline/airscout-planeserver` for `linux/amd64`, `linux/arm64`
and `linux/arm/v7`. It runs as a fixed non-root user (UID/GID 9872), mounts
`/config` and `/data` for persistent state, and listens on `9872/tcp`.

```sh
docker run --rm -p 9872:9872 \
  -v airscout-config:/config -v airscout-data:/data \
  ghcr.io/hamradioonline/airscout-planeserver
```

The image is not a release asset and is not in `SHA256SUMS`, so pull it by digest to pin it; update with `docker pull`.

### Arch User Repository (AUR)

Three packages are maintained for Arch Linux:

- `airscout-rs-bin` — the prebuilt `.pkg.tar.zst` release binaries
- `airscout-rs` — built from the release source
- `airscout-rs-git` — built from the latest `main`, refreshed nightly

The maintainer is `HamRadioOnline`. Update the packages through your AUR helper
(for example `paru -Syu`).

## Verify a release

Each release ships a `SHA256SUMS` manifest of every asset. A detached GPG
signature (`SHA256SUMS.asc`) and the public key (`SHA256SUMS.key.asc`) are
attached only when the release key was available. To verify:

```sh
gpg --show-keys --with-fingerprint SHA256SUMS.key.asc   # check the fingerprint
gpg --import SHA256SUMS.key.asc
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum -c SHA256SUMS        # Linux
shasum -a 256 -c SHA256SUMS    # macOS
```

Compare the primary-key fingerprint that `gpg --show-keys --with-fingerprint`
prints with the fingerprint in the [README](../README.md#release-signing-key) and
with the fingerprint printed in the release notes (which
`packaging/release/notes.py` writes next to the signing status). A good
signature proves the manifest was signed with that key and has not been modified
since; it does not prove the assets are safe unless you trust the key through a
second channel, which is why the fingerprint must match a trusted copy rather
than only the key shipped in the same release. The `sha256sum -c` /
`shasum -a 256 -c` pass then proves the downloaded assets match the manifest
byte-for-byte.

## Runtime libraries

The Linux packages declare the shared libraries the binaries link even when the
plane server runs headless: GTK 3 (`libgtk-3-0` on Debian, `gtk3` on RPM/Arch),
ALSA sound (`libasound2` on Debian, `alsa-lib` on RPM/Arch), the tray-icon
indicator (`libayatana-appindicator`, `xdotool`/`libxdo`), X11, xkbcommon,
Wayland, the GL/Vulkan loaders and Mesa drivers, and the icon themes. The
Flatpak bundles the indicator and `xdotool` modules it needs; the container
installs the same loader libraries and CA roots. See
`packaging/linux/nfpm.yaml` for the exact per-format dependency lists.

## Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| `9871` | UDP broadcast | Win-Test tracking output (desktop) |
| `9872` | UDP | AirScout/Win-Test receive and AirScout tracking |
| `9872` | TCP (HTTP) | Plane server listener |

The packaged plane server binds its HTTP listener to `127.0.0.1` by default;
the container exposes `9872/tcp` on all interfaces. See
[Network sockets](network.md) for the complete table.

## Release pipeline

A tag push builds, verifies and publishes the release. Tags are `vX.Y.Z`
(stable), `alpha-vX.Y.Z-<suffix>` and `beta-vX.Y.Z-<suffix>` (pre-releases);
`X.Y.Z` must equal the workspace version in `Cargo.toml`. The pipeline runs:

1. `check-tag` — rejects a tag whose version differs from `Cargo.toml`.
2. `test` — `cargo fmt` runs on Linux only; clippy and the workspace tests run
   on Linux, macOS and Windows.
3. `build` — release binaries for all target triples, staged per platform.
4. Per-format packaging — Linux packages, AppImages, Windows MSI/ZIP, macOS
   DMG, Flatpak and the container image.
5. `release` (prepare) — the `collect.py` script validates every candidate and
   assembles the public asset roster, writing `SHA256SUMS` and `inventory.json`;
   `sign-checksums.sh` then signs the checksums, writing `SHA256SUMS.asc` and
   `SHA256SUMS.key.asc` when the release key is present.
6. `release` (publish) — `notes.py` renders the release notes and `publish.py`
   creates, verifies and publishes the release.
7. Publish — Homebrew, WinGet, Flatpak and the container are published only
   when their eligibility gates pass; AUR is published for verified stable
   releases.

## Signing and publication

- **Linux packages and AppImages** are not code-signed; their integrity comes
  from the GPG-signed `SHA256SUMS`.
- **Windows** MSIs, and the EXEs inside the MSIs and ZIPs, are signed with a
  Certum certificate (RFC 3161 timestamp server `http://time.certum.pl/`).
- **macOS** DMGs use Developer ID signing, notarization and stapling when the
  Apple secrets are configured.
- **Flatpak** publishes a GPG-signed OSTree repository. Both architectures install and run the signed candidate
  from a local remote before any branch update. Each publish keeps the current and previous OSTree commits per ref
  and replaces `flatpak-repo` with a single orphan commit using `--force-with-lease` against the recorded base.
- **Container** images are unsigned (the image is not a release asset and is not
  in `SHA256SUMS`).

A missing secret skips only that signing step and the channels that depend on
it; a configured-but-failing signer blocks publication. The publication gates
are: Homebrew for non-alpha signed macOS, WinGet for verified stable Windows,
Flatpak for non-alpha signed repositories, AUR for verified stable checksums
with AUR access, and the container for non-alpha tags. When a secret is absent,
the release notes mark the affected artifacts unsigned
(`Unsigned EXEs and MSIs.`, `Unsigned and unnotarized DMGs.`).

### Owner pre-flight checklist

- In the `release` and `github-pages` environments, configure **Deployment branches and tags**
  to allow the release tags `v*`, `alpha-v*`, and `beta-v*`. Add those tag rules
  in both environments when using a restricted deployment policy.
- Set Pages Source = "GitHub Actions".
- For the container channel, link the GHCR package to the repository and make the package public
  after the first push.
- If a ruleset protects `flatpak-repo`, allow the release workflow to force-update that branch.
- If a required reviewer is configured, expect one approval per `release` deployment.

The release pipeline uses these repository secrets:

`CERTUM_ACCOUNT_EMAIL`, `CERTUM_TOTP_SEED`, `CERTUM_CERT_FINGERPRINT`
(Windows), `MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`, `MACOS_NOTARY_KEY`,
`MACOS_NOTARY_KEY_ID`, `MACOS_NOTARY_ISSUER` (macOS),
`FLATPAK_GPG_PRIVATE_KEY` (Flatpak), `RELEASE_GPG_PRIVATE_KEY`,
`RELEASE_GPG_PASSPHRASE` (checksums), `AUR_SSH_PRIVATE_KEY` (AUR),
`HOMEBREW_TAP_TOKEN` (Homebrew) and `WINGET_TOKEN` (WinGet).

Secret values never appear in the repository or the logs. Signed Windows and
macOS files expose the public certificate subject — the signer name the
certificate authority recorded, which may contain the owner's personal name —
while the package-authored metadata (maintainer, publisher, manufacturer and
vendor) uses `HamRadioOnline`. No personal name appears in the package metadata
or this repository; only the certificate subject can carry one.

## Owner setup

Before the first tagged release, the owner provisions each channel once. Each
item below names the manual action, the secrets it produces and the channels it
unlocks.

- **Certum certificate (Windows).** Order a Certum SimplySign code-signing
  certificate, then record the account email, the TOTP seed and the certificate
  fingerprint as the `CERTUM_ACCOUNT_EMAIL`, `CERTUM_TOTP_SEED` and
  `CERTUM_CERT_FINGERPRINT` secrets. Unlocks signed Windows MSIs/ZIPs and the
  WinGet channel.
- **Apple material (macOS).** Export the Developer ID certificate as a base64
  `.p12` with its password (`MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`), and create
  an App Store Connect API key for notarization (`MACOS_NOTARY_KEY`,
  `MACOS_NOTARY_KEY_ID`, `MACOS_NOTARY_ISSUER`). Unlocks signed, notarized DMGs
  and the Homebrew channel.
- **GPG keys (checksums and Flatpak).** Generate the release key
  (`RELEASE_GPG_PRIVATE_KEY`, `RELEASE_GPG_PASSPHRASE`) used to sign
  `SHA256SUMS`, and paste its primary-key fingerprint into
  [README.md#release-signing-key](../README.md#release-signing-key). Generate the
  Flatpak repository key (`FLATPAK_GPG_PRIVATE_KEY`). Unlocks signed checksums
  (and therefore the AUR gate) and the signed Flatpak repository.
- **AUR registration and SSH key.** Register the three package names on the AUR
  (`aur.archlinux.org`) and add the SSH public key that pairs with
  `AUR_SSH_PRIVATE_KEY` to the maintainer's AUR web account (the `aur` remote
  login user then accepts it over SSH). Unlocks AUR publication.
- **Homebrew tap.** Create the `HamRadioOnline/homebrew-tap` repository with an
  initialized default branch (the cask publisher requires it) and add
  `HOMEBREW_TAP_TOKEN` with write access. Unlocks the Homebrew channel.
- **winget fork and token.** Fork `microsoft/winget-pkgs` as
  `HamRadioOnline/winget-pkgs` and add `WINGET_TOKEN` with write access to that
  fork and permission to open pull requests against upstream
  `microsoft/winget-pkgs` (for example a classic `public_repo` scope); the
  publisher pushes the manifests to the fork and then opens the PR on
  `microsoft/winget-pkgs`. Unlocks the WinGet channel.
- **GHCR package visibility.** Make the `ghcr.io/hamradioonline/airscout-planeserver`
  package public. Unlocks the container channel.
- **GitHub Pages.** Enable Pages with Source = "GitHub Actions" so the release
  workflow can deploy the `flatpak-repo` tree through the `github-pages`
  environment. Unlocks the Flatpak channel.

## Source and license

AirScout-RS is a Rust port of [AirScout](https://github.com/dl2alf/AirScout) by
DL2ALF, licensed under the GNU General Public License, Version 3.0. As a derived
work it is distributed under the same terms (**GPL-3.0-only**); see
[LICENSE](../LICENSE) and [NOTICE](../NOTICE). The corresponding source is the
[`HamRadioOnline/AirScout-RS`](https://github.com/HamRadioOnline/AirScout-RS)
repository, and every release attaches a source snapshot
(`airscout-rs-<version>-source.tar.gz`).
