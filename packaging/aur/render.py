#!/usr/bin/env python3
"""Render AUR PKGBUILDs from the verified release payload or a git checkout."""
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "release"))
from collect import file, read_json, require, sha256, tag_info

REPOSITORY = "https://github.com/HamRadioOnline/AirScout-RS"
ARCH_SLUGS = {
    "x86_64": "linux-x86_64",
    "aarch64": "linux-arm64",
    "armv7h": "linux-armhf",
    "i686": "linux-i686",
}
PROGRAMS = ("airscout", "airscout-planeserver", "airscout-cli", "catcheck",
            "elevation-tile-gen", "maptile-gen")
DEPENDS = ("glibc", "libgcc", "gtk3", "alsa-lib", "libayatana-appindicator", "xdotool",
           "libx11", "libxkbcommon", "libxkbcommon-x11", "wayland", "libglvnd", "mesa",
           "vulkan-icd-loader", "vulkan-swrast", "adwaita-icon-theme", "hicolor-icon-theme",
           "systemd")
MAKEDEPENDS = ("cargo", "gcc", "pkgconf", "gtk3", "alsa-lib", "libayatana-appindicator", "xdotool")
INSTALL = HERE.parent / "linux" / "scripts" / "arch-install"
SOURCE_ARCHIVE = "airscout-rs-{version}-source.tar.gz"


def _list(values):
    return "(" + " ".join(f"'{value}'" for value in values) + ")"


def _header(pkgname, pkgver, pkgdesc, conflicts, makedepends):
    lines = [
        "# Maintainer: HamRadioOnline",
        f"pkgname={pkgname}",
        f"pkgver={pkgver}",
        "pkgrel=1",
        f'pkgdesc="{pkgdesc}"',
        "arch=('x86_64' 'aarch64' 'armv7h' 'i686')",
        f'url="{REPOSITORY}"',
        "license=('GPL-3.0-only')",
        f"depends={_list(DEPENDS)}",
    ]
    if makedepends:
        lines.append(f"makedepends={_list(makedepends)}")
    lines += [
        "provides=('airscout-rs')",
        f"conflicts={_list(conflicts)}",
        "backup=('etc/airscout-planeserver/planeserver.toml')",
        "install=arch-install",
    ]
    return lines


def _cargo_package(srcdir):
    return [
        "package() {",
        f'    cd "${{srcdir}}/{srcdir}"',
        "    mkdir -p stage/bin",
        "    for program in " + " ".join(PROGRAMS) + "; do",
        '        cp "target/release/${program}" "stage/bin/${program}"',
        "    done",
        "    cp README.md LICENSE NOTICE stage/",
        '    packaging/linux/install-root.sh stage "${pkgdir}"',
        "}",
    ]


def render_bin(version, hashes):
    lines = _header("airscout-rs-bin", version,
                    "Aircraft tracking and aircraft scatter prediction for amateur radio (pre-built)",
                    ("airscout-rs", "airscout-rs-git"), ())
    for arch, slug in ARCH_SLUGS.items():
        name = f"AirScout-RS-v${{pkgver}}-{slug}.pkg.tar.zst"
        lines.append(f'source_{arch}=("{name}::{REPOSITORY}/releases/download/v${{pkgver}}/{name}")')
        lines.append(f"sha256sums_{arch}=('{hashes[slug]}')")
    lines += [
        "",
        "package() {",
        '    cp -a "${srcdir}/usr" "${pkgdir}/"',
        '    cp -a "${srcdir}/etc" "${pkgdir}/"',
        "}",
    ]
    return "\n".join(lines) + "\n"


def render_source(version, source_sha256):
    lines = _header("airscout-rs", version,
                    "Aircraft tracking and aircraft scatter prediction for amateur radio",
                    ("airscout-rs-bin", "airscout-rs-git"), MAKEDEPENDS)
    lines.append(
        f'source=("airscout-rs-${{pkgver}}-source.tar.gz::'
        f'{REPOSITORY}/releases/download/v${{pkgver}}/airscout-rs-${{pkgver}}-source.tar.gz")')
    lines.append(f"sha256sums=('{source_sha256}')")
    lines += [
        "",
        "build() {",
        '    cd "${srcdir}/airscout-rs-${pkgver}"',
        "    cargo build --workspace --bins --release --locked",
        "}",
        "",
    ]
    lines += _cargo_package("airscout-rs-${pkgver}")
    return "\n".join(lines) + "\n"


def render_git_pkgbuild(pkgver):
    lines = _header("airscout-rs-git", pkgver,
                    "Aircraft tracking and aircraft scatter prediction for amateur radio (git)",
                    ("airscout-rs", "airscout-rs-bin"), ("git",) + MAKEDEPENDS)
    lines += [
        f'source=("airscout-rs::git+{REPOSITORY}.git")',
        "sha256sums=('SKIP')",
        "",
        "pkgver() {",
        '    cd "${srcdir}/airscout-rs"',
        '    BASE=$(awk -F\'"\' \'/^version = "/ {print $2; exit}\' Cargo.toml)',
        '    printf \'%s.r%s.g%s\' "${BASE}" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"',
        "}",
        "",
        "build() {",
        '    cd "${srcdir}/airscout-rs"',
        "    cargo build --workspace --bins --release --locked",
        "}",
        "",
    ]
    lines += _cargo_package("airscout-rs")
    return "\n".join(lines) + "\n"


def verify_checksum_signature(input_dir, fingerprint):
    manifest = file(input_dir / "SHA256SUMS")
    signature = file(input_dir / "SHA256SUMS.asc")
    key = file(input_dir / "SHA256SUMS.key.asc")
    with tempfile.TemporaryDirectory(prefix="aur-checksum-verify-") as home:
        gpg = ["gpg", "--no-options", "--homedir", home, "--batch", "--no-tty", "--no-autostart"]
        subprocess.run([*gpg, "--import", str(key)], check=True, stdin=subprocess.DEVNULL,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        verified = subprocess.run([*gpg, "--verify", "--status-fd", "1",
                                   str(signature), str(manifest)],
                                  check=True, stdin=subprocess.DEVNULL,
                                  capture_output=True, text=True)
    # Bind the signature to the release key, not merely to any key present in
    # the key file. A VALIDSIG status line records the primary-key fingerprint:
    # field 12 (index 11) in the full format, or field 3 (index 2) in the short
    # format / when the primary key itself signed (no subkey).
    primaries = set()
    for line in verified.stdout.splitlines():
        parts = line.split()
        if len(parts) < 3 or parts[:2] != ["[GNUPG:]", "VALIDSIG"]:
            continue
        primaries.add((parts[11] if len(parts) >= 12 else parts[2]).upper())
    require(fingerprint.upper() in primaries,
            "SHA256SUMS signature key does not match the release checksum fingerprint")


def render_stable(tag, input_dir, output_dir):
    version, channel = tag_info(tag)
    require(channel == "stable", "Only stable releases are published to the AUR")
    input_dir = Path(input_dir)
    inventory = read_json(input_dir / "inventory.json")
    require((inventory["tag"], inventory["version"], inventory["channel"]) ==
            (tag, version, channel), "Release inventory does not match the AUR tag")
    require(inventory["checksums_signed"] is True, "Release checksums are not signed")
    fingerprint = inventory.get("checksum_fingerprint")
    require(fingerprint and re.fullmatch(r"[0-9A-F]{40}|[0-9A-F]{64}", fingerprint),
            "Release checksum signature has no verified fingerprint")
    verify_checksum_signature(input_dir, fingerprint)

    names = {f"AirScout-RS-{tag}-{slug}.pkg.tar.zst" for slug in ARCH_SLUGS.values()}
    source_name = SOURCE_ARCHIVE.format(version=version)
    required = names | {source_name}
    checksums = {}
    for line in file(input_dir / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([a-zA-Z0-9_.-]+)", line)
        require(match, "Malformed SHA256SUMS entry")
        digest, name = match.groups()
        if name in required:
            require(name not in checksums, f"Duplicate AUR hash: {name}")
            checksums[name] = digest
    require(set(checksums) == required, "SHA256SUMS is missing a required AUR hash")

    assets = [asset for asset in inventory["assets"] if asset["format"] == "archlinux"]
    require(len(assets) == len(ARCH_SLUGS) and {asset["name"] for asset in assets} == names,
            "Unexpected Arch asset roster in release inventory")
    hashes = {}
    for asset in assets:
        name = asset["name"]
        require(asset["sha256"] == checksums[name] == sha256(file(input_dir / name)),
                f"Arch asset hash differs from the release inventory: {name}")
        hashes[asset["slug"]] = checksums[name]

    source_assets = [asset for asset in inventory["assets"] if asset["name"] == source_name]
    require(len(source_assets) == 1 and source_assets[0]["format"] == "source",
            "Release inventory is missing its source snapshot")
    require(source_assets[0]["sha256"] == checksums[source_name] == sha256(file(input_dir / source_name)),
            "Source snapshot hash differs from the release inventory")

    output_dir = Path(output_dir)
    require(not output_dir.exists(), f"Output already exists: {output_dir}")
    output_dir.mkdir(parents=True)
    _write(output_dir / "airscout-rs-bin", render_bin(version, hashes))
    _write(output_dir / "airscout-rs", render_source(version, checksums[source_name]))


def _workspace_version(ref, input_dir):
    text = _git(input_dir, "show", f"{ref}:Cargo.toml")
    section = re.search(r"(?ms)^\[workspace\.package\](.*?)(?=^\[|\Z)", text)
    require(section, "Cargo.toml has no [workspace.package] section")
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', section.group(1))
    require(match, "[workspace.package] has no version key")
    return match.group(1)


def render_git(ref, input_dir, output_dir):
    input_dir = Path(input_dir)
    version = _workspace_version(ref, input_dir)
    count = _git(input_dir, "rev-list", "--count", ref)
    short = _git(input_dir, "rev-parse", "--short", ref)
    output_dir = Path(output_dir)
    require(not output_dir.exists(), f"Output already exists: {output_dir}")
    output_dir.mkdir(parents=True)
    _write(output_dir / "airscout-rs-git", render_git_pkgbuild(f"{version}.r{count}.g{short}"))


def _git(directory, *args):
    result = subprocess.run(["git", "-C", str(directory), *args],
                            capture_output=True, text=True)
    require(result.returncode == 0, f"git {' '.join(args)} failed")
    return result.stdout.strip()


def _write(directory, content):
    directory.mkdir()
    (directory / "PKGBUILD").write_text(content, encoding="utf-8")
    shutil.copyfile(INSTALL, directory / "arch-install")
    (directory / "arch-install").chmod(0o755)


def main(argv):
    require(len(argv) == 5, "Usage: render.py <stable|git> <ref> <input-dir> <output-dir>")
    mode, ref, input_dir, output_dir = argv[1:]
    if mode == "stable":
        render_stable(ref, input_dir, output_dir)
    elif mode == "git":
        render_git(ref, input_dir, output_dir)
    else:
        raise ValueError(f"Unknown render mode: {mode}")


if __name__ == "__main__":
    try:
        main(sys.argv)
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        sys.exit(f"aur/render.py: {error}")
