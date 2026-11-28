#!/usr/bin/env python3
"""Render public release notes from the verified package inventory."""
import json
from pathlib import Path
import re
import sys


def render(inventory, tag_message, commits, github_notes):
    prs, rest, section = [], [], None
    for line in github_notes.splitlines():
        if re.match(r"^##\s+What's Changed", line):
            section = "prs"
            continue
        if line.startswith("## ") or line.startswith("**Full Changelog**"):
            section = "rest"
        if section == "prs":
            if line.strip():
                prs.append(line)
        elif line.strip() or rest:
            rest.append(line)

    parts = []
    if tag_message.strip():
        parts.append(tag_message.strip())
    if inventory["channel"] != "stable":
        parts.append("> **Pre-release.** These installers are intended for testing.")

    if commits.strip() or prs:
        parts.append("## What's Changed")
        if commits.strip():
            parts.append("### Commits\n" + commits.strip())
        if prs:
            parts.append("### Merged PRs\n" + "\n".join(prs))

    downloads = ["## Downloads", ""]
    labels = {"deb": "Debian package", "rpm": "RPM package", "archlinux": "Arch package",
              "appimage": "AppImage", "msi": "Windows installer", "zip": "Windows ZIP",
              "dmg": "macOS disk image", "flatpak": "Flatpak descriptor",
              "source": "Source archive"}
    for asset in inventory["assets"]:
        detail = labels[asset["format"]]
        if "glibc_floor" in asset:
            detail += f", glibc {asset['glibc_floor']} or newer"
        downloads.append(f"* `{asset['name']}` — {detail}")
    downloads.append("* `SHA256SUMS` — SHA-256 checksums of the files listed above")
    if inventory["checksums_signed"]:
        downloads.extend([
            "* `SHA256SUMS.asc` — detached OpenPGP signature",
            "* `SHA256SUMS.key.asc` — public OpenPGP key",
        ])
    parts.append("\n".join(downloads))

    windows = ("Certum signatures verified on EXEs and MSIs."
               if inventory["windows_signed"] else "Unsigned EXEs and MSIs.")
    macos = ("Developer ID signatures, notarization and stapling verified."
             if inventory["macos_signed"] else "Unsigned and unnotarized DMGs.")
    if inventory["flatpak_signed"]:
        flatpak = "Signed OSTree repository candidate."
    elif inventory["channel"] == "alpha":
        flatpak = "Channel skipped for alpha releases."
    else:
        flatpak = "Unsigned repository candidate. Descriptors are unavailable."
    checksums = ("OpenPGP signature verified. Compare the public key fingerprint "
                 f"`{inventory['checksum_fingerprint']}` with a trusted copy. Then run "
                 "`gpg --import SHA256SUMS.key.asc` and `gpg --verify SHA256SUMS.asc SHA256SUMS`."
                 if inventory["checksums_signed"] else "Unsigned. No OpenPGP signature is available.")
    parts.append("\n".join([
        "### Signing status", "",
        f"* Windows — {windows}",
        f"* macOS — {macos}",
        f"* Flatpak — {flatpak}",
        f"* `SHA256SUMS` — {checksums}",
        "",
        "Linux packages and AppImages are unsigned. Run `sha256sum -c SHA256SUMS` "
        "in the download directory to check the files.",
        "",
        "AirScout-RS is a port of AirScout by DL2ALF (GPL-3.0-only).",
    ]))
    if rest:
        parts.append("\n".join(rest).strip())
    return "\n\n".join(parts) + "\n"


def main():
    if len(sys.argv) != 6:
        sys.exit("Usage: notes.py <inventory> <tag-message> <commits> <github-notes> <output>")
    inventory, tag_message, commits, github_notes, output = map(Path, sys.argv[1:])
    text = render(json.loads(inventory.read_text(encoding="utf-8")),
                  tag_message.read_text(encoding="utf-8"), commits.read_text(encoding="utf-8"),
                  github_notes.read_text(encoding="utf-8"))
    output.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
