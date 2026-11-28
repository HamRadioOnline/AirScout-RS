#!/usr/bin/env python3
"""Render one release cask or publish it from the verified release payload."""
import base64
import json
import os
from pathlib import Path
import re
from string import Template
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "release"))
from collect import file, read_json, require, sha256, tag_info

TAP = "HamRadioOnline/homebrew-tap"
DMG_SLUGS = {"arm": "macos-arm64", "intel": "macos-x86_64"}


def dmg_hashes(tag, manifest):
    names = {f"AirScout-RS-{tag}-{slug}.dmg" for slug in DMG_SLUGS.values()}
    checksums = {}
    for line in file(manifest).read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([a-zA-Z0-9_.-]+)", line)
        require(match, "Malformed SHA256SUMS entry")
        digest, name = match.groups()
        if name in names:
            require(name not in checksums, f"Duplicate DMG hash: {name}")
            checksums[name] = digest
    require(checksums.keys() == names, "SHA256SUMS is missing a required DMG hash")
    return checksums


def render(tag, checksums):
    version, channel = tag_info(tag)
    require(channel != "alpha", "Alpha releases have no Homebrew cask")
    token = "airscout-rs@beta" if channel == "beta" else "airscout-rs"
    template = Template((HERE / f"{token}.rb.in").read_text(encoding="utf-8"))
    hashes = {key: checksums[f"AirScout-RS-{tag}-{slug}.dmg"] for key, slug in DMG_SLUGS.items()}
    return f"Casks/{token}.rb", template.substitute(
        tag=tag, version=tag if channel == "beta" else version, **hashes)


def verify_dmgs(tag, directory, checksums):
    version, channel = tag_info(tag)
    inventory = read_json(directory / "inventory.json")
    require((inventory["tag"], inventory["version"], inventory["channel"]) ==
            (tag, version, channel), "Release inventory does not match the cask tag")
    require(inventory["macos_signed"] is True, "Release inventory lacks verified DMGs")
    dmgs = [asset for asset in inventory["assets"] if asset["format"] == "dmg"]
    require(len(dmgs) == len(checksums) and {asset["name"] for asset in dmgs} == checksums.keys(),
            "Unexpected DMG roster in release inventory")
    for asset in dmgs:
        name = asset["name"]
        require(asset["signing"] == "signed", f"DMG signing result is missing: {name}")
        require(asset["sha256"] == checksums[name] == sha256(file(directory / name)),
                f"DMG hash differs from the release inventory: {name}")


def graphql(query, variables):
    env = dict(os.environ)
    env["GH_TOKEN"] = env.pop("HOMEBREW_TAP_TOKEN")
    # The CLI and API can echo credentials in errors. Only fixed diagnostics
    # leave this boundary, even if the configured token has multiple lines.
    result = subprocess.run(
        ["gh", "api", "--hostname", "github.com", "--method", "POST", "graphql", "--input", "-"],
        env=env, input=json.dumps({"query": query, "variables": variables}),
        text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    require(result.returncode == 0, "Tap GitHub API operation failed")
    response = json.loads(result.stdout)
    require(not response.get("errors") and response.get("data"), "Tap GitHub API operation failed")
    return response["data"]


def publish_cask(tag, path, source):
    branch = graphql('''
        query {
          repository(owner: "HamRadioOnline", name: "homebrew-tap") {
            defaultBranchRef { name target { oid } }
          }
        }
    ''', {})["repository"]["defaultBranchRef"]
    require(branch, "Homebrew tap must have an initialized default branch")
    head = branch["target"]["oid"]
    require(re.fullmatch(r"[0-9a-f]{40}", head), "Invalid Homebrew tap head")

    existing = graphql('''
        query($expression: String!) {
          repository(owner: "HamRadioOnline", name: "homebrew-tap") {
            object(expression: $expression) {
              __typename
              ... on Blob { isTruncated text }
            }
          }
        }
    ''', {"expression": f"{head}:{path}"})["repository"]["object"]
    if existing is not None:
        require(existing["__typename"] == "Blob", "Homebrew tap cask path is not a file")
        if not existing["isTruncated"] and existing["text"] == source:
            print(f"Homebrew cask already matches {tag}")
            return

    # GitHub signs the single-file commit and appends it only if the branch
    # still points to the exact commit used to read the existing cask.
    commit = graphql('''
        mutation($input: CreateCommitOnBranchInput!) {
          createCommitOnBranch(input: $input) {
            commit {
              oid
              parents(first: 2) { nodes { oid } }
              signature { isValid state wasSignedByGitHub }
            }
          }
        }
    ''', {"input": {
        "branch": {"repositoryNameWithOwner": TAP, "branchName": branch["name"]},
        "expectedHeadOid": head,
        "message": {"headline": f"Update {Path(path).stem} for {tag}"},
        "fileChanges": {"additions": [
            {"path": path, "contents": base64.b64encode(source.encode("utf-8")).decode("ascii")},
        ]},
    }})["createCommitOnBranch"]["commit"]
    require(commit["parents"]["nodes"] == [{"oid": head}], "Tap commit has an unexpected parent")
    require(commit["signature"] == {"isValid": True, "state": "VALID", "wasSignedByGitHub": True},
            "Tap commit does not have a verified GitHub signature")
    require(re.fullmatch(r"[0-9a-f]{40}", commit["oid"]), "Invalid published tap commit ID")
    print(f"Published {path} in verified tap commit {commit['oid']}")


def publish(tag, directory):
    published = False
    try:
        signed = os.environ["MACOS_SIGNED"]
        require(signed in ("true", "false"), "Invalid macOS signing output")
        token = os.environ.get("HOMEBREW_TAP_TOKEN", "")
        if signed == "false" or not token:
            reason = "macOS DMGs are unsigned" if signed == "false" else "HOMEBREW_TAP_TOKEN is missing"
            print(f"Homebrew publication skipped: {reason}")
            return
        require(token.isascii() and not any(character.isspace() for character in token),
                "HOMEBREW_TAP_TOKEN must be a single token")
        checksums = dmg_hashes(tag, directory / "SHA256SUMS")
        path, source = render(tag, checksums)
        verify_dmgs(tag, directory, checksums)
        publish_cask(tag, path, source)
        published = True
    finally:
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write(f"published={'true' if published else 'false'}\n")


def main():
    require(len(sys.argv) == 4,
            "Usage: render.py <tag> <SHA256SUMS> <output-dir> or render.py publish <tag> <asset-dir>")
    first, second, third = sys.argv[1:]
    if first == "publish":
        publish(second, Path(third))
    else:
        path, source = render(first, dmg_hashes(first, Path(second)))
        destination = Path(third) / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(source, encoding="utf-8")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError) as error:
        sys.exit(f"Homebrew: {error}")
