import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from test_release import ROOT, VERSION, WORKFLOWS, run, workflow_jobs, yaml

RENDER = ROOT / "packaging/winget/render.py"
ID = "HamRadioOnline.AirScout-RS"
TAG = f"v{VERSION}"
ARCHES = {"x64": "windows-x86_64", "arm64": "windows-arm64", "x86": "windows-i686"}
PRODUCT = {arch: f"{{12345678-0000-4444-8888-{index:012d}}}"
           for index, arch in enumerate(ARCHES, 1)}
UPGRADE = "{5CEC7FA4-CCAD-467A-91D1-759E51F78ADD}"


def renderer():
    spec = importlib.util.spec_from_file_location("winget_render", RENDER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fixture(directory):
    assets = []
    for arch, slug in ARCHES.items():
        name = f"AirScout-RS-{TAG}-{slug}.msi"
        data = f"synthetic MSI for {arch}".encode()
        (directory / name).write_bytes(data)
        assets.append(dict(name=name, sha256=hashlib.sha256(data).hexdigest(),
                           format="msi", slug=slug, signing="verified"))
    checksums = directory / "SHA256SUMS"
    checksums.write_text("".join(f"{asset['sha256']}  {asset['name']}\n" for asset in assets))
    (directory / "inventory.json").write_text(json.dumps({
        "tag": TAG, "version": VERSION, "channel": "stable",
        "windows_signed": True, "assets": assets,
    }))
    return checksums, assets


def msi_identity(path):
    arch = next(arch for arch, slug in ARCHES.items() if path.stem.endswith(slug))
    return {"ProductCode": PRODUCT[arch], "UpgradeCode": UPGRADE,
            "ProductName": "AirScout-RS", "Manufacturer": "HamRadioOnline",
            "ProductVersion": VERSION}


class WingetTests(unittest.TestCase):
    def test_stable_manifest(self):
        module = renderer()
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            checksums, assets = fixture(directory)
            with patch.object(module, "msi_properties", side_effect=msi_identity):
                module.render(TAG, checksums, directory / "output")
            manifests = directory / "output/manifests/h/HamRadioOnline/AirScout-RS" / VERSION
            self.assertEqual({path.name for path in manifests.iterdir()},
                             {f"{ID}.yaml", f"{ID}.installer.yaml", f"{ID}.locale.en-US.yaml"})
            contents = {path.name: path.read_text() for path in manifests.iterdir()}
            for source in contents.values():
                self.assertIn(f"PackageIdentifier: {ID}\n", source)
                self.assertIn(f"PackageVersion: {VERSION}\n", source)
            installers = contents[f"{ID}.installer.yaml"]
            self.assertIn("InstallerType: msi\n", installers)
            self.assertIn("Scope: machine\n", installers)
            self.assertIn("DefaultLocale: en-US\n", contents[f"{ID}.yaml"])
            self.assertIn("Publisher: HamRadioOnline\n", contents[f"{ID}.locale.en-US.yaml"])
            blocks = installers.split("- Architecture: ")[1:]
            self.assertEqual(len(blocks), len(ARCHES))
            self.assertEqual({block.splitlines()[0] for block in blocks}, set(ARCHES))
            for block in blocks:
                arch = block.splitlines()[0]
                asset = next(asset for asset in assets if asset["slug"] == ARCHES[arch])
                self.assertIn(f"InstallerUrl: https://github.com/HamRadioOnline/AirScout-RS/"
                              f"releases/download/{TAG}/{asset['name']}\n", block)
                self.assertIn(f"InstallerSha256: {asset['sha256'].upper()}\n", block)
                self.assertIn(f"ProductCode: '{PRODUCT[arch]}'\n", block)
                self.assertIn(f"UpgradeCode: '{UPGRADE}'\n", block)
            if yaml:
                self.assertEqual(set(entry["Architecture"] for entry in
                                     yaml.safe_load(installers)["Installers"]), set(ARCHES))

    def test_unverified_inputs_are_rejected(self):
        module = renderer()
        for defect in ("alpha", "beta", "bytes", "duplicate", "unsigned", "inventory-hash"):
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as tmp:
                directory = Path(tmp)
                sums, assets = fixture(directory)
                inv_path = directory / "inventory.json"
                inv = json.loads(inv_path.read_text())
                tag = TAG
                if defect in {"alpha", "beta"}:
                    tag = f"{defect}-{TAG}-test"
                elif defect == "bytes":
                    (directory / assets[0]["name"]).write_bytes(b"tampered")
                elif defect == "duplicate":
                    sums.write_text(sums.read_text() * 2)
                elif defect == "unsigned":
                    inv["assets"][0]["signing"] = "missing"
                else:
                    inv["assets"][0]["sha256"] = "a" * 64
                inv_path.write_text(json.dumps(inv))
                with patch.object(module, "msi_properties", side_effect=msi_identity):
                    with self.assertRaises(ValueError):
                        module.render(tag, sums, directory / "output")
                self.assertFalse(list(directory.rglob("*.yaml")))

    def test_no_pr_without_signing_or_token(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "github-output"
            for signed, token, expected in (("true", "", 0), ("false", "fake", 0),
                                            ("false", "fake\ntoken", 0),
                                            ("true", "fake\ntoken", 1), ("True", "fake", 1)):
                with self.subTest(signed=signed, token_set=bool(token)):
                    output.write_text("")
                    env = dict(os.environ, WINDOWS_SIGNED=signed, WINGET_TOKEN=token,
                               GITHUB_OUTPUT=str(output))
                    result = run(sys.executable, RENDER, "publish", VERSION, tmp, env=env)
                    self.assertEqual(result.returncode, expected, result.stderr)
                    self.assertEqual(output.read_text(), "published=false\n")
                    if not expected:
                        self.assertIn("skipped", result.stdout)
                    self.assertNotIn("fake", result.stdout + result.stderr)

    def test_workflow_gates_and_secret_boundary(self):
        caller = workflow_jobs("release.yml")["publish-winget"]
        self.assertIn("release", caller["needs"])
        self.assertEqual(caller["if"],
                         "${{ success() && needs.release.outputs.winget_eligible == 'true' }}")
        source = (WORKFLOWS / "release-channels.yml").read_text()
        jobs = workflow_jobs("release-channels.yml")
        job = jobs["winget"]
        self.assertEqual(job["environment"], "release")
        self.assertEqual(job["runs-on"], "windows-2025")
        self.assertEqual({name for name, value in jobs.items()
                          if "secrets.WINGET_TOKEN" in str(value)}, {"winget"})
        if yaml:
            call = yaml.load(source, Loader=yaml.BaseLoader)["on"]["workflow_call"]
            self.assertEqual(call["secrets"]["WINGET_TOKEN"], {"required": "false"})
            self.assertIn("|| 'false'", call["outputs"]["winget_published"]["value"])
            self.assertEqual(caller["with"]["windows_signed"],
                             "${{ needs.release.outputs.windows_signed }}")
            steps = job["steps"]
            for key in ("Render verified MSI manifests", "Validate manifests with pinned WinGet",
                        "Open or reuse the stable PR"):
                step = next(step for step in steps if step.get("name") == key)
                self.assertIn("steps.gate.outputs.eligible == 'true'", step["if"])
            self.assertNotIn("env", next(step for step in steps
                                        if step.get("name") == "Validate manifests with pinned WinGet"))
            self.assertIn("release-assets-verified", [step.get("with", {}).get("name") for step in steps])
        else:
            self.assertIn("WINGET_TOKEN:\n        required: false", source.split("\njobs:", 1)[0])
            self.assertIn("jobs.winget.outputs.published || 'false'", source)
            self.assertIn("windows_signed: ${{ needs.release.outputs.windows_signed }}", caller["source"])
            self.assertGreaterEqual(job["source"].count("steps.gate.outputs.eligible == 'true'"), 4)
            self.assertIn("name: release-assets-verified", job["source"])

    def test_publication_pin(self):
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        for pin in lock["winget-cli"].values():
            self.assertRegex(pin["sha256"], r"^[0-9a-f]{64}$")
            self.assertIn(f"/v{pin['version']}/", pin["url"])
            self.assertTrue(pin["source"].startswith("https://api.github.com/repos/microsoft/winget-cli/"))

    def test_new_pr_and_matching_retry(self):
        from base64 import b64decode
        from urllib.parse import urlencode

        module = renderer()
        branch = f"airscout-rs-v{VERSION}"
        base_oid, manifest_oid = "1" * 40, "2" * 40
        fork = f"repos/{module.FORK}"
        upstream = f"repos/{module.UPSTREAM}"
        ref = f"git/ref/heads/{branch}"
        contents = f"contents/{module.manifest_path(VERSION).as_posix()}"
        manifests = {f"{module.manifest_path(VERSION).as_posix()}/{name}": f"{name}\n".encode()
                     for name in module.NAMES}
        files = [{"filename": path, "status": "added",
                  "sha": hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest()}
                 for path, content in manifests.items()]
        commit = {"commit": {"verification": {"verified": True}}, "parents": [{"sha": base_oid}],
                  "files": files}
        head_label = f"HamRadioOnline:{branch}"
        filters = urlencode({"state": "all", "head": head_label, "per_page": 2})
        lookup = f"{upstream}/pulls?{filters}"
        head = {"label": head_label, "sha": manifest_oid}
        repo = {"fork": True, "parent": {"full_name": module.UPSTREAM, "default_branch": "master"},
                "default_branch": "master"}
        verified = {"isValid": True, "state": "VALID", "wasSignedByGitHub": True}
        calls = []

        def expect(path, result, **kwargs):
            calls.append((path, kwargs, result))

        def request(path, **kwargs):
            self.assertTrue(calls, f"Unexpected API call: {path}")
            expected_path, expected_kwargs, result = calls.pop(0)
            self.assertEqual(path, expected_path)
            if path == "graphql":
                self.assertEqual(set(kwargs), {"post"})
                post = kwargs["post"]
                self.assertIn("createCommitOnBranch(input: $input)", post["query"])
                data = post["variables"]["input"]
                self.assertEqual(data["branch"], {"repositoryNameWithOwner": module.FORK, "branchName": branch})
                self.assertEqual(data["expectedHeadOid"], base_oid)
                self.assertEqual({item["path"]: b64decode(item["contents"], validate=True)
                                  for item in data["fileChanges"]["additions"]}, manifests)
            else:
                self.assertEqual(kwargs, expected_kwargs)
            if isinstance(result, Exception):
                raise result
            return result

        with patch.object(module.GitHub, "request", side_effect=request):
            with self.subTest("missing fork"):
                expect(fork, None, missing=True)
                with self.assertRaisesRegex(ValueError, "Owner must prepare"):
                    module.submit(module.GitHub(), VERSION, manifests)
                self.assertFalse(calls)

            def expect_new_branch(merge_type):
                # Sync the fork's default branch onto upstream first, then read
                # the fork's default-branch head and anchor the version branch
                # there rather than on the (possibly stale) upstream head.
                expect(fork, repo, missing=True)
                expect(lookup, [])
                expect(f"{fork}/merge-upstream", {"merge_type": merge_type},
                       post={"branch": "master"})
                upstream_ref = {"object": {"sha": base_oid}}
                expect(f"{upstream}/git/ref/heads/master", upstream_ref)
                expect(f"{fork}/git/ref/heads/master", {"object": {"sha": base_oid}})
                expect(f"{upstream}/{contents}?ref={base_oid}", None, missing=True)
                expect(f"{fork}/{ref}", None, missing=True)
                expect(f"{fork}/{contents}?ref={base_oid}", None, missing=True)
                expect(f"{fork}/git/refs", {}, post={"ref": f"refs/heads/{branch}", "sha": base_oid})

            for signature in (dict(verified, isValid=False), verified):
                with self.subTest("create", signature=signature):
                    expect_new_branch("fast-forward")
                    expect("graphql", {"data": {"createCommitOnBranch": {
                        "commit": {"oid": manifest_oid, "signature": signature},
                    }}})
                    if signature["isValid"]:
                        expect(f"{fork}/commits/{manifest_oid}", commit)
                        expect(f"{upstream}/pulls", {"number": 123, "head": head}, post={
                            "title": f"Add {ID} {VERSION}", "head": head_label, "base": "master",
                            "body": f"Stable release: https://github.com/HamRadioOnline/AirScout-RS/releases/tag/{TAG}",
                            "maintainer_can_modify": False,
                        })
                        with patch("builtins.print") as logged:
                            module.submit(module.GitHub(), VERSION, manifests)
                            logged.assert_called_once_with(f"WinGet PR: https://github.com/{module.UPSTREAM}/pull/123")
                    else:
                        with self.assertRaisesRegex(ValueError, "lacks a verified GitHub signature"):
                            module.submit(module.GitHub(), VERSION, manifests)
                    self.assertFalse(calls)

            with self.subTest("already-synced"):
                expect_new_branch("none")
                expect("graphql", {"data": {"createCommitOnBranch": {
                    "commit": {"oid": manifest_oid, "signature": verified},
                }}})
                expect(f"{fork}/commits/{manifest_oid}", commit)
                expect(f"{upstream}/pulls", {"number": 123, "head": head}, post={
                    "title": f"Add {ID} {VERSION}", "head": head_label, "base": "master",
                    "body": f"Stable release: https://github.com/HamRadioOnline/AirScout-RS/releases/tag/{TAG}",
                    "maintainer_can_modify": False,
                })
                with patch("builtins.print") as logged:
                    module.submit(module.GitHub(), VERSION, manifests)
                    logged.assert_called_once_with(f"WinGet PR: https://github.com/{module.UPSTREAM}/pull/123")
                self.assertFalse(calls)

            for kind, result in (("merge", {"merge_type": "merge"}),
                                 ("conflict", module.GitHubHttpError(409))):
                with self.subTest("diverged", kind=kind):
                    expect(fork, repo, missing=True)
                    expect(lookup, [])
                    expect(f"{fork}/merge-upstream", result, post={"branch": "master"})
                    with self.assertRaisesRegex(ValueError, "sync the fork manually"):
                        module.submit(module.GitHub(), VERSION, manifests)
                    self.assertFalse(calls)

            for state, merged, matching in (("open", False, True), ("closed", True, True),
                                             ("closed", False, True), ("open", False, False)):
                with self.subTest("retry", state=state, merged=merged, matching=matching):
                    expect(fork, repo, missing=True)
                    expect(lookup, [{"number": 123}])
                    expect(f"{upstream}/pulls/123", {"state": state, "merged": merged, "head": head,
                                                    "base": {"ref": "master"}, "commits": 1})
                    if state == "closed" and not merged:
                        with self.assertRaisesRegex(ValueError, "closed without merging"):
                            module.submit(module.GitHub(), VERSION, manifests)
                    else:
                        expect(f"{upstream}/commits/{manifest_oid}", commit)
                        if matching:
                            with patch("builtins.print"):
                                module.submit(module.GitHub(), VERSION, manifests)
                        else:
                            altered = {path: data + b"\n" for path, data in manifests.items()}
                            with self.assertRaisesRegex(ValueError, "does not match"):
                                module.submit(module.GitHub(), VERSION, altered)
                    self.assertFalse(calls)

    def test_sync_fork_distinguishes_api_failures(self):
        from urllib.error import HTTPError

        module = renderer()
        secret = "synthetic-credential"
        with patch.dict(os.environ, WINGET_TOKEN=secret), patch.object(module, "urlopen") as open_url:
            for status, message in ((401, "authentication or token permissions"),
                                    (403, "authentication or token permissions"),
                                    (409, "sync the fork"),
                                    (422, "operation failed"),
                                    (500, "retry"),
                                    (503, "retry")):
                with self.subTest(status=status):
                    open_url.side_effect = HTTPError("https://api.github.com", status, secret, {}, None)
                    with self.assertRaisesRegex(ValueError, message) as raised:
                        module.sync_fork(module.GitHub(), "main")
                    self.assertNotIn(secret, str(raised.exception))
                    if status != 409:
                        self.assertNotIn("sync the fork", str(raised.exception))

    def test_api_redacts_sensitive_errors(self):
        from http.client import IncompleteRead
        from urllib.error import HTTPError

        module = renderer()
        secret = "synthetic-credential"
        with patch.dict(os.environ, WINGET_TOKEN=secret), patch.object(module, "urlopen") as open_url:
            open_url.side_effect = HTTPError("https://api.github.com", 403, secret, {}, None)
            with self.assertRaisesRegex(ValueError, r"^WinGet GitHub API check authentication or token permissions \(HTTP 403\)$"):
                module.GitHub().request("test")
            request = open_url.call_args.args[0]
            self.assertEqual(request.get_method(), "GET")
            self.assertEqual(request.headers["Authorization"], f"Bearer {secret}")

            # A transport-level http.client error must be redacted the same way.
            open_url.side_effect = IncompleteRead(secret.encode())
            with self.assertRaisesRegex(ValueError, r"^WinGet GitHub API operation failed$") as raised:
                module.GitHub().request("test")
            self.assertNotIn(secret, str(raised.exception))
