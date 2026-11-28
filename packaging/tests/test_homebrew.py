import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from test_release import ROOT, VERSION, WORKFLOWS, run, workflow_jobs, yaml

HOMEBREW = ROOT / "packaging/homebrew"
RENDER = HOMEBREW / "render.py"
REPOSITORY = "https://github.com/HamRadioOnline/AirScout-RS"
PROGRAMS = ("airscout", "airscout-planeserver", "airscout-cli", "catcheck",
            "elevation-tile-gen", "maptile-gen")
HEAD = "1" * 40
NEXT = "2" * 40
SIGNATURE = {"isValid": True, "state": "VALID", "wasSignedByGitHub": True}


def dmg_fixture(directory, tag):
    assets = []
    for arch in ("arm64", "x86_64"):
        slug = f"macos-{arch}"
        name = f"AirScout-RS-{tag}-{slug}.dmg"
        data = f"synthetic DMG for {arch}".encode()
        (directory / name).write_bytes(data)
        assets.append(dict(name=name, sha256=hashlib.sha256(data).hexdigest(),
                           format="dmg", slug=slug, signing="signed"))
    manifest = directory / "SHA256SUMS"
    manifest.write_text("".join(f"{asset['sha256']}  {asset['name']}\n" for asset in assets))
    (directory / "inventory.json").write_text(json.dumps({
        "tag": tag, "version": VERSION, "channel": "beta" if tag.startswith("beta-") else "stable",
        "macos_signed": True, "assets": assets,
    }))
    return manifest, assets


class HomebrewTests(unittest.TestCase):
    def test_cask_selection(self):
        for tag, token, version in (
            (f"v{VERSION}", "airscout-rs", VERSION),
            (f"beta-v{VERSION}-rc.1", "airscout-rs@beta", f"beta-v{VERSION}-rc.1"),
        ):
            with self.subTest(tag=tag), tempfile.TemporaryDirectory() as tmp:
                directory = Path(tmp)
                manifest, assets = dmg_fixture(directory, tag)
                output = directory / "output"
                result = run(sys.executable, RENDER, tag, manifest, output)
                self.assertEqual(result.returncode, 0, result.stderr)
                files = list(output.rglob("*.rb"))
                self.assertEqual(files, [output / "Casks" / f"{token}.rb"])
                source = files[0].read_text()
                self.assertIn(f'cask "{token}" do', source)
                self.assertIn(f'version "{version}"', source)
                arch = re.search(r'arch arm: "([^"]+)", intel: "([^"]+)"', source)
                url = re.search(r'url "([^"]+)"', source)[1]
                for key, slug, asset in zip(("arm", "intel"), arch.groups(), assets):
                    self.assertRegex(source, rf'{key}: +"{asset["sha256"]}"')
                    self.assertEqual(url.replace("#{arch}", slug),
                                     f"{REPOSITORY}/releases/download/{tag}/{asset['name']}")
                self.assertNotEqual(assets[0]["sha256"], assets[1]["sha256"])
                if shutil.which("ruby"):
                    result = run("ruby", "-c", files[0])
                    self.assertEqual(result.returncode, 0, result.stderr)

        for tag, defect in ((f"alpha-v{VERSION}-test", "alpha"), (f"v{VERSION}", "missing"),
                            (f"v{VERSION}", "duplicate"), (f"v{VERSION}", "malformed")):
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as tmp:
                directory = Path(tmp)
                manifest, _ = dmg_fixture(directory, tag)
                first, second = manifest.read_text().splitlines(keepends=True)
                if defect == "missing":
                    manifest.write_text(first)
                elif defect == "duplicate":
                    manifest.write_text(first + second + first)
                elif defect == "malformed":
                    manifest.write_text(first[1:] + second)
                result = run(sys.executable, RENDER, tag, manifest, directory / "output")
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(list(directory.rglob("*.rb")))

    def test_cask_command_links(self):
        for token, other in (("airscout-rs", "airscout-rs@beta"),
                             ("airscout-rs@beta", "airscout-rs")):
            with self.subTest(token=token):
                source = (HOMEBREW / f"{token}.rb.in").read_text()
                self.assertIn('app "AirScout-RS.app"', source)
                self.assertIn(f'conflicts_with cask: "{other}"', source)
                self.assertEqual(re.findall(r'^  binary "#\{appdir\}/AirScout-RS\.app/'
                                            r'Contents/MacOS/([^"]+)"$', source, re.M),
                                 list(PROGRAMS))
                self.assertNotRegex(source, r"(?m)^  (?:uninstall|zap)\b")

    def test_workflow_gates_and_secret_boundary(self):
        caller = workflow_jobs("release.yml")["publish-homebrew"]
        self.assertIn("release", caller["needs"])
        self.assertEqual(caller["if"],
                         "${{ success() && needs.release.outputs.homebrew_eligible == 'true' }}")
        source = (WORKFLOWS / "release-channels.yml").read_text()
        callee = workflow_jobs("release-channels.yml")
        job = callee["homebrew"]
        self.assertEqual(job["environment"], "release")
        self.assertEqual(job["runs-on"], "ubuntu-latest")
        self.assertEqual(set(name for name, value in callee.items()
                             if "secrets.HOMEBREW_TAP_TOKEN" in str(value)), {"homebrew"})
        if yaml:
            call = yaml.load(source, Loader=yaml.BaseLoader)["on"]["workflow_call"]
            self.assertEqual(call["secrets"]["HOMEBREW_TAP_TOKEN"], {"required": "false"})
            self.assertIn("|| 'false'", call["outputs"]["homebrew_published"]["value"])
            self.assertEqual(caller["with"]["macos_signed"], "${{ needs.release.outputs.macos_signed }}")
        else:
            declared = source.split("\n    secrets:\n", 1)[1].split("\n    outputs:", 1)[0]
            self.assertRegex(declared, r"(?m)^      HOMEBREW_TAP_TOKEN:\n        required: false$")
            self.assertIn("jobs.homebrew.outputs.published || 'false'", source)
            self.assertIn("macos_signed: ${{ needs.release.outputs.macos_signed }}", caller["source"])
        self.assertIn("name: release-assets-verified", source)

    def test_publish_skip_and_bad_inventory(self):
        tag = f"v{VERSION}"
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            _, assets = dmg_fixture(directory, tag)
            output = directory / "github-output"
            cmd = (sys.executable, RENDER, "publish", tag, directory)
            env = dict(os.environ, GITHUB_OUTPUT=str(output))
            env.pop("HOMEBREW_TAP_TOKEN", None)
            for signed, token, okay in (("true", "", True), ("false", "fake", True),
                                        ("true", "fake-first\nfake-second", False),
                                        ("true", "fake", False)):
                with self.subTest(signed=signed, token_set=bool(token), okay=okay):
                    env.update(MACOS_SIGNED=signed, HOMEBREW_TAP_TOKEN=token)
                    output.write_text("")
                    if token == "fake" and signed == "true":
                        (directory / assets[0]["name"]).write_text("tampered")
                    result = run(*cmd, env=env)
                    self.assertEqual(result.returncode == 0, okay, result.stderr)
                    self.assertEqual(output.read_text(), "published=false\n")
                    if okay:
                        self.assertIn("skipped", result.stdout.lower())
                    else:
                        self.assertNotIn("fake", result.stdout + result.stderr)
                    if token == "fake" and signed == "true":
                        self.assertIn("hash", result.stderr.lower())

    def test_publish_verifies_manifest_inventory_and_bytes(self):
        sys.path.insert(0, str(HOMEBREW))
        self.addCleanup(sys.path.pop, 0)
        import render

        tag = f"beta-v{VERSION}-rc.1"
        for defect in (None, "manifest", "inventory", "unsigned", "missing"):
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as tmp:
                directory = Path(tmp)
                manifest, assets = dmg_fixture(directory, tag)
                inventory_file = directory / "inventory.json"
                if defect == "manifest":
                    manifest.write_text(manifest.read_text().replace(assets[1]["sha256"], "0" * 64))
                elif defect == "inventory":
                    inventory = json.loads(inventory_file.read_text())
                    inventory["assets"][1]["sha256"] = "0" * 64
                    inventory_file.write_text(json.dumps(inventory))
                elif defect == "unsigned":
                    inventory = json.loads(inventory_file.read_text())
                    inventory["macos_signed"] = False
                    inventory_file.write_text(json.dumps(inventory))
                elif defect == "missing":
                    (directory / assets[1]["name"]).unlink()
                output = directory / "github-output"
                with patch.dict(os.environ, {"GITHUB_OUTPUT": str(output), "MACOS_SIGNED": "true",
                                             "HOMEBREW_TAP_TOKEN": "synthetic"}), \
                        patch.object(render, "publish_cask") as publish:
                    if defect is None:
                        render.publish(tag, directory)
                        publish.assert_called_once()
                        _, path, cask = publish.call_args.args
                        self.assertEqual(path, "Casks/airscout-rs@beta.rb")
                        for asset in assets:
                            self.assertIn(asset["sha256"], cask)
                    else:
                        with self.assertRaises(ValueError):
                            render.publish(tag, directory)
                        publish.assert_not_called()
                expected = "true" if defect is None else "false"
                self.assertEqual(output.read_text(), f"published={expected}\n")

    def test_api_errors_hide_credentials(self):
        sys.path.insert(0, str(HOMEBREW))
        self.addCleanup(sys.path.pop, 0)
        import render

        token = "synthetic-private-token"
        result = subprocess.CompletedProcess([], 1, stdout=f"API error: {token}")
        with patch.dict(os.environ, {"HOMEBREW_TAP_TOKEN": token}), \
                patch.object(render.subprocess, "run", return_value=result) as gh:
            with self.assertRaisesRegex(ValueError, "^Tap GitHub API operation failed$"):
                render.graphql("query { viewer { login } }", {})
        args, kwargs = gh.call_args
        self.assertEqual(args, (["gh", "api", "--hostname", "github.com", "--method", "POST",
                                 "graphql", "--input", "-"],))
        self.assertEqual(kwargs["env"]["GH_TOKEN"], token)
        self.assertNotIn("HOMEBREW_TAP_TOKEN", kwargs["env"])
        self.assertNotIn(token, kwargs["input"])
        self.assertEqual(kwargs["stderr"], subprocess.DEVNULL)

    def test_publish_expected_head_and_signature(self):
        sys.path.insert(0, str(HOMEBREW))
        self.addCleanup(sys.path.pop, 0)
        import render

        path = "Casks/airscout-rs@beta.rb"
        tag = f"beta-v{VERSION}-rc.1"
        source = "synthetic beta cask\n"
        branch = {"repository": {"defaultBranchRef": {
            "name": "test-branch", "target": {"oid": HEAD},
        }}}
        for scenario in ("updated", "identical", "bad-signature", "wrong-parent", "stale"):
            with self.subTest(scenario=scenario):
                existing = {"__typename": "Blob", "isTruncated": False, "text": source}
                observed = {"repository": {"object": existing if scenario == "identical" else None}}
                new_commit = {"oid": NEXT, "signature": SIGNATURE, "parents": {"nodes": [{"oid": HEAD}]}}
                if scenario == "bad-signature":
                    new_commit["signature"] = None
                if scenario == "wrong-parent":
                    new_commit["parents"]["nodes"][0]["oid"] = "3" * 40
                reply = {"createCommitOnBranch": {"commit": new_commit}}
                if scenario == "stale":
                    reply = ValueError("Tap GitHub API operation failed")
                with patch.object(render, "graphql", side_effect=[branch, observed, reply]) as api:
                    if scenario in ("bad-signature", "wrong-parent", "stale"):
                        with self.assertRaises(ValueError):
                            render.publish_cask(tag, path, source)
                    else:
                        render.publish_cask(tag, path, source)
                calls = api.call_args_list
                self.assertEqual(calls[1].args[1], {"expression": f"{HEAD}:{path}"})
                if scenario == "identical":
                    self.assertEqual(len(calls), 2)
                    continue
                self.assertEqual(len(calls), 3)
                self.assertIn("createCommitOnBranch", calls[2].args[0])
                self.assertEqual(calls[2].args[1], {"input": {
                    "branch": {"repositoryNameWithOwner": "HamRadioOnline/homebrew-tap",
                               "branchName": "test-branch"},
                    "expectedHeadOid": HEAD,
                    "message": {"headline": f"Update airscout-rs@beta for {tag}"},
                    "fileChanges": {"additions": [{"path": path,
                                                  "contents": "c3ludGhldGljIGJldGEgY2Fzawo="}]},
                }})


if __name__ == "__main__":
    unittest.main()
