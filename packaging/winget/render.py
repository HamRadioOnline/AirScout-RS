#!/usr/bin/env python3
"""Render and submit stable WinGet manifests for the verified MSI payload."""
from base64 import b64encode
import ctypes
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))
from collect import file, read_json, require, sha256, tag_info

PUBLISHER = "HamRadioOnline"
APP = "AirScout-RS"
PACKAGE = f"{PUBLISHER}.{APP}"
REPOSITORY = f"https://github.com/{PUBLISHER}/{APP}"
ARCHES = {"x64": "windows-x86_64", "arm64": "windows-arm64", "x86": "windows-i686"}
FORK = "HamRadioOnline/winget-pkgs"
UPSTREAM = "microsoft/winget-pkgs"
SCHEMA = "1.6.0"
NAMES = (f"{PACKAGE}.yaml", f"{PACKAGE}.installer.yaml", f"{PACKAGE}.locale.en-US.yaml")
MSI_PROPERTIES = ("ProductName", "Manufacturer", "ProductVersion", "ProductCode", "UpgradeCode")


def manifest_path(version):
    return Path("manifests/h") / PUBLISHER / APP / version


def stable_version(tag):
    version, channel = tag_info(tag)
    require(channel == "stable", "WinGet accepts only stable release tags")
    return version


def verify_msis(tag, version, sums):
    directory = sums.parent
    inventory = read_json(directory / "inventory.json")
    require((inventory["tag"], inventory["version"], inventory["channel"]) ==
            (tag, version, "stable"), "Release inventory does not match the stable tag")
    require(inventory["windows_signed"] is True, "Release inventory lacks verified Windows assets")
    names = {f"{APP}-{tag}-{slug}.msi": arch for arch, slug in ARCHES.items()}
    checksums = {}
    for line in file(sums).read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([a-zA-Z0-9_.-]+)", line)
        require(match, "Malformed SHA256SUMS entry")
        digest, name = match.groups()
        require(name not in checksums, f"Duplicate checksum: {name}")
        checksums[name] = digest

    assets = [asset for asset in inventory["assets"] if asset["format"] == "msi"]
    require(len(assets) == len(names) and {asset["name"] for asset in assets} == names.keys(),
            "Unexpected MSI roster in release inventory")
    verified = {}
    for asset in assets:
        name = asset["name"]
        arch = names[name]
        path = file(directory / name)
        require(asset["slug"] == ARCHES[arch] and asset["signing"] == "verified",
                f"Invalid MSI architecture or signing result: {name}")
        require(asset["sha256"] == checksums.get(name) == sha256(path),
                f"MSI hash differs from the release inventory or SHA256SUMS: {name}")
        verified[arch] = (path, asset["sha256"].upper())
    return verified


def msi_properties(path):
    """Read the Property table of the signed MSI in Windows Installer read-only mode."""
    require(sys.platform == "win32", "MSI metadata extraction requires Windows")
    msi = ctypes.WinDLL("msi")
    handle = ctypes.c_uint
    database, view = handle(), handle()
    properties = {}
    try:
        require(msi.MsiOpenDatabaseW(str(path.resolve()), None, ctypes.byref(database)) == 0,
                f"Cannot open MSI database: {path.name}")
        condition = " OR ".join(f"`Property` = '{name}'" for name in MSI_PROPERTIES)
        query = f"SELECT `Property`, `Value` FROM `Property` WHERE {condition}"
        require(msi.MsiDatabaseOpenViewW(database, query, ctypes.byref(view)) == 0
                and msi.MsiViewExecute(view, 0) == 0, f"Cannot read MSI properties: {path.name}")
        while True:
            record = handle()
            status = msi.MsiViewFetch(view, ctypes.byref(record))
            if status == 259:  # ERROR_NO_MORE_ITEMS
                return properties
            require(status == 0, f"Cannot read MSI property row: {path.name}")
            try:
                values = []
                for field in (1, 2):
                    buffer = ctypes.create_unicode_buffer(256)
                    length = handle(len(buffer))
                    require(msi.MsiRecordGetStringW(record, field, buffer, ctypes.byref(length)) == 0,
                            f"Cannot read MSI property value: {path.name}")
                    values.append(buffer.value)
                properties[values[0]] = values[1]
            finally:
                msi.MsiCloseHandle(record)
    finally:
        if view.value:
            msi.MsiCloseHandle(view)
        if database.value:
            msi.MsiCloseHandle(database)


def render(tag, sums, output):
    version = stable_version(tag)
    assets = verify_msis(tag, version, sums)
    installers = []
    product_codes = set()
    upgrade_codes = set()
    for arch in ARCHES:
        path, digest = assets[arch]
        properties = msi_properties(path)
        require((properties["ProductName"], properties["Manufacturer"], properties["ProductVersion"])
                == (APP, PUBLISHER, version), f"Unexpected MSI product identity: {path.name}")
        product, upgrade = (properties[key].upper() for key in ("ProductCode", "UpgradeCode"))
        guid = r"\{[0-9A-F]{8}-(?:[0-9A-F]{4}-){3}[0-9A-F]{12}\}"
        require(re.fullmatch(guid, product) and re.fullmatch(guid, upgrade),
                f"Invalid MSI product or upgrade code: {path.name}")
        product_codes.add(product)
        upgrade_codes.add(upgrade)
        installers.append(f"""- Architecture: {arch}
  InstallerUrl: {REPOSITORY}/releases/download/{tag}/{path.name}
  InstallerSha256: {digest}
  ProductCode: '{product}'
  AppsAndFeaturesEntries:
  - DisplayName: {APP}
    Publisher: {PUBLISHER}
    ProductCode: '{product}'
    UpgradeCode: '{upgrade}'
""")
    require(len(product_codes) == len(ARCHES) and len(upgrade_codes) == 1,
            "MSIs must have distinct product codes and one common upgrade code")
    directory = output / manifest_path(version)
    directory.mkdir(parents=True, exist_ok=True)
    for name, kind, body in (
        (NAMES[0], "version", "DefaultLocale: en-US\n"),
        (NAMES[1], "installer", "InstallerType: msi\nScope: machine\nInstallers:\n" + "".join(installers)),
        (NAMES[2], "defaultLocale", f"""PackageLocale: en-US
Publisher: {PUBLISHER}
PackageName: {APP}
PackageUrl: {REPOSITORY}
License: GPL-3.0-only
ShortDescription: Amateur radio aircraft scatter prediction and tracking.
"""),
    ):
        source = (f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.{kind}.{SCHEMA}.schema.json\n"
                  f"PackageIdentifier: {PACKAGE}\nPackageVersion: {version}\n{body}"
                  f"ManifestType: {kind}\nManifestVersion: {SCHEMA}\n")
        (directory / name).write_bytes(source.encode("utf-8"))


def eligible(tag):
    signed = os.environ.get("WINDOWS_SIGNED")
    token = os.environ.get("WINGET_TOKEN", "")
    require(signed in ("true", "false"), "Invalid Windows signing output")
    _, channel = tag_info(tag)
    # Check the signing and channel state before the token syntax: an unsigned
    # or prerelease build must skip WinGet cleanly, never fail on the token.
    if channel != "stable":
        reason = "the release is a prerelease"
    elif signed == "false":
        reason = "Windows signing is incomplete"
    elif not token:
        reason = "WINGET_TOKEN is missing"
    else:
        require(re.fullmatch(r"[!-~]+", token), "WINGET_TOKEN must be a single token")
        return True
    print(f"WinGet publication skipped: {reason}")
    return False


def write_output(name, value):
    output = os.environ.get("GITHUB_OUTPUT")
    if output:
        with open(output, "a", encoding="utf-8") as handle:
            handle.write(f"{name}={'true' if value else 'false'}\n")


class GitHubHttpError(ValueError):
    def __init__(self, status):
        self.status = status
        if status in (401, 403):
            detail = "check authentication or token permissions"
        elif 500 <= status < 600:
            detail = "GitHub server error; retry the request"
        else:
            detail = "operation failed"
        super().__init__(f"WinGet GitHub API {detail} (HTTP {status})")


class GitHub:
    def request(self, path, *, post=None, missing=False):
        request = Request("https://api.github.com/" + path,
                          data=json.dumps(post).encode() if post is not None else None,
                          headers={"Authorization": "Bearer " + os.environ["WINGET_TOKEN"],
                                   "Accept": "application/vnd.github+json",
                                   "Content-Type": "application/json",
                                   "X-GitHub-Api-Version": "2022-11-28"})
        # Never surface response bodies, exception details or request headers.
        try:
            with urlopen(request, timeout=60) as response:
                return json.load(response)
        except HTTPError as error:
            if missing and error.code == 404:
                return None
            raise GitHubHttpError(error.code) from None
        except (URLError, TimeoutError, ValueError, http.client.HTTPException):
            raise ValueError("WinGet GitHub API operation failed") from None


def oid(value):
    require(re.fullmatch(r"[0-9a-f]{40}", value), "Invalid WinGet Git commit ID")
    return value


def check_commit(commit, blobs):
    require(commit["commit"]["verification"]["verified"] is True,
            "WinGet manifest commit lacks a verified signature")
    require(len(commit["parents"]) == 1, "WinGet manifest commit must have exactly one parent")
    changes = commit["files"]
    require(len(changes) == len(blobs)
            and {item["filename"]: item["sha"] for item in changes if item["status"] == "added"} == blobs,
            "Existing WinGet manifest commit does not match the verified release")
    return oid(commit["parents"][0]["sha"])


def check_base(api, ancestor, head):
    if ancestor != head:
        comparison = api.request(f"repos/{UPSTREAM}/compare/{ancestor}...{head}?per_page=1")
        require(comparison["status"] == "ahead",
                "WinGet branch has diverged from upstream. Check the owner-prepared fork and version branch.")


def sync_fork(api, default):
    """Fast-forward the fork default branch onto upstream before anchoring the
    version branch. A fork that cannot be synced (diverged or conflicting) is
    rejected so the owner syncs it manually instead of basing a stale branch."""
    message = "The WinGet fork is out of date; sync the fork manually"
    try:
        sync = api.request(f"repos/{FORK}/merge-upstream", post={"branch": default})
    except GitHubHttpError as error:
        if error.status == 409:
            raise ValueError(message) from None
        raise
    require(sync.get("merge_type") != "merge", message)
    require(sync.get("merge_type") in ("fast-forward", "none"),
            "WinGet GitHub API returned an unexpected fork sync result")
    return sync


def commit_manifests(api, branch, head, version, manifests):
    query = """
      mutation($input: CreateCommitOnBranchInput!) {
        createCommitOnBranch(input: $input) {
          commit { oid signature { isValid state wasSignedByGitHub } }
        }
      }
    """
    response = api.request("graphql", post={"query": query, "variables": {"input": {
        "branch": {"repositoryNameWithOwner": FORK, "branchName": branch},
        "expectedHeadOid": head,
        "message": {"headline": f"Add {PACKAGE} {version}"},
        "fileChanges": {"additions": [
            {"path": path, "contents": b64encode(content).decode("ascii")}
            for path, content in manifests.items()
        ]},
    }}})
    require(not response.get("errors") and response.get("data"), "WinGet signed commit operation failed")
    commit = response["data"]["createCommitOnBranch"]["commit"]
    require(commit["signature"] == {"isValid": True, "state": "VALID", "wasSignedByGitHub": True},
            "WinGet manifest commit lacks a verified GitHub signature")
    return oid(commit["oid"])


def submit(api, version, manifests):
    blobs = {path: hashlib.sha1(f"blob {len(data)}\0".encode() + data, usedforsecurity=False).hexdigest()
             for path, data in manifests.items()}
    fork = api.request(f"repos/{FORK}", missing=True)
    require(fork and fork["fork"] and fork.get("parent", {}).get("full_name") == UPSTREAM,
            f"Owner must prepare the {FORK} fork of {UPSTREAM} and grant WINGET_TOKEN access")
    base = fork["parent"]["default_branch"]
    branch = f"airscout-rs-v{version}"
    pr_head = f"{PUBLISHER}:{branch}"
    filters = urlencode({"state": "all", "head": pr_head, "per_page": 2})
    pulls = api.request(f"repos/{UPSTREAM}/pulls?{filters}")
    require(len(pulls) <= 1, "Multiple WinGet PRs already exist for this version")
    if pulls:
        number = int(pulls[0]["number"])
        pr = api.request(f"repos/{UPSTREAM}/pulls/{number}")
        require(pr["state"] == "open" or pr["merged"] is True,
                f"WinGet PR #{number} is closed without merging")
        require(pr["head"]["label"].lower() == pr_head.lower()
                and pr["base"]["ref"] == base and pr["commits"] == 1,
                f"WinGet PR #{number} has an unexpected head, base or commit count")
        head = oid(pr["head"]["sha"])
        check_commit(api.request(f"repos/{UPSTREAM}/commits/{head}"), blobs)
    else:
        default = fork["default_branch"]
        sync_fork(api, default)
        upstream_ref = api.request(f"repos/{UPSTREAM}/git/ref/heads/{quote(base, safe='')}")
        upstream = oid(upstream_ref["object"]["sha"])
        fork_ref = api.request(f"repos/{FORK}/git/ref/heads/{quote(default, safe='')}")
        fork_head = oid(fork_ref["object"]["sha"])
        manifest = manifest_path(version).as_posix()
        require(api.request(f"repos/{UPSTREAM}/contents/{manifest}?ref={upstream}", missing=True) is None,
                "This version already exists in WinGet and has no matching release PR")
        ref = api.request(f"repos/{FORK}/git/ref/heads/{branch}", missing=True)
        # Anchor the version branch on the fork's synced default-branch head, not
        # the upstream head, so POST /git/refs always resolves on the fork even
        # when it previously lagged microsoft/winget-pkgs.
        head = fork_head if ref is None else oid(ref["object"]["sha"])
        existing = api.request(f"repos/{FORK}/contents/{manifest}?ref={head}", missing=True)
        if existing is None:
            check_base(api, head, upstream)
            if ref is None:
                api.request(f"repos/{FORK}/git/refs", post={"ref": f"refs/heads/{branch}", "sha": head})
            seed = head
            head = commit_manifests(api, branch, seed, version, manifests)
            require(check_commit(api.request(f"repos/{FORK}/commits/{head}"), blobs) == seed,
                    "WinGet manifest commit has an unexpected parent")
        else:
            require(ref is not None, "The fork default branch already contains these WinGet manifests")
            parent = check_commit(api.request(f"repos/{FORK}/commits/{head}"), blobs)
            check_base(api, parent, upstream)
        pr = api.request(f"repos/{UPSTREAM}/pulls", post={
            "title": f"Add {PACKAGE} {version}", "head": pr_head, "base": base,
            "body": f"Stable release: {REPOSITORY}/releases/tag/v{version}",
            "maintainer_can_modify": False,
        })
        require(pr["head"]["sha"] == head, "WinGet PR head differs from the verified commit")
        number = int(pr["number"])
    print(f"WinGet PR: https://github.com/{UPSTREAM}/pull/{number}")


def publish(version, directory):
    published = False
    try:
        if not eligible(f"v{version}"):
            return
        path = manifest_path(version)
        require(directory.parts[-len(path.parts):] == path.parts,
                "Unexpected WinGet manifest directory for this version")
        require({item.name for item in directory.iterdir()} == set(NAMES), "Unexpected WinGet manifest files")
        submit(GitHub(), version, {(path / name).as_posix(): file(directory / name).read_bytes() for name in NAMES})
        published = True
    finally:
        write_output("published", published)


def main(args):
    if len(args) == 2 and args[0] == "gate":
        allowed = False
        try:
            allowed = eligible(args[1])
        finally:
            write_output("eligible", allowed)
    elif len(args) == 3 and args[0] == "publish":
        publish(args[1], Path(args[2]))
    elif len(args) == 3:
        render(args[0], Path(args[1]), Path(args[2]))
    else:
        raise ValueError("Usage: render.py <tag> <SHA256SUMS> <output-dir>")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except ValueError as error:
        sys.exit(f"WinGet: {error}")
    except (OSError, KeyError, TypeError, AttributeError):
        sys.exit("WinGet: invalid release input or unexpected GitHub response")
