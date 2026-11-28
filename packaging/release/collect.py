#!/usr/bin/env python3
"""Validate candidate artifacts and collect only the final public package bytes."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile

APP_ID = "io.github.hamradioonline.AirScoutRS"
LINUX = ("x86_64", "arm64", "armhf", "i686")
WINDOWS = ("x86_64", "arm64", "i686")
MACOS = ("arm64", "x86_64")
FLATPAK = {"x86_64": "linux-x86_64", "aarch64": "linux-arm64"}
PLATFORMS = {"amd64": "linux/amd64", "arm64": "linux/arm64", "armv7": "linux/arm/v7"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def file(path):
    require(not path.is_symlink() and path.is_file() and path.stat().st_size > 0,
            f"Missing or invalid file: {path}")
    return path


def entries(path):
    require(not path.is_symlink() and path.is_dir(), f"Missing or invalid directory: {path}")
    return {entry.name for entry in path.iterdir()}


def read_json(path):
    return json.loads(file(path).read_text(encoding="utf-8"))


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def tag_info(tag):
    match = re.fullmatch(r"(?:v(\d+\.\d+\.\d+)|(alpha|beta)-v(\d+\.\d+\.\d+)-[A-Za-z0-9.]+)",
                         tag, re.ASCII)
    require(match, f"Invalid release tag: {tag}")
    stable, channel, prerelease = match.groups()
    return stable or prerelease, channel or "stable"


def source_snapshot_name(version):
    return f"airscout-rs-{version}-source.tar.gz"


def collect_package(directory, tag, fmt, slug, extension, states):
    name = f"AirScout-RS-{tag}-{slug}.{extension}"
    require(entries(directory) == {"assets", "result.json"}, f"Unexpected layout: {directory}")
    require(entries(directory / "assets") == {name}, f"Unexpected asset roster: {directory}")
    result = read_json(directory / "result.json")
    expected = {"format": fmt, "slug": slug, "assets": [name], "signing": result["signing"]}
    if fmt in {"deb", "rpm", "archlinux", "appimage"}:
        floor = result["glibc_floor"]
        require(re.fullmatch(r"[0-9]+\.[0-9]+(?:\.[0-9]+)?", floor),
                f"Invalid glibc floor: {directory}")
        if slug in {"linux-x86_64", "linux-arm64"}:
            require(tuple(map(int, floor.split("."))) <= (2, 35, 0),
                    f"glibc floor above 2.35: {directory}")
        expected["glibc_floor"] = floor
    require(result == expected and result["signing"] in states,
            f"Invalid package result: {directory}")
    return file(directory / "assets" / name), result


def flatpak_assets(root, channel):
    directory = root / "package-flatpak-repo"
    if channel == "alpha":
        require(not directory.exists(), "Alpha must have no Flatpak repository candidate")
        return {}, False

    require(entries(directory) == {"repo", "repo.result.json"},
            "Unexpected Flatpak repository artifact layout")
    repo = directory / "repo"
    result = read_json(directory / "repo.result.json")
    ref = f"{APP_ID}{'.beta' if channel == 'beta' else ''}.flatpakref"
    signed = result["signing"] == "verified"
    names = ["airscout-rs.flatpakrepo", ref] if signed else []
    require(result == {"format": "flatpak", "channel": channel, "assets": names,
                       "signing": "verified" if signed else "missing"},
            "Invalid Flatpak repository result")
    require("summary" in entries(repo), "Flatpak repository is missing its summary")
    file(repo / "summary")
    if signed:
        file(repo / "summary.sig")
    # The inherited OSTree tree can still contain the other channel's descriptors.
    return {name: file(repo / name) for name in names}, signed


def validate_container(directory, version):
    require(entries(directory) == {"images", "index.json", "result.json"},
            "Unexpected container artifact layout")
    require(entries(directory / "images") == {f"{arch}.tar" for arch in PLATFORMS},
            "Unexpected container image roster")
    result = read_json(directory / "result.json")
    images = result["images"]
    require(result == {"format": "container", "version": version,
                       "platforms": sorted(PLATFORMS.values()), "images": images,
                       "assets": ["index.json"], "signing": "not_applicable"}
            and set(images) == {f"linux-{arch}" for arch in PLATFORMS},
            "Invalid container result")
    manifests = {}
    for arch, platform in PLATFORMS.items():
        image = images[f"linux-{arch}"]
        digest = image["digest"]
        tarball = f"images/{arch}.tar"
        require(re.fullmatch(r"sha256:[0-9a-f]{64}", digest)
                and image == {"platform": platform, "digest": digest, "tarball": tarball},
                f"Invalid container image descriptor: {arch}")
        file(directory / tarball)
        manifests[platform] = digest
    index = read_json(directory / "index.json")
    indexed = {}
    for manifest in index["manifests"]:
        platform = manifest["platform"]
        key = "/".join(platform[item] for item in ("os", "architecture", "variant")
                       if item in platform)
        require(key not in indexed, f"Duplicate container platform: {key}")
        indexed[key] = manifest["digest"]
    require(index["schemaVersion"] == 2
            and index["mediaType"] == "application/vnd.oci.image.index.v1+json"
            and indexed == manifests, "Container index and result disagree")


def collect(root, tag, output, source=None):
    version, channel = tag_info(tag)
    names = entries(root)
    expected = {"package-container", "package-flatpak-x86_64", "package-flatpak-aarch64"}
    if channel != "alpha":
        expected.add("package-flatpak-repo")

    assets = {}

    def add(path, result):
        require(path.name not in assets, f"Duplicate public basename: {path.name}")
        assets[path.name] = (path, result)

    def package(fmt, slug, extension, states):
        artifact = f"package-{fmt}-{slug}"
        expected.add(artifact)
        return collect_package(root / artifact, tag, fmt, slug, extension, states)

    for arch in LINUX:
        formats = {"deb": "deb", "archlinux": "pkg.tar.zst", "appimage": "AppImage"}
        if arch in ("x86_64", "arm64"):
            formats["rpm"] = "rpm"
        for fmt, extension in formats.items():
            add(*package(fmt, f"linux-{arch}", extension, {"not_applicable"}))

    windows_signing = set()
    for arch in WINDOWS:
        for fmt in ("msi", "zip"):
            asset, result = package(fmt, f"windows-{arch}", fmt, {"missing", "verified"})
            windows_signing.add(result["signing"])
            add(asset, result)
    require(len(windows_signing) == 1, "Inconsistent Windows signing results")

    macos_signing = set()
    for arch in MACOS:
        asset, result = package("dmg", f"macos-{arch}", "dmg", {"unsigned", "signed"})
        macos_signing.add(result["signing"])
        add(asset, result)
    require(len(macos_signing) == 1, "Inconsistent macOS signing results")

    for arch, slug in FLATPAK.items():
        directory = root / f"package-flatpak-{arch}"
        require(entries(directory) == {"build.tar.gz", "result.json"}, f"Unexpected layout: {directory}")
        file(directory / "build.tar.gz")
        require(read_json(directory / "result.json") ==
                {"format": "flatpak", "slug": slug, "branch": channel,
                 "signing": "not_applicable", "assets": []}, f"Invalid Flatpak build result: {arch}")
    descriptors, flatpak_signed = flatpak_assets(root, channel)
    for path in descriptors.values():
        add(path, {"format": "flatpak", "signing": "verified"})
    validate_container(root / "package-container", version)
    require(names == expected, f"Unexpected candidate artifact roster: {sorted(names ^ expected)}")

    snapshot = file(Path(source)) if source is not None else None
    require(snapshot is not None, "Release is missing its source snapshot")
    require(snapshot.name == source_snapshot_name(version),
            f"Source snapshot must be named {source_snapshot_name(version)}")
    add(snapshot, {"format": "source", "signing": "not_applicable"})

    require(not output.exists() and not output.is_symlink(), f"Output already exists: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=output.parent, prefix=".release-assets-") as temporary:
        work = Path(temporary)
        public = work / "output"
        public.mkdir()
        inventory = {"tag": tag, "channel": channel, "version": version, "assets": [],
                     "windows_signed": windows_signing == {"verified"},
                     "macos_signed": macos_signing == {"signed"}, "flatpak_signed": flatpak_signed,
                     "checksums_signed": False}
        checksums = []
        for name, (source, result) in sorted(assets.items()):
            destination = public / name
            shutil.copyfile(source, destination)
            digest = sha256(destination)
            checksums.append(f"{digest}  {name}\n")
            inventory["assets"].append({
                "name": name, "sha256": digest,
                **{key: result[key] for key in ("format", "slug", "signing", "glibc_floor")
                   if key in result},
            })
        (public / "SHA256SUMS").write_text("".join(checksums), encoding="utf-8")
        (public / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n", encoding="utf-8")
        public.rename(output)


def main():
    if len(sys.argv) != 5:
        sys.exit("Usage: collect.py <candidate-root> <tag> <output-dir> <source-snapshot>")
    root, tag, output, source = sys.argv[1:]
    os.umask(0o022)
    try:
        collect(Path(root), tag, Path(output), source)
    except (OSError, TypeError, KeyError, ValueError) as error:
        sys.exit(f"collect.py: {error}")


if __name__ == "__main__":
    main()
