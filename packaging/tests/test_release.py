import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from test_container import CONTAINER, container_stage_root, toolchain_env
from test_linux import APP_ID, ROOT, yaml

RELEASE = ROOT / "packaging/release"
WORKFLOWS = ROOT / ".github/workflows"
VERSION = re.search(r'(?m)^\[workspace\.package\]\nversion = "([^"]+)"',
                    (ROOT / "Cargo.toml").read_text())[1]
LINUX = {"x86_64": "2.35", "arm64": "2.35", "armhf": "2.34", "i686": "2.36"}
DEP = {"check-tag", "linux", "appimage", "windows", "macos", "flatpak", "container"}
# The release publishes a deterministic source snapshot alongside the binaries.
SOURCE_SNAPSHOT_NAME = f"airscout-rs-{VERSION}-source.tar.gz"
SOURCE_SNAPSHOT_BYTES = b"source snapshot fixture\n"
SOURCE_SNAPSHOT_DIGEST = hashlib.sha256(SOURCE_SNAPSHOT_BYTES).hexdigest()


def run(*args, env=None):
    return subprocess.run(args, cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                          capture_output=True, text=True)


def fixture(root, tag, container_candidate, *, windows="missing", macos="unsigned", flatpak="missing"):
    public = {}

    def package(fmt, slug, extension, signing, floor=None):
        name = f"AirScout-RS-{tag}-{slug}.{extension}"
        artifact = root / f"package-{fmt}-{slug}"
        data = f"{name}\n{signing}\n".encode()
        (artifact / "assets").mkdir(parents=True)
        (artifact / "assets" / name).write_bytes(data)
        result = {"format": fmt, "slug": slug, "assets": [name], "signing": signing}
        if floor:
            result["glibc_floor"] = floor
        (artifact / "result.json").write_text(json.dumps(result))
        public[name] = data

    for arch, floor in LINUX.items():
        for fmt, extension in (("deb", "deb"), ("archlinux", "pkg.tar.zst"),
                               ("appimage", "AppImage"), ("rpm", "rpm")):
            if fmt != "rpm" or arch in ("x86_64", "arm64"):
                package(fmt, f"linux-{arch}", extension, "not_applicable", floor)
    for arch in ("x86_64", "arm64", "i686"):
        for fmt in ("msi", "zip"):
            package(fmt, f"windows-{arch}", fmt, windows)
    for arch in ("x86_64", "arm64"):
        package("dmg", f"macos-{arch}", "dmg", macos)
    channel = "stable" if tag.startswith("v") else tag.split("-")[0]
    for arch, slug in (("x86_64", "linux-x86_64"), ("aarch64", "linux-arm64")):
        artifact = root / f"package-flatpak-{arch}"
        artifact.mkdir(parents=True)
        (artifact / "build.tar.gz").write_bytes(b"transport fixture")
        (artifact / "result.json").write_text(json.dumps(
            {"format": "flatpak", "slug": slug, "branch": channel,
             "assets": [], "signing": "not_applicable"}))
    if channel != "alpha":
        artifact = root / "package-flatpak-repo"
        (artifact / "repo").mkdir(parents=True)
        (artifact / "repo" / "summary").write_bytes(b"summary")
        assets = []
        if flatpak == "verified":
            (artifact / "repo" / "summary.sig").write_bytes(b"signature")
            branch_suffix = ".beta" if channel == "beta" else ""
            assets = ["airscout-rs.flatpakrepo", f"{APP_ID}{branch_suffix}.flatpakref"]
            for name in assets:
                public[name] = f"descriptor {name}".encode()
                (artifact / "repo" / name).write_bytes(public[name])
        (artifact / "repo.result.json").write_text(json.dumps(
            {"format": "flatpak", "channel": channel, "assets": assets, "signing": flatpak}))
    shutil.copytree(container_candidate, root / "package-container")
    return public


def workflow_jobs(name):
    source = (WORKFLOWS / name).read_text()
    if yaml:
        return yaml.load(source, Loader=yaml.BaseLoader)["jobs"]
    blocks = re.findall(r"(?ms)^  ([\w-]+):\n(.*?)(?=^  [\w-]+:|\Z)",
                        source.split("\njobs:\n", 1)[1])
    return {job: dict(re.findall(r"(?m)^    ([\w-]+): (.+)$", block), source=block)
            for job, block in blocks}


class ReleaseTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        tmp = tempfile.TemporaryDirectory()
        cls.addClassCleanup(tmp.cleanup)
        root = Path(tmp.name)
        cls.container_candidate = root / "candidate"
        # Use the real producer with mocked container tools to exercise its output contract.
        result = run("bash", CONTAINER / "build.sh", container_stage_root(root),
                     cls.container_candidate, env=toolchain_env(root))
        if result.returncode:
            raise RuntimeError(result.stderr)

    def test_beta_tag_gate_rejects_unsupported_suffixes(self):
        prefix = f"beta-v{VERSION}-"
        longest_suffix = "a" * (128 - len(prefix))
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "github-output"
            env = dict(os.environ, GITHUB_OUTPUT=str(output))
            for suffix in ("rc.1", "rc.0", "rc01", longest_suffix):
                with self.subTest(suffix=suffix):
                    output.write_text("")
                    tag = prefix + suffix
                    result = run("bash", RELEASE / "check-tag.sh", tag, env=env)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output.read_text().splitlines(), [
                        f"tag={tag}", f"version={VERSION}", "channel=beta", "prerelease=true",
                    ])
            for suffix in ("rc.01", "rc..1", ".rc", "rc.", longest_suffix + "a"):
                with self.subTest(suffix=suffix):
                    output.write_text("")
                    result = run("bash", RELEASE / "check-tag.sh", prefix + suffix, env=env)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(output.read_text(), "")

    def collect(self, root, tag, output):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / SOURCE_SNAPSHOT_NAME
            source.write_bytes(SOURCE_SNAPSHOT_BYTES)
            return run(sys.executable, RELEASE / "collect.py", root, tag, output, source)

    def test_checksum_covers_final_bytes(self):
        for prefix, expected_count in (("", 24), ("beta-", 24), ("alpha-", 22)):
            with self.subTest(prefix=prefix), tempfile.TemporaryDirectory() as tmp:
                tag = f"{prefix}v{VERSION}" + ("-test" if prefix else "")
                root = Path(tmp) / "candidates"
                output = Path(tmp) / "output"
                public = fixture(root, tag, self.container_candidate, windows="verified",
                                 macos="signed", flatpak="verified")
                self.assertEqual(len(public), expected_count)
                result = self.collect(root, tag, output)
                self.assertEqual(result.returncode, 0, result.stderr)
                expected_public = {**public, SOURCE_SNAPSHOT_NAME: SOURCE_SNAPSHOT_BYTES}
                self.assertEqual((output / "SHA256SUMS").read_text(), "".join(
                    f"{hashlib.sha256(expected_public[name]).hexdigest()}  {name}\n"
                    for name in sorted(expected_public)))
                self.assertEqual({p.name for p in output.iterdir()},
                                 set(expected_public) | {"inventory.json", "SHA256SUMS"})
                for name, data in expected_public.items():
                    self.assertEqual((output / name).read_bytes(), data)

        for defect in ("missing", "duplicate", "private"):
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as tmp:
                tag = f"v{VERSION}"
                root = Path(tmp) / "candidates"
                fixture(root, tag, self.container_candidate)
                package = root / "package-deb-linux-x86_64"
                asset = package / "assets" / f"AirScout-RS-{tag}-linux-x86_64.deb"
                if defect == "missing":
                    asset.unlink()
                elif defect == "duplicate":
                    other = root / "package-rpm-linux-x86_64" / "assets"
                    (other / asset.name).write_bytes(asset.read_bytes())
                else:
                    (package / "assets" / "result.json").write_text("{}")
                output = Path(tmp) / "output"
                result = self.collect(root, tag, output)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(output.exists())

    def test_collect_lists_source_snapshot(self):
        with tempfile.TemporaryDirectory() as tmp:
            tag = f"v{VERSION}"
            root = Path(tmp) / "candidates"
            fixture(root, tag, self.container_candidate)
            output = Path(tmp) / "output"
            result = self.collect(root, tag, output)
            self.assertEqual(result.returncode, 0, result.stderr)
            inventory = json.loads((output / "inventory.json").read_text())
            sources = [asset for asset in inventory["assets"]
                       if asset["name"] == SOURCE_SNAPSHOT_NAME]
            self.assertEqual(len(sources), 1)
            self.assertEqual(sources[0]["format"], "source")
            self.assertEqual(sources[0]["signing"], "not_applicable")
            self.assertEqual(sources[0]["sha256"], SOURCE_SNAPSHOT_DIGEST)
            self.assertEqual(hashlib.sha256((output / SOURCE_SNAPSHOT_NAME).read_bytes()).hexdigest(),
                             SOURCE_SNAPSHOT_DIGEST)
            self.assertIn(SOURCE_SNAPSHOT_NAME, (output / "SHA256SUMS").read_text())

    def test_missing_key_is_channel_local(self):
        # All candidate calls use the implicit success condition on every prerequisite.
        jobs = workflow_jobs("release.yml")
        release = jobs["release"]
        needs = release["needs"]
        if isinstance(needs, str):
            needs = re.findall(r"[\w-]+", needs)
        self.assertEqual(set(needs), DEP)
        self.assertNotIn("if", release)
        for candidate in DEP - {"check-tag"}:
            self.assertNotIn("if", jobs[candidate])
            self.assertIn("build", jobs[candidate]["needs"])
        for name in ("flatpak", "container"):
            job = jobs[f"publish-{name}"]
            self.assertIn("release", job["needs"])
            self.assertEqual(job["if"], f"${{{{ success() && needs.release.outputs.{name}_eligible == 'true' }}}}")
            reused = workflow_jobs(f"release-{name}.yml")
            self.assertEqual(reused["build"]["if"], "${{ !inputs.publish }}")
            if name == "flatpak":
                self.assertIn("verify-candidate", reused["publish"]["needs"])
                self.assertIn("needs.verify-candidate.result == 'success'", reused["publish"]["if"])
            else:
                self.assertNotIn("needs", reused["publish"])

        sys.path.insert(0, str(RELEASE))
        self.addCleanup(sys.path.pop, 0)
        from publish import publication_gates
        signed = {"windows_signed": False, "macos_signed": True,
                  "flatpak_signed": True, "checksums_signed": True}
        self.assertEqual(publication_gates("stable", signed, aur_access=True), {
            "homebrew_eligible": True, "winget_eligible": False, "flatpak_eligible": True,
            "aur_eligible": True, "container_eligible": True,
        })
        self.assertEqual(publication_gates("beta", signed, aur_access=True), {
            "homebrew_eligible": True, "winget_eligible": False, "flatpak_eligible": True,
            "aur_eligible": False, "container_eligible": True,
        })
        self.assertFalse(any(publication_gates("alpha", signed, aur_access=True).values()))
        self.assertFalse(publication_gates("stable", signed, aur_access=False)["aur_eligible"])

        with tempfile.TemporaryDirectory() as tmp:
            tag = f"v{VERSION}"
            root = Path(tmp) / "candidates"
            fixture(root, tag, self.container_candidate, macos="signed")
            output = Path(tmp) / "output"
            result = self.collect(root, tag, output)
            self.assertEqual(result.returncode, 0, result.stderr)
            inventory = json.loads((output / "inventory.json").read_text())
            self.assertFalse(inventory["windows_signed"])
            self.assertTrue(inventory["macos_signed"])

    def test_collect_rejects_unarchived_flatpak_transport(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidates = root / "candidates"
            tag = f"alpha-v{VERSION}-test"
            fixture(candidates, tag, self.container_candidate)
            for arch in ("aarch64", "x86_64"):
                directory = candidates / f"package-flatpak-{arch}"
                (directory / "build.tar.gz").unlink()
                (directory / "build").mkdir()
            result = self.collect(candidates, tag, root / "output")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Unexpected layout:", result.stderr)

    def test_collect_rejects_arm64_above_glibc_floor(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidates = root / "candidates"
            tag = f"alpha-v{VERSION}-test"
            fixture(candidates, tag, self.container_candidate)
            path = candidates / "package-deb-linux-arm64/result.json"
            result = json.loads(path.read_text())
            result["glibc_floor"] = "2.36"
            path.write_text(json.dumps(result))
            collected = self.collect(candidates, tag, root / "output")
            self.assertNotEqual(collected.returncode, 0)
            self.assertIn("glibc floor above 2.35", collected.stderr)

    def test_notes_list_glibc_and_unsigned_assets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tag = f"beta-v{VERSION}-test"
            candidates = root / "candidates"
            fixture(candidates, tag, self.container_candidate)
            output = root / "output"
            result = self.collect(candidates, tag, output)
            self.assertEqual(result.returncode, 0, result.stderr)
            tag_message, commits, github = (root / name for name in ("tag", "commits", "github"))
            tag_message.write_text("Annotated release message\n")
            commits.write_text("* A direct commit\n")
            github.write_text("## What's Changed\n* PR example\n\n## New Contributors\n* Contributor\n")
            notes = root / "notes"
            result = run(sys.executable, RELEASE / "notes.py", output / "inventory.json",
                         tag_message, commits, github, notes)
            self.assertEqual(result.returncode, 0, result.stderr)
            rendered = notes.read_text()
            for arch, floor in LINUX.items():
                self.assertRegex(rendered, rf"linux-{arch}[^\n]*glibc {re.escape(floor)}")
            for label in ("Windows", "macOS", "SHA256SUMS"):
                self.assertRegex(rendered, rf"{label}[^\n]*[Uu]nsigned")
            for fragment in ("Annotated release message", "A direct commit", "PR example",
                             "New Contributors", "Pre-release"):
                self.assertIn(fragment, rendered)
            self.assertIn("### Commits\n* A direct commit", rendered)
            self.assertNotIn("inventory.json", rendered)

    def test_checksum_signing_without_credentials(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "assets"
            output.mkdir()
            (output / "SHA256SUMS").write_text("checksum fixture\n")
            step_output = Path(tmp) / "github-output"
            env = {key: value for key, value in os.environ.items()
                   if not key.startswith("RELEASE_GPG_")}
            env["GITHUB_OUTPUT"] = str(step_output)
            result = run("bash", RELEASE / "sign-checksums.sh", output, env=env)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "missing")
            self.assertEqual(step_output.read_text(), "checksums_signed=false\nfingerprint=\n")

            env["RELEASE_GPG_PASSPHRASE"] = "fake-first\nfake-second"
            step_output.unlink()
            result = run("bash", RELEASE / "sign-checksums.sh", output, env=env)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(step_output.read_text(), "checksums_signed=false\nfingerprint=\n")
            self.assertEqual({p.name for p in output.iterdir()}, {"SHA256SUMS"})
            for line in env["RELEASE_GPG_PASSPHRASE"].splitlines():
                self.assertNotIn(line, result.stdout + result.stderr)

    @unittest.skipUnless(shutil.which("gpg"), "GnuPG unavailable")
    def test_checksum_signing_with_encrypted_key(self):
        # Keep the socket path short enough for GPG, even on macOS.
        with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
            root = Path(tmp)
            home = root / "fixture"
            home.mkdir(mode=0o700)
            scratch = root / "scratch"
            scratch.mkdir()
            output = root / "assets"
            output.mkdir()
            manifest = output / "SHA256SUMS"
            manifest.write_text("checksum fixture\n")
            step_output = root / "github-output"
            passphrase = "synthetic-checksum-passphrase"
            gpg = ["gpg", "--no-options", "--homedir", home, "--batch", "--no-tty",
                   "--pinentry-mode", "loopback"]
            self.addCleanup(run, "gpgconf", "--homedir", home, "--kill", "gpg-agent")
            result = run(*gpg, "--passphrase", passphrase, "--quick-generate-key",
                         "Packaging Fixture <packaging@example.invalid>", "ed25519", "sign", "0")
            self.assertEqual(result.returncode, 0, result.stderr)
            key = run(*gpg, "--passphrase", passphrase, "--armor", "--export-secret-keys")
            self.assertEqual(key.returncode, 0, key.stderr)
            self.assertIn("BEGIN PGP PRIVATE KEY", key.stdout)
            env = {name: value for name, value in os.environ.items()
                   if not name.startswith("RELEASE_GPG_")}
            env.update(GITHUB_OUTPUT=str(step_output), TMPDIR=str(scratch),
                       RELEASE_GPG_PRIVATE_KEY=key.stdout)
            for password in (None, "incorrect-passphrase", passphrase):
                with self.subTest(has_password=password is not None):
                    if password is not None:
                        env["RELEASE_GPG_PASSPHRASE"] = password
                    step_output.write_text("")
                    result = run("bash", RELEASE / "sign-checksums.sh", output, env=env)
                    self.assertEqual(result.returncode == 0, password == passphrase, result.stderr)
                    expected = "true" if password == passphrase else "false"
                    self.assertIn(f"checksums_signed={expected}\n", step_output.read_text())
                    self.assertFalse(list(scratch.iterdir()))
                    for line in key.stdout.splitlines() + [passphrase, "incorrect-passphrase"]:
                        if line:
                            self.assertNotIn(line, result.stdout + result.stderr)
                    if password != passphrase:
                        self.assertEqual({p.name for p in output.iterdir()}, {"SHA256SUMS"})

            self.assertEqual(result.stdout.strip(), "verified")
            signature = output / "SHA256SUMS.asc"
            public_key = output / "SHA256SUMS.key.asc"
            self.assertIn("BEGIN PGP SIGNATURE", signature.read_text())
            self.assertIn("BEGIN PGP PUBLIC KEY", public_key.read_text())
            for path in (signature, public_key):
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o644)
            verify_home = root / "verify"
            verify_home.mkdir(mode=0o700)
            verifier = ["gpg", "--no-options", "--homedir", verify_home, "--batch"]
            self.assertEqual(run(*verifier, "--import", public_key).returncode, 0)
            self.assertEqual(run(*verifier, "--verify", signature, manifest).returncode, 0)
            sys.path.insert(0, str(RELEASE))
            self.addCleanup(sys.path.pop, 0)
            from publish import verify_signature
            verify_signature(signature, output)
            manifest.write_text("changed contents\n")
            with self.assertRaises(subprocess.CalledProcessError):
                verify_signature(signature, output)
            run("gpgconf", "--homedir", home, "--kill", "gpg-agent")
            run("gpgconf", "--homedir", verify_home, "--kill", "gpg-agent")

    def test_publish_resumes_draft_and_checks_published_bytes(self):
        sys.path.insert(0, str(RELEASE))
        self.addCleanup(sys.path.pop, 0)
        import publish

        for starting_state in ("new", "draft", "published"):
            with self.subTest(starting_state=starting_state), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                tag = f"v{VERSION}"
                repo = "example/project"
                endpoint = f"repos/{repo}/releases"
                public = fixture(root / "candidate", tag, self.container_candidate)
                output = root / "output"
                result = self.collect(root / "candidate", tag, output)
                self.assertEqual(result.returncode, 0, result.stderr)
                notes = root / "notes"
                notes.write_text("" if starting_state == "draft" else "Release notes fixture\n")
                inventory = json.loads((output / "inventory.json").read_text())
                release = {"tag_name": tag, "name": f"Release {tag}", "body": notes.read_text() or None,
                           "prerelease": False, "draft": starting_state != "published", "id": 32}
                remote_release = [] if starting_state == "new" else [release]
                public_all = {**public, SOURCE_SNAPSHOT_NAME: SOURCE_SNAPSHOT_BYTES}
                expected = set(public_all) | {"SHA256SUMS"}
                remote_assets = {}
                if starting_state in ("draft", "published"):
                    remote_assets["SHA256SUMS"] = hashlib.sha256(
                        (output / "SHA256SUMS").read_bytes()).hexdigest()
                if starting_state == "published":
                    remote_assets.update({name: hashlib.sha256(data).hexdigest()
                                          for name, data in public_all.items()})

                def pages(path):
                    if path == endpoint:
                        return remote_release
                    self.assertEqual(path, f"{endpoint}/32/assets")
                    return [{"name": name, "state": "uploaded",
                             "digest": f"sha256:{digest}", "id": i}
                            for i, (name, digest) in enumerate(remote_assets.items(), 1)]

                def gh(*args):
                    command = args[:3]
                    if command == ("release", "create", tag):
                        self.assertEqual(args[3:], ("--repo", repo, "--verify-tag", "--draft",
                                                   "--title", f"Release {tag}", "--notes-file",
                                                   str(notes)))
                        self.assertFalse(remote_release)
                        remote_release.append(release)
                    elif command == ("release", "upload", tag):
                        self.assertTrue(release["draft"])
                        self.assertEqual(args[3:5], ("--repo", repo))
                        uploaded = {Path(path).name for path in args[5:]}
                        self.assertEqual(uploaded, expected - set(remote_assets))
                        for path in map(Path, args[5:]):
                            remote_assets[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
                    elif command == ("release", "edit", tag):
                        self.assertEqual(args[3:], ("--repo", repo, "--draft=false", "--latest"))
                        self.assertEqual(set(remote_assets), expected)
                        release["draft"] = False
                    else:
                        self.fail(f"Unexpected gh command: {args}")

                with patch.object(publish, "pages", side_effect=pages), \
                        patch.object(publish, "gh", side_effect=gh) as invoked, \
                        patch.object(publish, "api", return_value=release):
                    publish.publish(repo, tag, output, notes, inventory)
                    self.assertFalse(release["draft"])
                    expected_calls = {"new": 3, "draft": 2, "published": 0}
                    self.assertEqual(invoked.call_count, expected_calls[starting_state])
                    remote_assets["SHA256SUMS"] = "0" * 64
                    with self.assertRaisesRegex(ValueError, "digest differs"):
                        publish.publish(repo, tag, output, notes, inventory)
                    self.assertEqual(invoked.call_count, expected_calls[starting_state])

    def test_publish_waits_for_the_new_draft_to_be_listed(self):
        # GitHub's release list can lag a fresh `gh release create` by a few
        # seconds (alpha-v0.1.0-9 failed with "No draft found").
        sys.path.insert(0, str(RELEASE))
        self.addCleanup(sys.path.pop, 0)
        import publish

        tag, repo = "alpha-v0.1.0-9", "example/project"
        endpoint = f"repos/{repo}/releases"
        release = {"tag_name": tag, "draft": True, "id": 7}
        state = {"created": False, "lag": 2}

        def pages(path):
            self.assertEqual(path, endpoint)
            if not state["created"]:
                return []
            if state["lag"]:
                state["lag"] -= 1
                return []
            return [release]

        def create(*args):
            state["created"] = True

        with patch.object(publish, "pages", side_effect=pages), \
                patch.object(publish.time, "sleep") as slept:
            found = publish.create_draft(endpoint, tag, create)
        self.assertIs(found, release)
        self.assertEqual(slept.call_count, 2)

        state.update(created=False, lag=100)
        with patch.object(publish, "pages", side_effect=pages), \
                patch.object(publish.time, "sleep"):
            with self.assertRaisesRegex(ValueError, "No draft found after creating"):
                publish.create_draft(endpoint, tag, create)

    @unittest.skipUnless(yaml, "PyYAML unavailable")
    def test_reusable_callers_grant_nested_job_permissions(self):
        # GitHub validates every nested job of a called workflow at startup,
        # including jobs whose `if` is false; a nested job requesting more than
        # the caller grants fails the whole run with startup_failure.
        rank = {"none": 0, "read": 1, "write": 2}

        def grants(permissions):
            if isinstance(permissions, str):
                level = {"read-all": "read", "write-all": "write"}.get(permissions, permissions)
                return lambda scope: level
            return lambda scope: (permissions or {}).get(scope, "none")

        for caller_file in sorted(WORKFLOWS.glob("*.yml")):
            caller = yaml.load(caller_file.read_text(), Loader=yaml.BaseLoader)
            for job_name, job in caller.get("jobs", {}).items():
                uses = job.get("uses", "")
                if not uses.startswith("./.github/workflows/"):
                    continue
                granted = grants(job.get("permissions", caller.get("permissions", "write-all")))
                called = yaml.load((ROOT / uses[2:]).read_text(), Loader=yaml.BaseLoader)
                for nested_name, nested in called["jobs"].items():
                    requested = nested.get("permissions", called.get("permissions"))
                    if requested is None or isinstance(requested, str):
                        continue
                    for scope, level in requested.items():
                        with self.subTest(caller=f"{caller_file.name}:{job_name}",
                                          nested=f"{uses}:{nested_name}", scope=scope):
                            self.assertLessEqual(rank[level], rank[granted(scope)])

    def test_secret_boundary(self):
        workflow = (WORKFLOWS / "release-assets.yml").read_text()
        jobs = workflow_jobs("release-assets.yml")
        names = {"RELEASE_GPG_PRIVATE_KEY", "RELEASE_GPG_PASSPHRASE", "AUR_SSH_PRIVATE_KEY"}
        if yaml:
            declared = yaml.load(workflow, Loader=yaml.BaseLoader)["on"]["workflow_call"]["secrets"]
            self.assertEqual(declared, {name: {"required": "false"} for name in names})
        else:
            block = workflow.split("    secrets:\n", 1)[1].split("\n    outputs:", 1)[0]
            self.assertEqual(set(re.findall(r"(?m)^      ([A-Z_]+):\n        required: false$",
                                            block)), names)
        for name in names:
            consumers = [job for job in jobs.values() if f"secrets.{name}" in str(job)]
            self.assertEqual(len(consumers), 1, name)
            self.assertEqual(consumers[0]["environment"], "release")


if __name__ == "__main__":
    unittest.main()
