# AirScout-RS Packaging and Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship installable AirScout-RS releases for Windows, macOS and Linux, with platform signing, update channels and a headless PlaneServer service and image.

**Architecture:** Build each target once into a six-binary stage, then assemble, smoke-test and sign package candidates without publishing them. Collect verified assets into the GitHub Release and enable each channel only after the release gate passes and that channel's required signing is available.

**Tech Stack:** Rust stable, egui/eframe, GitHub Actions, nFPM, appimagetool, WiX, Certum SimplySign, Apple's Xcode tools, Flatpak/OSTree, GnuPG and Docker Buildx.

**Spec:** [2026-11-27-packaging-design.md](../specs/2026-11-27-packaging-design.md)

## Global Constraints

- No new crates, dependencies or cargo features. Never `cargo update`. `Cargo.lock` unchanged. `hamlib` stays off. Keep the existing stable toolchain and install new targets with `rustup target add` in CI.
- CI-only packaging tools pinned to exact versions and checksums. Use immutable image digests and verify downloaded bytes before execution. Install nothing on the host machine.
- Prefix every local Cargo build/test/lint with `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` and pass `--locked`. For the Cargo fmt subcommand, which takes no lockfile flag, use the same environment and check the lockfile separately. Keep `target/` at or below 60 GB. Check `du -sk target` before and after local compilation and stop a build approaching the limit.
- Signed commits, linear history, no push. Each task's commit step means stage only its named files and run `SSH_AUTH_SOCK="$HOME/Library/Containers/com.bitwarden.desktop/Data/.bitwarden-ssh-agent.sock" git commit -S -m "<message>"`.
- No secrets in the repo, logs or artifacts. Use AirScout-RS repository secrets and the owner-reviewed GitHub environment `release` for signing and private-key jobs only. Missing credentials skip that signature and its dependent channels only, keep the installer available, and mark it unsigned in release notes. A configured signer returning an error fails the release gate.
- GPL-3.0-only. Include LICENSE and NOTICE in every package and retain the README credit “AirScout by DL2ALF”. Include applicable third-party notices and corresponding-source references for redistributed runtime components.
- GitHub-hosted runners only. Linux x86_64 glibc floor 2.35. Report measured floors for other Linux targets. No universal macOS binary, 32-bit RPM/Flatpak or Windows armhf.
- Maintainer and publisher metadata use “HamRadioOnline”. No personal names, personal emails or call signs in package metadata. Use synthetic test values TE1ST, JO50AA and localhost. Platform-managed certificate identity is addressed in Task 14.
- Permanent application ID `io.github.hamradioonline.AirScoutRS` for Flatpak, macOS, desktop and AppStream. Keep `ProjectDirs::from("eu", "airscout", "AirScout-RS")`. PlaneServer headless starts without a display and still requires GTK, appindicator and xdo runtime support on Linux.
- Preserve current tag grammar and workspace version check. Stable `vX.Y.Z`, beta `beta-vX.Y.Z-<suffix>`, alpha `alpha-vX.Y.Z-<suffix>`. Alpha publishes only the GitHub pre-release, including installers and available signatures.
- No own APT/DNF repository, Flathub publication, Chocolatey, Scoop, Snap or in-app updater. No beta AUR or winget. Keep the tap ready for another cask.
- Every task has local checks. Write stdlib-only fixtures under `packaging/tests/` and invoke named methods with `python3 -m unittest discover -s packaging/tests -p '<test-file>.py' -v`. Assert workflow structure using existing PyYAML with `BaseLoader` if importable, otherwise check job boundaries, declared keys and literals with stdlib text checks. Use `bash -n` for Bash and shellcheck only if present. Task 15 owns hosted verification.

## Review Focus

- A package built on a newer runner starts on the advertised floor and never loads a host-architecture library (Tasks 4, 5 and 15).
- The service starts with an empty state directory and no display, with its listener on localhost and the unit disabled at install (Task 4).
- A missing signing secret leaves unrelated channels eligible. A bad configured key blocks all publication (Task 10).
- A Windows beta and its same-version final use one installed product slot and retain user data (Task 6).
- A beta Flatpak update preserves both architectures of the stable branch and the prior usable repository on failure (Task 8).

---

## File Structure and Interfaces

- Application startup, resources and tests live in `crates/airscout` and `crates/airscout-planeserver`. `packaging/icons` owns upstream ICOs and derived assets. `packaging/linux` owns the shared desktop, service and package-root files.
- `.github/workflows/release.yml` keeps the tag and test gates, build matrix, publish barrier and channel conditions. Reusable jobs under `.github/workflows/release-{linux,appimage,windows,macos,flatpak,container,channels,assets}.yml` own package work and credentials. `ci.yml` keeps its existing gates, including the mingw build. Each reusable workflow declares its needed secrets explicitly. A caller grants only the permissions its callee needs.
- `packaging/tools.lock.json` records CI tools by name/platform with version, URL and SHA-256 or OCI digest. `packaging/release/check-tag.sh` emits `tag`, `version`, `channel` and `prerelease` into `GITHUB_OUTPUT`. `packaging/release/stage.sh <target> <slug> <output-dir>` writes `bin/`, README.md, LICENSE, NOTICE and `stage.json` with target, slug, version and Linux glibc floor. Actions artifact `stage-<slug>` is a tar transport on Unix to retain executable bits, ZIP on Windows.
- The canonical binary names are `airscout`, `airscout-planeserver`, `airscout-cli`, `catcheck`, `elevation-tile-gen` and `maptile-gen`, with `.exe` on Windows. Public asset basename is `AirScout-RS-<tag>-<slug>` plus extension. Slugs are `linux-{x86_64,arm64,armhf,i686}`, `windows-{x86_64,arm64,i686}` and `macos-{arm64,x86_64}`.
- Each package job uploads `package-<format>-<slug>` or an explicitly named private candidate artifact. The package payload includes `assets/` for public files, plus a sidecar `result.json` containing `format`, `slug`, `assets` (basename list), `signing` (`verified`, `missing`, or `not_applicable`) and measured Linux `glibc_floor` where applicable. Errors fail the job and never produce a successful sidecar.
- Locally test script interfaces with synthetic stages in temporary directories. Do not install host packaging tools or exercise external publication while implementing. Keep each package or channel as an independent review unit under this shared release contract.

### Task 1: Upstream icons and reproducible assets

**Files:** Create `packaging/icons/{Main.ico,PlaneServer.ico,AirScout-RS.icns,generate.sh,render.swift}`, `packaging/icons/hicolor/{16,32,48,64,128,256}.png`, `crates/airscout/assets/app-icon.rgba`, `crates/airscout-planeserver/assets/app-icon.rgba`. Modify `NOTICE` near the desktop map icon attribution. Test `packaging/tests/test_icons.py`.

**Interfaces:** `generate.sh <output-dir>` emits the named derivatives, with a 32×32 straight RGBA buffer (4096 bytes) for each Rust program. ICOs are source assets, with Main for GUI and PlaneServer for server.

- [ ] Write `test_icon_assets` and `test_icon_notice` with assertions `assert rgba_len == 32 * 32 * 4 and has_transparent_and_visible_pixels`, `assert png_sizes == {16,32,48,64,128,256} and icns.startswith(b"icns")` and `assert "AirScoutPlaneServer/PlaneServer.ico" in notice and "AirScout/Main.ico" in notice`.
- [ ] Run `test_icons.py` and expect missing assets and attribution.
- [ ] Copy verbatim from `/Users/philipp/Projects/hamradioomline/AirScout-RS/target/upstream/AirScout/{AirScout/Main.ico,AirScoutPlaneServer/PlaneServer.ico}`. Record upstream paths, DL2ALF and GPL-3.0 in NOTICE. Use Swift with macOS ImageIO/CoreGraphics and `iconutil` already supplied by macOS/Xcode. Extract the 256×256 PNG source frame present in each ICO, render the named PNG sizes and 32×32 RGBA buffers, and use the Mac iconset size/scale conventions for ICNS. Unpremultiply alpha for the buffers. Run with the installed tools when available or regenerate on hosted macOS. Keep the generated bytes in version control.
- [ ] Run the icon tests and `bash -n packaging/icons/generate.sh`. Verify each copied ICO with `cmp` against its source. Expect both comparisons and all tests to pass.
- [ ] Commit `assets(packaging): add upstream application icons`.

### Task 2: GUI command-line smoke entry and embedded resources

**Files:** Modify `crates/airscout/src/main.rs:1-37`, `crates/airscout-planeserver/src/main.rs:1-6`, `crates/airscout-planeserver/src/gui.rs:724`, `crates/airscout-planeserver/src/tray.rs:18-56`. Create `crates/{airscout,airscout-planeserver,as-cli,catcheck,elevation-tile-gen,maptile-gen}/build.rs` and `packaging/windows/embed-icon.rs`. Test `crates/airscout/tests/packaging_cli.rs` and the existing PlaneServer tray/CLI tests.

**Interfaces:** `airscout --version` writes `airscout <CARGO_PKG_VERSION>\n` and exits 0. `--help` writes usage including `--clean`, `--help` and `--version` and exits 0. `embed_icon(ico: &Path, bin: &str)` runs only for Windows targets, using SDK `rc.exe` for MSVC and target-prefixed `windres` for GNU. Keep `tray::icon_rgba() -> Vec<u8>`.

- [ ] Write `version_does_not_start_gui` and `help_does_not_start_gui` with `assert!(output.status.success())`, `assert_eq!(version, format!("airscout {}\n", env!("CARGO_PKG_VERSION")))` and assertions for the three help flags. Spawn with pipes and a bounded timeout, no display and disposable home. Assert the smoke paths create no data/config. Assert `icon_rgba().as_slice() == include_bytes!("../assets/app-icon.rgba")` in the tray test.
- [ ] Run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout --test packaging_cli --locked`. Expect failed smoke tests because startup still enters eframe.
- [ ] Handle the exact help/version flags before rustls, settings and eframe initialization. Preserve the cleanup path and its existing first-argument aliases. Apply `#![windows_subsystem = "windows"]` to the GUI only and attach each icon to its eframe viewport. Use the same PlaneServer RGBA bytes for the existing tray creation. Emit Cargo resource rerun directives and binary-scoped linker arguments. Preserve CI's MSVC and mingw paths. Use the corresponding ICO in each GUI/server PE. Use Main.ico for the four remaining Windows executables via their own small `build.rs` shims in `crates/{as-cli,catcheck,elevation-tile-gen,maptile-gen}`.
- [ ] Repeat the GUI test and run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test -p airscout-planeserver --locked`. Expect passing tests including `headless_serves_planes_json_while_running`. Run `git diff --exit-code HEAD -- Cargo.lock`.
- [ ] Commit `feat(packaging): embed app icons and add GUI smoke flags`.

### Task 3: Release staging and cross-architecture builds

**Files:** Modify `.github/workflows/release.yml` (`check-tag`, `test`, `build` and interim archive assembly). Create `Cross.toml`, `packaging/cross/{armhf,i686}.Dockerfile`, `packaging/tools.lock.json`, `packaging/release/{check-tag,stage}.sh` and `packaging/release/fetch-tool.py`. Test `packaging/tests/test_stage.py`.

**Interfaces:** Stage and tag interfaces defined above. `fetch-tool.py <name> <platform> <output-file>` reads the pin, verifies the exact SHA-256 and returns failure before exposing a mismatched binary. Preserve legacy archive uploads until Task 10 swaps the aggregator. Stages use a distinct `stage-*` artifact filter throughout.

- [ ] Write `test_tag_contract` with `assert classify("v0.1.0") == ("stable", "0.1.0")`, analogous beta/alpha cases and rejection of `v0.2.0`/invalid suffix against the current fixture. Write `test_stage_requires_six_programs` asserting exact filenames, preserved modes, legal files and rejection of one missing executable. Write `test_release_matrix` asserting the nine target/runner pairs below. Write `test_fetch_rejects_wrong_digest` with `assert status != 0 and not output.exists()` using a local file fixture.
- [ ] Run `test_stage.py`. Expect missing scripts and incomplete matrix.
- [ ] Use Linux `x86_64-unknown-linux-gnu` on `ubuntu-22.04`, `aarch64-unknown-linux-gnu` on `ubuntu-24.04-arm`, and `armv7-unknown-linux-gnueabihf`/ `i686-unknown-linux-gnu` via pinned Cross on `ubuntu-22.04`. Pin Cross base images by digest. Add only verified tool pins and implement the shared digest-verifying fetcher with stdlib Python. Install target-architecture GTK 3, ALSA, Ayatana appindicator and xdo development libraries and matching C toolchain/sysroot. Configure target pkg-config paths and use ELF headers, interpreter and symbol versions to reject wrong architectures and record floors. For Windows use `x86_64-pc-windows-msvc` and `i686-pc-windows-msvc` on `windows-latest`, `aarch64-pc-windows-msvc` on `windows-11-arm`. For macOS use `aarch64-apple-darwin` on `macos-latest` and `x86_64-apple-darwin` on `macos-15-intel` for native execution. Preserve the existing CI test gate and use `cargo build --workspace --bins --release --locked --target <triple>` or the corresponding `cross build`. Require the target stage to contain all six binaries before upload.
- [ ] Run `test_stage.py`, `bash -n` individually on both scripts, `python3 -m json.tool packaging/tools.lock.json` and `git diff --exit-code HEAD -- Cargo.lock`. Expect all checks to pass.
- [ ] Commit `ci(packaging): stage the release build matrix`.

### Task 4: nFPM, desktop integration and systemd

**Files:** Create `.github/workflows/release-linux.yml`, `packaging/linux/nfpm.yaml`, `packaging/linux/io.github.hamradioonline.AirScoutRS.{desktop,metainfo.xml}`, `packaging/linux/airscout-planeserver.{service,sysusers}`, `packaging/linux/{00-airscout-planeserver.preset,planeserver.toml}`, `packaging/linux/{install-root,package}.sh`, `packaging/linux/scripts/{deb-postinstall,deb-preremove,rpm-postinstall,rpm-preremove,arch-install}`. Modify `packaging/tools.lock.json`. Test `packaging/tests/test_linux.py`.

**Interfaces:** `install-root.sh <stage-dir> <root-dir>` populates a portable package root. `package.sh <stage-dir> <tag> <format> <output-dir>` invokes nFPM and emits the public asset and sidecar. AUR source/git reuse the install-root script. AppImage and Flatpak reuse desktop metadata.

- [ ] Write `test_root_layout` with `assert installed_bins == expected_six and desktop_execs == ["airscout"]` and checks for all hicolor sizes, AppStream ID and both legal files. Write `test_format_matrix` asserting `deb/archlinux == {x86_64,arm64,armhf,i686}`, `rpm == {x86_64,arm64}`. Write `test_service_fresh_install` asserting user, state/config paths, exact ExecStart flags and disable preset. Add `test_upgrade_keeps_enablement` asserting install hooks applied to a fake systemctl on upgrade never start, disable or re-preset the service.
- [ ] Run `test_linux.py`. Expect missing files and install interface.
- [ ] Install into `/usr/bin`, `/usr/share/{applications,metainfo,icons/hicolor}` and `/usr/share/licenses/airscout-rs`. GUI desktop ID and icon use the application ID. Set package name `airscout-rs`, maintainer HamRadioOnline and SPDX license. Use arch mapping `amd64,arm64,arm7,386` with expected deb `amd64,arm64,armhf,i386`, Arch `x86_64,aarch64,armv7h,i686` and RPM `x86_64,aarch64`. Use nFPM formats `deb,rpm,archlinux` with extensions `.deb,.rpm,.pkg.tar.zst`. Use Debian `X.Y.Z~<channel>.<suffix>`, RPM Version `X.Y.Z` with prerelease Release `0.<channel>.<suffix>` (stable `1`), and Arch upstream version `X.Y.Z<channel>.<suffix>` (stable `X.Y.Z`, pkgrel `1`). Verify comparison with the native package managers before publishing. Declare distro-specific GTK3, ALSA, Ayatana appindicator, xdo, systemd and other ELF/runtime dependencies for every target. Account for GTK graphics/backend assets normally supplied by distro packages.
- [ ] Install the service in the distro vendor unit path. Use `systemd-sysusers` for dedicated `airscout-planeserver`, a preset with `disable airscout-planeserver.service`, and first-install-only preset hooks without autostart. Use `StateDirectory=airscout-planeserver`, `User=airscout-planeserver`, `NoNewPrivileges=true`, `ProtectSystem=strict`, `PrivateTmp=true`, `Restart=on-failure`. ExecStart is `/usr/bin/airscout-planeserver --headless --config /etc/airscout-planeserver/planeserver.toml --data /var/lib/airscout-planeserver`. Ship a config with `bind = "127.0.0.1"`, `port = 9872` as a dist conffile (`config|noreplace` on RPM, `backup` on Arch); add `packaging/linux/planeserver.toml` to the root builder and preserve modified config/state on upgrade/removal. Include shared sysusers and preset metadata in source/git Arch recipes, with package hooks matching the nFPM Arch package.
- [ ] Run `test_linux.py` and syntax-check scripts. Expect exact file lists and disabled-by-default lifecycle checks to pass.
- [ ] Commit `feat(packaging): add native Linux packages and PlaneServer service`.

### Task 5: AppImage for four Linux targets

**Files:** Create `.github/workflows/release-appimage.yml`, `packaging/appimage/{AppRun,build.sh}`. Modify `packaging/tools.lock.json`. Test `packaging/tests/test_appimage.py`.

**Interfaces:** `build.sh <stage-dir> <tag> <output-dir>` builds `AppDir` and outputs `.AppImage` plus sidecar. `AppRun [--tool <binary>] [args...]` defaults to `airscout`. Paths are relative to the mounted or extracted AppDir.

- [ ] Write `test_apprun_dispatch` with `assert run("--tool", "airscout-cli", "--help") == ["airscout-cli", "--help"]` and rejection of `../outside`, using scripts as fake binaries in a path with spaces. Add `test_pinned_runtimes` with `assert arches == {"x86_64","aarch64","armhf","i686"}` and a digest assertion per runtime.
- [ ] Run `test_appimage.py`. Expect missing AppRun and pins.
- [ ] Reuse the shared desktop/icon/metainfo root and all six binaries. Place AppDir root desktop, icon and `.DirIcon` as the format requires. Bundle required target libraries from the matching build distribution/sysroot, including dynamically opened GTK/indicator support, with relative library/module paths. Keep libc, ELF loader, host graphics driver and NSS outside the bundle. Include notices for bundled libraries. Use pinned appimagetool via extract-and-run and explicit `--runtime-file` from a pinned Type 2 release for each CPU, never a mutable runtime download. Preserve the host shell PATH while dispatching AppDir binaries. Document desktop and PATH integration via extracting or linking a per-command wrapper invoking `--tool`.
- [ ] Run `test_appimage.py` and `bash -n packaging/appimage/build.sh`. Expect routing, input rejection and pin coverage to pass.
- [ ] Commit `feat(packaging): build AppImages for all Linux targets`.

### Task 6: Windows MSI, ZIP and Certum signing

**Files:** Create `.github/workflows/release-windows.yml`, `packaging/windows/{AirScoutRS.wxs,build-msi.ps1,sign.sh,build-zip.ps1}`. Modify `packaging/tools.lock.json`. Test `packaging/tests/test_windows.py`.

**Interfaces:** Sign the staged EXEs before MSI and ZIP assembly. Sign the final MSI afterward. `build-msi.ps1 -StageDir <dir> -Tag <tag> -Architecture <x64|arm64|x86> -OutputDir <dir>` produces `.msi`, and `build-zip.ps1 -StageDir <dir> -Tag <tag> -OutputDir <dir>` produces `.zip`. Emit one result sidecar per format. Mark ZIP verified after all six EXEs verify, and MSI verified after its signature and all six EXEs verify.

- [ ] Write `test_msi_contract` parsing WiX XML with assertions for six exe files, one shortcut, one PATH entry, one fixed UpgradeCode, machine scope and `AllowSameVersionUpgrades="yes"`. Write `test_signing_order` asserting `sign-exes < build-msi < sign-msi < package-result` and `sign-exes < build-zip`. Include complete, absent and partially configured secret sets.
- [ ] Run `test_windows.py`. Expect missing WiX input and workflow.
- [ ] Use pinned WiX supporting all three architectures and invoke it on an x64 Windows runner if the pinned tool cannot execute on ARM. Install to the architecture-appropriate Program Files known folder under `AirScout-RS`. Set `Manufacturer="HamRadioOnline"`, use UpgradeCode `{5CEC7FA4-CCAD-467A-91D1-759E51F78ADD}`, maintain stable component IDs for the same install resources and generate a distinct ProductCode for each tag/architecture. Use numeric X.Y.Z as ProductVersion within MSI limits, stable/beta replacement through MajorUpgrade, and meaningful downgrade error for smaller X.Y.Z. Add only the GUI shortcut and include all six EXEs, legal files and README. ZIP retains the six EXEs and legal files in a tagged directory.
- [ ] Implement isolated Linux signing jobs using repo secrets `CERTUM_ACCOUNT_EMAIL`, `CERTUM_TOTP_SEED` and `CERTUM_CERT_FINGERPRINT`, mapped to the pinned tool's supported inputs. Verify the selected leaf fingerprint against its configured value before use. Check the ssign crate digest and install it in CI using `cargo install --locked --version <pin>` from the verified source. Use native MSI signing only if the pinned ssign version supports it. Otherwise use pinned `osslsigncode` with the compatible Certum/ssign PKCS#11 module. Timestamp using the Certum RFC 3161 service and verify each EXE/MSI signature and timestamp before reporting verified. Document the hosted Windows SimplySign Desktop/`signtool` fallback and require a demonstrated noninteractive path before selecting it. Use scoped credentials and temporary files removed on every exit.
- [ ] Run `test_windows.py`, `bash -n packaging/windows/sign.sh` and XML parsing via the test. If PowerShell is present, parse both scripts with its parser. Expect all available checks to pass.
- [ ] Commit `feat(packaging): build and sign Windows MSI and ZIP assets`.

### Task 7: Native macOS bundle and DMG

**Files:** Create `.github/workflows/release-macos.yml`, `packaging/macos/{Info.plist,build-signed-dmg.sh,ci-import-cert.sh}`. Test `packaging/tests/test_macos.py`.

**Interfaces:** `build-signed-dmg.sh <stage-dir> <tag> <output-dir>` produces `.dmg` plus sidecar. Apps mount as `AirScout-RS.app`. Homebrew consumes fixed paths under `Contents/MacOS/`.

- [ ] Write `test_bundle_layout` asserting `CFBundleIdentifier == "io.github.hamradioonline.AirScoutRS"`, `CFBundleExecutable == "airscout"` and `CFBundleIconFile == "AirScout-RS.icns"`. Write `test_dmg_signing_order` asserting binaries → app → DMG → notarize → staple → verify, and that one missing Apple secret selects the unsigned build.
- [ ] Run `test_macos.py`. Expect missing plist and workflow.
- [ ] Place six binaries in `Contents/MacOS`, icon, README, LICENSE and NOTICE in `Contents/Resources`. Populate short and build versions with workspace X.Y.Z. Assemble an Applications link in each DMG. Adapt KST4Contest's throwaway keychain and inside-out signing from `packaging/macos/{ci-import-cert,build-signed-dmg}.sh`. Use Developer ID with hardened runtime and timestamp, no JIT entitlements. Use `MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`, `MACOS_NOTARY_KEY`, `MACOS_NOTARY_KEY_ID`, `MACOS_NOTARY_ISSUER`. Submit with `notarytool --wait`, staple, mount and verify with codesign/stapler and Gatekeeper assessment. Keep arm64 and Intel bundles distinct. For missing credentials, produce the unsigned DMG in an unprivileged job or the signing job's unsigned branch.
- [ ] Run `test_macos.py`, `bash -n` on both scripts and `plutil -lint packaging/macos/Info.plist` when present. Expect all available checks to pass.
- [ ] Commit `feat(packaging): build and notarize native macOS DMGs`.

### Task 8: Flatpak build and signed repository

**Files:** Create `.github/workflows/release-flatpak.yml`, `packaging/flatpak/io.github.hamradioonline.AirScoutRS.yml`, `packaging/flatpak/{stage,prepare-repo,publish-repo}.sh`. Modify `packaging/tools.lock.json`. Test `packaging/tests/test_flatpak.py`.

**Interfaces:** `stage.sh <stage-dir> <ostree-branch> <output-dir>` prepares one arch build. `prepare-repo.sh <base-repo> <candidate-dir> <stable|beta> <output-repo>` consumes both architecture builds and outputs a signed candidate plus public descriptor assets. `publish-repo.sh <candidate-repo> <expected-base-sha>` publishes only after Task 10 releases the candidate and gates the job.

- [ ] Write `test_channel_refs` with assertions for stable/beta branch and descriptor names and no alpha publication. Write `test_merge_keeps_refs` with `assert merged_refs == {"stable/x86_64","stable/aarch64","beta/x86_64","beta/aarch64"}` and `assert original_refs == before` when simulating a failed export. Use a fake flatpak command locally.
- [ ] Run `test_flatpak.py`. Expect missing manifest and merge interface.
- [ ] Build from prebuilt Linux binaries for x86_64/aarch64 using Freedesktop Platform and SDK 25.08, with pinned tooling and runtime refs. Put six executables in `/app/bin`; bundle missing runtime GTK/indicator/xdo components and notices under `/app`. Export the GUI desktop, icon and appstream ID. Grant network, Wayland/fallback X11, IPC, graphics and PulseAudio. Allow serial with documented `--device=all` opt-in when native device nodes are needed, plus host user permissions. Supply CLI invocation `flatpak run --command=<binary> io.github.hamradioonline.AirScoutRS//<branch>`. Alpha may smoke-test the local unsigned Flatpak build and exports no channel descriptors.
- [ ] In `release`, import `FLATPAK_GPG_PRIVATE_KEY` into a temp GnuPG home. Based on KST4Contest's OSTree-on-branch flow, export into a copy of `flatpak-repo`, retain refs for other architectures and channels, regenerate and sign summary, and export only the public key into `airscout-rs.flatpakrepo` and `io.github.hamradioonline.AirScoutRS.flatpakref` or `io.github.hamradioonline.AirScoutRS.beta.flatpakref`. Use `https://hamradioonline.github.io/AirScout-RS/` with `.nojekyll` and Actions deploying the exact tree from that branch. Use the Flatpak signing key for the bot's signed branch commit with a configured organization identity. Serialize updates, use a fast-forward/base-SHA guard, and on stale base reprepare and reverify before publishing. Prune unreferenced objects from the deployed candidate and preflight Pages size. Missing key skips repository signing and channel publication only.
- [ ] Run `test_flatpak.py` and syntax-check scripts. Expect both branches preserved and rejection of a partial update.
- [ ] Commit `feat(packaging): prepare signed stable and beta Flatpak channels`.

### Task 9: PlaneServer container

**Files:** Create `.github/workflows/release-container.yml`, `packaging/container/{Dockerfile,build.sh,publish.sh}`. Modify `packaging/tools.lock.json`. Test `packaging/tests/test_container.py`.

**Interfaces:** `build.sh <stage-root> <output-dir>` emits unpublished OCI candidates for `linux/{amd64,arm64,arm/v7}`. `publish.sh <oci-dir> <stable|beta> <version>` uploads the exact smoke-tested image descriptors and promotes the manifest to `ghcr.io/hamradioonline/airscout-planeserver`.

- [ ] Write `test_platforms_and_tags` with `assert platforms == {"linux/amd64","linux/arm64","linux/arm/v7"}`, stable tags `{"0.1.0","latest"}`, beta `{"beta"}`, alpha empty. Write `test_headless_entrypoint` asserting a non-root user, two writable mount paths and 9872/tcp.
- [ ] Run `test_container.py`. Expect absent build and image contracts.
- [ ] Select digest-pinned Debian/Ubuntu base per arch with glibc at or above the measured floor and add GTK/appindicator/xdo/ALSA plus actual loader dependencies and CA roots. Copy only PlaneServer plus README, LICENSE and NOTICE. Use a fixed non-root UID/GID 9872, `/config` and `/data` volumes, and exec-form entrypoint `airscout-planeserver --headless --config /config/planeserver.toml --data /data`. Use existing all-interface bind and default port 9872 in the container. Build and test offline from staged candidate files before pushing. Use OCI labels with organization and source URL. Serialize mutable tag promotion and verify existing immutable version tags match on retries. Do not promote older tags over a newer stable or beta.
- [ ] Run `test_container.py` and `bash -n` on the two scripts. Expect the exact three platforms and stable/beta-only eligibility.
- [ ] Commit `feat(packaging): assemble multiarch PlaneServer containers`.

### Task 10: Release asset inventory, notes and GPG gate

**Files:** Modify `.github/workflows/release.yml` (release and publication dependencies). Create `.github/workflows/release-assets.yml`, `packaging/release/{collect.py,notes.py,sign-checksums.sh}`. Test `packaging/tests/test_release.py`.

**Interfaces:** `collect.py <candidate-root> <tag> <output-dir>` validates the exact artifact roster, copies public assets and writes sorted `SHA256SUMS` and private `inventory.json`. `notes.py <inventory> <tag-message> <commits> <github-notes> <output>` renders notes. `sign-checksums.sh <output-dir>` produces detached ASCII `SHA256SUMS.asc` and reports verified/missing. Expose `windows_signed`, `macos_signed`, `flatpak_signed` and `checksums_signed` as booleans from successful release processing.

- [ ] Write `test_checksum_covers_final_bytes` asserting exact public basename coverage and rejection of missing assets, duplicate names and private sidecars. Write `test_missing_key_is_channel_local` asserting absent Certum permits signed macOS and eligible unrelated channels, while a failed configured signer or build permits zero publication. Add `test_notes_list_glibc_and_unsigned_assets` asserting each Linux floor and unsigned platform/checksum status explicitly.
- [ ] Run `test_release.py`. Expect missing collector, notes and release gate.
- [ ] Wire package candidate jobs, validate workflow results before collecting and replace interim archives. Require 10 native Linux packages, four AppImages, six Windows assets and two DMGs for each tag, plus Flatpak descriptors only for a verified stable/beta candidate. Treat intentional alpha channel suppression and missing credentials as explicit valid states. Compute sums over final signed or unsigned bytes, sorted by basename, then sign in `release` with `RELEASE_GPG_PRIVATE_KEY` and optional `RELEASE_GPG_PASSPHRASE` (required for an encrypted private key). Import into temp GNUPGHOME, verify detached signature, remove secret material and publish only the public key or fingerprint separately from the signing home. Include its trust/distribution instructions in Task 14.
- [ ] Preserve annotated tag message, previous-tag rules, filtered first-parent commits and GitHub-generated PR notes from current `release.yml`. Replace archive instructions with inventory-based download and signing status. Create a draft release, verify completeness and then publish it (stable latest, alpha/beta prerelease). On retry resume a matching draft or verify an existing published release matches before continuing. Block channels on failure or cancellation. If some credentials are missing, gate Homebrew on the verified DMGs, winget on verified MSI, Flatpak on signed OSTree, and stable AUR on GPG-signed release checksums and AUR SSH access. Wire separate Flatpak and GHCR publish jobs downstream of the completed release. GHCR uses its image candidate gate and package permission. A failed configured build or signing job anywhere prevents all publication.
- [ ] Run `test_release.py`, `bash -n packaging/release/sign-checksums.sh` and `git diff --check`. Expect deterministic checksum coverage and passing gate cases.
- [ ] Commit `ci(packaging): publish verified assets and signed checksums`.

### Task 11: Homebrew casks

**Files:** Create `.github/workflows/release-channels.yml`, `packaging/homebrew/{airscout-rs.rb.in,airscout-rs@beta.rb.in,render.py}`. Modify `.github/workflows/release.yml` (Homebrew call). Test `packaging/tests/test_homebrew.py`.

**Interfaces:** `render.py <tag> <SHA256SUMS> <output-dir>` renders `Casks/airscout-rs.rb` or `Casks/airscout-rs@beta.rb` for `HamRadioOnline/homebrew-tap`.

- [ ] Write `test_cask_selection` asserting stable and beta filenames, distinct arm64/Intel asset SHA-256 values, exact tagged release URLs and rejection of alpha/missing SHA. Write `test_cask_command_links` asserting all six bundle paths are represented as binary stanzas and both casks conflict to avoid colliding shared app and command paths.
- [ ] Run `test_homebrew.py`. Expect missing templates and renderer.
- [ ] Set cask version from the complete tag without the leading stable `v` (retain the beta suffix). Install `AirScout-RS.app`, link commands from `Contents/MacOS`, preserve user data on uninstall. Verify available DMG hashes from the release inventory. Gate publishing on `macos_signed` and `HOMEBREW_TAP_TOKEN`. Write only the selected cask in a verified signed fast-forward commit, with expected head and idempotent identical-content handling. Keep KST4Contest additions outside this task.
- [ ] Run `test_homebrew.py`. Parse rendered Ruby with `ruby -c` if present. Expect both architectures, exact URLs and correct cask selection.
- [ ] Commit `ci(packaging): publish stable and beta Homebrew casks`.

### Task 12: winget stable manifest PR

**Files:** Modify `.github/workflows/{release,release-channels}.yml`, `packaging/tools.lock.json`. Create `packaging/winget/{render.py,publish.ps1}`. Test `packaging/tests/test_winget.py`.

**Interfaces:** `render.py <tag> <SHA256SUMS> <output-dir>` writes three manifests under `manifests/h/HamRadioOnline/AirScout-RS/<version>/`, with installer metadata from the verified MSI inventory. `publish.ps1 -ManifestDir <dir> -Version <version>` opens or reuses the stable PR.

- [ ] Write `test_stable_manifest` asserting `PackageIdentifier == "HamRadioOnline.AirScout-RS"`, `Architecture == {"x64","arm64","x86"}`, machine MSI installers and final MSI URLs and hashes. Assert beta/alpha rejection and absent token or incomplete Windows signing yields no PR.
- [ ] Run `test_winget.py`. Expect missing renderer and publication job.
- [ ] Generate version, installer and default locale (`en-US`) manifests with HamRadioOnline publisher. Include the corresponding MSI product identity where winget uses it. Validate with the pinned winget tooling on Windows. Use `WINGET_TOKEN` and an owner-prepared fork of `microsoft/winget-pkgs`, a version-specific branch, a verified signed commit and a single PR per version. Repeat only when assets match.
- [ ] Run `test_winget.py` and PowerShell syntax parsing if available. Expect the exact stable manifest matrix and matching hashes.
- [ ] Commit `ci(packaging): submit signed stable MSI releases to winget`.

### Task 13: AUR stable and git packages

**Files:** Create `packaging/aur/{airscout-rs-bin,airscout-rs,airscout-rs-git}/{PKGBUILD,.SRCINFO}`, `packaging/aur/{render.py,publish.sh}`, `.github/workflows/{aur-publish,aur-nightly-git}.yml`. Modify `.github/workflows/release.yml` (stable AUR call) and `packaging/tools.lock.json`. Test `packaging/tests/test_aur.py`.

**Interfaces:** `render.py <stable|git> <ref> <input-dir> <output-dir>` outputs rendered PKGBUILD inputs. Stable input includes checksums for release Arch assets and a verified source snapshot at the tag. `publish.sh <recipe-root> <package-name>` fast-forwards the AUR package branch.

- [ ] Write `test_aur_recipes` asserting package names, organization maintainer, provides/conflicts, source-versus-bin strategy, distribution architecture mappings and beta/alpha rejection. Add `test_repo_rewrite_preserves_state` asserting the binary recipe installs all package-root files and copies neither `.PKGINFO` nor archive packaging metadata into the filesystem root.
- [ ] Run `test_aur.py`. Expect missing recipes and renderer.
- [ ] Adapt KST4Contest's PKGBUILD update, `makepkg --printsrcinfo` and pinned AUR SSH host key checks. For `-bin`, select the exact Arch asset and per-arch SHA-256, preserve service config backup and hooks, and copy the package root. For source/git, use locked native Cargo and Task 4's install-root with equivalent runtime deps. Tag/archive source requires a verified, pinned SHA-256. The git recipe follows main and derives `<workspace-version>.r<rev-count>.g<short-sha>`. Track `.SRCINFO` alongside the recipes and regenerate from the matching PKGBUILD in a digest-pinned Arch builder. Map `x86_64,aarch64,armv7h,i686` explicitly to corresponding release slugs. AUR is published once per package, with downstream Arch Linux ARM/Arch Linux 32 compatibility conditional on those distributions.
- [ ] Trigger stable AUR from the successful release gate after checksum signing. Trigger the git package's metadata refresh on a daily schedule and optional workflow_dispatch, validate that main's candidate builds first and skip stale or unchanged candidates. Use `AUR_SSH_PRIVATE_KEY` in `release`, temporary SSH config, known-host fingerprint validation, and signed fast-forward commits with organization automation identity. Use the approved SSH key for git SSH signing if permitted by AUR and owner key policy. Keep the AUR import and any update to its metadata independent of beta and alpha tags.
- [ ] Run `test_aur.py` and `bash -n` on all PKGBUILDs and the publish script. Expect synced fixture metadata, correct channel gating and package-root layout.
- [ ] Commit `ci(packaging): publish AUR binary, source and git recipes`.

### Task 14: Installation and release operations docs

**Files:** Modify `README.md` (installation and source credit). Create `docs/packaging.md`. Test `packaging/tests/test_packaging_docs.py`.

**Interfaces:** README links to the install matrix and verification instructions. The operations section lists repository secret names, approval configuration, owner actions and unsigned fallbacks.

- [ ] Write `test_docs_cover_release_contract` asserting the app ID, winget ID, flatpakrefs, casks, AUR packages, GHCR path, `SHA256SUMS.asc`, `systemctl enable --now airscout-planeserver`, port 9872, runtime libraries and credit are present across both docs.
- [ ] Run `test_packaging_docs.py`. Expect missing installation coverage.
- [ ] Explain per-format install/update, PATH integration, Ubuntu-compatible glibc floors from each release, app bundle backups, signed/unsigned behavior, and GPG fingerprint verification followed by `sha256sum -c SHA256SUMS` (or `shasum -a 256 -c SHA256SUMS`). Cover service setup, localhost and explicit network exposure, config/state paths and Docker volumes/UID. Document active-user permissions for Flatpak serial and CLI wrappers, AppImage integration, and ZIP command access. Include the source URL for corresponding source.
- [ ] Document owner provisioning of the Certum certificate, Apple material, GPG keys, AUR registrations/SSH access, GitHub tap, winget fork/token, GHCR visibility, Pages using Actions deployment of the `flatpak-repo` tree, and the repository secrets plus required reviewer for `release`. Explain that signed Windows/macOS files expose the public certificate subject, which might contain the owner's personal name, while package-authored metadata uses HamRadioOnline. Document signing availability and per-channel gating.
- [ ] Run `test_packaging_docs.py` and `git diff --check`. Expect all references to match the published contract.
- [ ] Commit `docs(packaging): document installation and releases`.

### Task 15: Hosted verification and publication acceptance

**Files:** Modify `.github/workflows/release.yml` and the reusable release workflows to wire their artifact smoke steps before the release gate. Create `packaging/tests/hosted/{smoke-stage.py,verify-package.sh,verify-windows.ps1,verify-macos.sh}` and `packaging/tests/test_hosted_contract.py`.

**Interfaces:** `smoke-stage.py <bin-dir> <version>` runs all six version/help commands in a clean environment and checks a bounded PlaneServer `--headless` request to `/planes.json` via localhost. `verify-package.sh <format> <asset> <version>` extracts or installs the artifact in a clean target userspace and runs the smoke. The native Windows/macOS verification scripts take asset and version and exercise real installer semantics. Successful candidate verification is required by Task 10. Publication acceptance runs only after each eligible channel publishes.

- [ ] Write `test_package_smoke_matrix` with assertions that every public package job appears upstream of the gate, each format's smoke case is mapped to all its required targets, and Linux images are built without hidden extra Cargo builds. Add `test_smoke_headless_is_isolated` with fake commands and a localhost HTTP fixture asserting isolated config/data, no display, bounded process lifetime, JSON status/shape and cleanup on failure.
- [ ] Run `test_hosted_contract.py`. Expect missing smoke interface and incomplete gate dependencies.
- [ ] Wire hosted smoke tests. For Linux verify artifact architecture, glibc and DT_NEEDED closure, desktop/AppStream validity, install and version/help all six on compatible clean userspace, and PlaneServer HTTP without a display. On armhf/i686 use matching userspace through pinned emulation when needed. Smoke AppImage extract-and-run in each arch, including every `--tool` dispatch. Test Debian, RPM and Arch service dependencies and packaged enablement. On a booted systemd runner verify disabled after install, manual enable/start as dedicated user, restart serving after upgrade, config and state retention, and no start as part of install. Where a hosted runner cannot boot a target service, inspect lifecycle scripts and still run the extracted CLI and daemon in its target userspace.
- [ ] On Windows inspect PE architecture and resources, verify all EXE/MSI Authenticode results when credentials exist, smoke MSI install plus PATH, GUI shortcut, uninstall and beta-to-final same-X.Y.Z upgrade with synthetic fixtures in CI. Test ZIP extraction separately. Check GUI help/version with redirected output and a timeout. On macOS mount the final DMG, inspect binary architecture, check six commands and expected app ID, and verify hardened runtime, Developer ID, notarization and stapled ticket when signed. Start the packaged PlaneServer everywhere the format permits.
- [ ] Test both Flatpak refs and their signed summaries using clean user installs, including an update preserving the other channel. Use Actions Pages deploy of the preverified tree checked out from the published `flatpak-repo` commit, since a `GITHUB_TOKEN` push does not trigger automatic branch-source deployment. Pull each GHCR platform by digest and verify non-root startup, persisted state and HTTP on 9872. Validate Cask install/checksums and CLI links, AUR makepkg output and rendered `.SRCINFO`, and winget manifest validation and PR state. Keep authenticated acceptance calls in jobs with required scopes and avoid logging credentials.
- [ ] Run `python3 -m unittest discover -s packaging/tests -v`, syntax-check the smoke scripts and run `git diff --check` and `git diff --exit-code <pre-packaging-base> -- Cargo.lock` using the commit at the start of implementation. Run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace --locked` and `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build --workspace --bins --locked` on the available host within the disk budget. Expect all locally runnable checks to pass.
- [ ] Commit `test(packaging): verify installable release artifacts`.
- [ ] After owner-authorized integration, the owner pushes a valid alpha tag matching the workspace version for the full GitHub installer and signature smoke. Verify the release is prerelease and every channel job stays skipped. Use an owner-pushed beta tag for Flatpak beta, Homebrew beta and GHCR beta. At the intended stable rollout, the owner pushes `vX.Y.Z` to exercise all stable channels, verify the stable MSI replaces the beta, and confirm the remote refs, checksums, notarization, package installs and winget PR outcomes. Record missing credentials and any skipped signing/acceptance checks separately. Treat actual signing, cross-target execution, systemd lifecycle, package-source publication and authenticated installs as hosted acceptance, and retain job links and outcomes.

## Spec Gaps and Builder Decisions

- `macos-latest` is arm64. The Intel build and execution use `macos-15-intel` to preserve native builds. [Hosted runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- The spec calls for a Git-backed Flatpak repository. A push with `GITHUB_TOKEN` cannot trigger Pages branch-source deployment. Configure explicit Actions Pages deployment from the signed branch, subject to Pages size limits. [GitHub Pages source configuration](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-a-github-pages-site)
- The Windows upgrade rule deliberately admits equal product versions in both directions. Task 15 tests the required beta-to-final direction. A policy forbidding final-to-beta at the same X.Y.Z would require an additional installer rule. [WiX MajorUpgrade behavior](https://docs.firegiant.com/wix3/xsd/wix/majorupgrade/)
- Verify the pinned Certum path against the owner's certificate before claiming noninteractive signing. Authenticode and Developer ID embed a public certificate identity even with organization-authored package metadata. [ssign documentation](https://docs.rs/crate/ssign/latest)
- Stable publication assumes AUR relies on signed Linux release checksums and GHCR has no separate signature requirement. AUR registrations, a winget fork and the tap are owner prerequisites. The signed-commit rule also requires a usable automation signing identity for published Git branches.
- Flatpak and AppImage expose sandbox-aware or image-relative command invocations, with host PATH entries available via the documented user wrappers. Serial devices in Flatpak require the documented user override and host device permissions. [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)
- Package availability on armhf/i686 requires target distributions that still provide the needed libraries. Intel/ARM compiler and runtime coverage, including the measured glibc floors, is conditional on successfully passing the hosted checks. [nFPM architecture mapping](https://nfpm.goreleaser.com/docs/arch-mapping/), [AppImage runtime releases](https://github.com/AppImage/type2-runtime/releases)
