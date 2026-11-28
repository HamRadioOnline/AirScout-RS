#!/usr/bin/env python3
"""Publish an exact, verified asset roster through a resumable GitHub draft."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

from collect import file, read_json, require, sha256, tag_info

SIGNING = ("windows_signed", "macos_signed", "flatpak_signed", "checksums_signed")
GATES = ("homebrew_eligible", "winget_eligible", "flatpak_eligible", "aur_eligible",
         "container_eligible")


def publication_gates(channel, signed, aur_access):
    return {
        "homebrew_eligible": channel != "alpha" and signed["macos_signed"],
        "winget_eligible": channel == "stable" and signed["windows_signed"],
        "flatpak_eligible": channel != "alpha" and signed["flatpak_signed"],
        "aur_eligible": channel == "stable" and signed["checksums_signed"] and aur_access,
        "container_eligible": channel != "alpha",
    }


def gh(*args):
    return subprocess.check_output(["gh", *args], stdin=subprocess.DEVNULL)


def api(path, *args):
    return json.loads(gh("api", path, *args))


def pages(path):
    return [item for page in api(f"{path}?per_page=100", "--paginate", "--slurp")
            for item in page]


def tag_releases(endpoint, tag):
    matches = [release for release in pages(endpoint) if release["tag_name"] == tag]
    require(len(matches) <= 1, f"Multiple GitHub releases match {tag}")
    return matches


def create_draft(endpoint, tag, create, attempts=10, delay=3):
    """Create the draft and wait until the release list shows it.

    GitHub's list endpoint can lag a fresh create by a few seconds.
    """
    create()
    for attempt in range(attempts):
        matches = tag_releases(endpoint, tag)
        if matches:
            break
        if attempt + 1 < attempts:
            time.sleep(delay)
    require(len(matches) == 1 and matches[0]["draft"], f"No draft found after creating {tag}")
    return matches[0]


def public_assets(output, inventory):
    expected = {}
    checksums = []
    for asset in inventory["assets"]:
        name = asset["name"]
        require(re.fullmatch(r"[a-zA-Z0-9_.-]+", name) and name not in expected
                and name not in {"inventory.json", "SHA256SUMS", "SHA256SUMS.asc",
                                 "SHA256SUMS.key.asc"}, f"Invalid release asset name: {name}")
        digest = asset["sha256"]
        require(sha256(file(output / name)) == digest, f"Collected asset changed: {name}")
        expected[name] = digest
        checksums.append(f"{digest}  {name}\n")
    manifest = file(output / "SHA256SUMS")
    require(manifest.read_text(encoding="utf-8") == "".join(checksums),
            "Release checksum roster changed")
    expected["SHA256SUMS"] = sha256(manifest)
    if inventory["checksums_signed"]:
        for name in ("SHA256SUMS.asc", "SHA256SUMS.key.asc"):
            expected[name] = sha256(file(output / name))
    require({path.name for path in output.iterdir()} == set(expected) | {"inventory.json"},
            "Unexpected file in finalized release payload")
    return expected


def verify_signature(signature, output):
    # Signing timestamps can change on retry. Accept a different signature
    # only if it verifies against these exact checksums and the current key.
    with tempfile.TemporaryDirectory(prefix="release-signature-") as home:
        gpg = ["gpg", "--no-options", "--homedir", home, "--batch", "--no-tty", "--no-autostart"]

        def run(*args):
            subprocess.run([*gpg, *args], check=True, stdin=subprocess.DEVNULL,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        run("--import", str(output / "SHA256SUMS.key.asc"))
        run("--verify", str(signature), str(output / "SHA256SUMS"))


def verify_download(asset_api, name, output, expected):
    with tempfile.TemporaryDirectory(prefix="release-asset-") as temporary:
        path = Path(temporary) / name
        with path.open("wb") as stream:
            subprocess.run(["gh", "api", asset_api, "--header", "Accept: application/octet-stream"],
                           check=True, stdin=subprocess.DEVNULL, stdout=stream)
        if name == "SHA256SUMS.asc":
            verify_signature(path, output)
        else:
            require(sha256(path) == expected, f"GitHub release asset bytes differ: {name}")


def publish(repository, tag, output, notes, inventory):
    version, channel = tag_info(tag)
    require((inventory["tag"], inventory["version"], inventory["channel"]) ==
            (tag, version, channel), "Inventory does not match release tag")
    require(all(type(inventory[key]) is bool for key in SIGNING), "Invalid signing states")
    expected = public_assets(output, inventory)
    output = output.resolve()
    if inventory["checksums_signed"]:
        verify_signature(output / "SHA256SUMS.asc", output)
    title = f"{'Release' if channel == 'stable' else channel.title()} {tag}"
    prerelease = channel != "stable"
    endpoint = f"repos/{repository}/releases"
    flags = ("--prerelease",) if prerelease else ()
    notes_text = notes.read_text(encoding="utf-8").rstrip()

    def validate_release(release):
        require(release["tag_name"] == tag and release["name"] == title
                and release["prerelease"] is prerelease
                and (release["body"] or "").replace("\r\n", "\n").rstrip() == notes_text,
                f"Existing GitHub release metadata does not match {tag}")

    matches = tag_releases(endpoint, tag)
    if matches:
        release = matches[0]
    else:
        release = create_draft(endpoint, tag, lambda: gh(
            "release", "create", tag, "--repo", repository, "--verify-tag",
            "--draft", "--title", title, "--notes-file", str(notes), *flags))
    validate_release(release)
    release_api = f"{endpoint}/{release['id']}"

    def verified_assets(*, incomplete=False):
        found = set()
        for asset in pages(f"{release_api}/assets"):
            name = asset["name"]
            require(name in expected and name not in found, f"Unexpected GitHub release asset: {name}")
            asset_api = f"{endpoint}/assets/{asset['id']}"
            if incomplete and asset["state"] == "starter" and asset["size"] == 0:
                # GitHub can leave an empty starter after a failed upload.
                # Remove only that placeholder from a matching draft.
                gh("api", asset_api, "--method", "DELETE")
                continue
            require(asset["state"] == "uploaded", f"GitHub release asset is incomplete: {name}")
            digest = asset.get("digest")
            if not digest or (name == "SHA256SUMS.asc" and digest != f"sha256:{expected[name]}"):
                verify_download(asset_api, name, output, expected[name])
            else:
                require(digest == f"sha256:{expected[name]}",
                        f"GitHub release asset digest differs: {name}")
            found.add(name)
        if not incomplete:
            require(found == set(expected), f"GitHub release assets missing: {sorted(set(expected) - found)}")
        return found

    if release["draft"]:
        uploaded = verified_assets(incomplete=True)
        missing = sorted(set(expected) - uploaded)
        if missing:
            gh("release", "upload", tag, "--repo", repository, *(str(output / name) for name in missing))
        verified_assets()
        gh("release", "edit", tag, "--repo", repository, "--draft=false",
           "--latest=false" if prerelease else "--latest")
        release = api(release_api)
        validate_release(release)
        require(not release["draft"], f"Release is still a draft: {tag}")
    verified_assets()
    print(f"Verified published release {tag}")


def main():
    if len(sys.argv) != 4:
        sys.exit("Usage: publish.py <tag> <output-dir> <notes>")
    tag, output, notes = sys.argv[1:]
    flags = dict.fromkeys((*SIGNING, *GATES), False)
    try:
        output = Path(output)
        inventory = read_json(output / "inventory.json")
        aur_access = os.environ["AUR_ACCESS"]
        require(aur_access in {"true", "false"}, "Invalid AUR credential state")
        publish(os.environ["GITHUB_REPOSITORY"], tag, output, Path(notes), inventory)
        flags.update({key: inventory[key] for key in SIGNING})
        flags.update(publication_gates(inventory["channel"], flags, aur_access == "true"))
    except (OSError, TypeError, KeyError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"publish.py: {error}")
    finally:
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
            for key, value in flags.items():
                print(f"{key}={'true' if value else 'false'}", file=stream)


if __name__ == "__main__":
    main()
