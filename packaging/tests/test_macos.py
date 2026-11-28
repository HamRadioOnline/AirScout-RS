from base64 import b64encode
from itertools import product
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

from test_linux import PROGRAMS, ROOT, run, yaml


MACOS = ROOT / "packaging/macos"
WORKFLOW = ROOT / ".github/workflows/release-macos.yml"
SECRETS = ("MACOS_CERT_P12", "MACOS_CERT_PASSWORD", "MACOS_NOTARY_KEY",
           "MACOS_NOTARY_KEY_ID", "MACOS_NOTARY_ISSUER")
SECRET_VALUES = {
    "MACOS_CERT_P12": b64encode(b"TE1ST p12 contents").decode(),
    "MACOS_CERT_PASSWORD": "TE1ST-password",
    "MACOS_NOTARY_KEY": "-----BEGIN PRIVATE KEY-----\nTE1ST\n-----END PRIVATE KEY-----\n",
    "MACOS_NOTARY_KEY_ID": "TE1STKEYID",
    "MACOS_NOTARY_ISSUER": "TE1ST-issuer-uuid",
}
# Distinct, non-overlapping values so the redaction test can assert that every
# secret line is scrubbed regardless of substitution order.
REDACTION_SECRETS = {
    "MACOS_CERT_PASSWORD": "redact-password-9f",
    "MACOS_NOTARY_KEY": "-----BEGIN REDACT KEY-----\nfirst-redact-line\nsecond-redact-line\n-----END REDACT KEY-----\n",
    "MACOS_NOTARY_KEY_ID": "REDACTKEYID7",
    "MACOS_NOTARY_ISSUER": "REDACTISSUER8",
}
APP_ID = "io.github.hamradioonline.AirScoutRS"


def macos_stage_fixture(stage, version="0.1.0", slug="macos-arm64"):
    (stage / "bin").mkdir(parents=True)
    for name in PROGRAMS:
        program = stage / "bin" / name
        program.write_text("#!/bin/sh\nexit 0\n")
        program.chmod(0o755)
    for name in ("README.md", "LICENSE", "NOTICE"):
        (stage / name).write_text(f"{name} fixture\n")
    (stage / "stage.json").write_text(json.dumps({
        "target": "aarch64-apple-darwin" if slug == "macos-arm64" else "x86_64-apple-darwin",
        "slug": slug,
        "version": version,
        "glibc_floor": None,
    }))


def signing_state(*present):
    env = {k: v for k, v in os.environ.items() if not k.startswith("MACOS_")}
    env.update({key: SECRET_VALUES[key] for key, set_ in zip(SECRETS, present) if set_})
    return run("bash", MACOS / "build-signed-dmg.sh", "check", env=env, check=False)


class MacosTests(unittest.TestCase):
    def test_bundle_layout(self):
        plist = plistlib.loads((MACOS / "Info.plist").read_bytes())
        self.assertEqual(plist["CFBundleIdentifier"], APP_ID)
        self.assertEqual(plist["CFBundleExecutable"], "airscout")
        self.assertEqual(plist["CFBundleIconFile"], "AirScout-RS.icns")
        self.assertEqual(plist["CFBundlePackageType"], "APPL")

    def test_entitlements_no_jit(self):
        plist = plistlib.loads((MACOS / "AirScout-RS.entitlements").read_bytes())
        self.assertEqual(plist, {"com.apple.security.network.client": True})
        self.assertNotIn("com.apple.security.cs.allow-jit", plist)
        self.assertNotIn("com.apple.security.cs.allow-unsigned-executable-memory", plist)
        self.assertNotIn("com.apple.security.cs.disable-library-validation", plist)

    def test_dmg_signing_order(self):
        script = (MACOS / "build-signed-dmg.sh").read_text()
        order = [
            '--sign "$FULL_IDENTITY" "$binary"',          # sign each Mach-O binary
            '--entitlements "$ENTITLEMENTS" --sign "$FULL_IDENTITY" "$app"',  # seal app
            'hdiutil create',                              # build DMG
            '--sign "$FULL_IDENTITY" "$dmg"',             # sign DMG
            'xcrun notarytool submit',                     # notarize
            'xcrun stapler staple',                        # staple ticket
            'codesign --verify --deep --strict',           # verify shipped app
            'spctl --assess',                              # Gatekeeper assessment
        ]
        positions = [script.index(marker) for marker in order]
        self.assertEqual(positions, sorted(positions))

        # Gatekeeper assessment must run under run_checked (so a signed build
        # fails the gate on rejection) and never swallow its exit status with
        # "|| true".
        spctl_lines = [line for line in script.splitlines() if "spctl --assess" in line]
        self.assertTrue(spctl_lines)
        for line in spctl_lines:
            self.assertIn("run_checked", line)
            self.assertNotIn("|| true", line)

        # One missing Apple secret selects the unsigned build.
        for missing in range(5):
            present = [True] * 5
            present[missing] = False
            result = signing_state(*present)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "unsigned")

    def test_secret_sets(self):
        with tempfile.TemporaryDirectory() as temporary:
            for present in product((False, True), repeat=5):
                with self.subTest(present=present):
                    env = {k: v for k, v in os.environ.items() if not k.startswith("MACOS_")}
                    env.update({key: SECRET_VALUES[key] for key, set_ in zip(SECRETS, present) if set_})
                    env["TMPDIR"] = temporary
                    result = run("bash", MACOS / "build-signed-dmg.sh", "check", env=env, check=False)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.strip(),
                                     "signed" if all(present) else "unsigned")
                    for value in SECRET_VALUES.values():
                        self.assertNotIn(value, result.stdout + result.stderr)

    def test_workflow_call_secrets(self):
        workflow = WORKFLOW.read_text()
        if yaml:
            on = yaml.load(workflow, Loader=yaml.BaseLoader)["on"]
            secrets = on["workflow_call"]["secrets"]
            self.assertEqual(set(secrets), set(SECRETS))
            for secret in SECRETS:
                self.assertEqual(secrets[secret]["required"], "false")
        else:
            block = re.search(r"(?ms)^    secrets:\n(.*?)(?=^permissions:)", workflow)
            self.assertIsNotNone(block)
            for secret in SECRETS:
                self.assertRegex(block.group(1),
                                 rf"(?m)^      {re.escape(secret)}:\n        required: false$")

    def test_signing_order(self):
        workflow = WORKFLOW.read_text()
        if yaml:
            jobs = yaml.load(workflow, Loader=yaml.BaseLoader)["jobs"]
            self.assertEqual(jobs["sign"]["needs"], "assemble")
            self.assertEqual(jobs["sign"]["environment"], "release")
            self.assertNotIn("environment", jobs["assemble"])
            for slug in ("macos-arm64", "macos-x86_64"):
                self.assertIn(slug, [row["slug"] for row in jobs["assemble"]["strategy"]["matrix"]["include"]])
                self.assertIn(slug, [row["slug"] for row in jobs["sign"]["strategy"]["matrix"]["include"]])
        else:
            jobs = dict(re.findall(r"(?ms)^  ([\w-]+):\n(.*?)(?=^  [\w-]+:|\Z)",
                                   workflow.split("\njobs:\n", 1)[1]))
            self.assertIn("\n    needs: assemble\n", jobs["sign"])
            self.assertIn("\n    environment: release\n", jobs["sign"])
            self.assertNotIn("\n    environment:", jobs["assemble"])
            for job in ("assemble", "sign"):
                self.assertRegex(jobs[job], r"(?m)^    runs-on: \${{ matrix\.os }}")
                for slug in ("macos-arm64", "macos-x86_64"):
                    self.assertIn(f"slug: {slug}", jobs[job])
        for secret in SECRETS:
            self.assertNotIn(f"secrets.{secret}", str(jobs["assemble"]))
            self.assertIn(f"secrets.{secret}", str(jobs["sign"]))

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("hdiutil"),
                         "requires macOS DMG tools")
    def test_bundle_assembly(self):
        version = re.search(r'(?m)^\[workspace\.package\]\nversion = "([^"]+)"',
                            (ROOT / "Cargo.toml").read_text())[1]
        with tempfile.TemporaryDirectory() as tmp:
            # A nested parent that does not exist yet, plus a trailing slash, must
            # be created and canonicalized (not break os.rename).
            stage = Path(tmp) / "stage"
            output = Path(tmp) / "nested" / "package"
            macos_stage_fixture(stage, version=version)
            result = run("bash", MACOS / "build-signed-dmg.sh", stage, f"v{version}",
                         str(output) + "/")
            self.assertEqual(result.returncode, 0, result.stderr)

            dmg = output / "assets" / f"AirScout-RS-v{version}-macos-arm64.dmg"
            self.assertTrue(dmg.is_file(), list((output / "assets").iterdir()))

            sidecar = json.loads((output / "result.json").read_text())
            self.assertEqual(sidecar, {
                "format": "dmg", "slug": "macos-arm64",
                "assets": [f"AirScout-RS-v{version}-macos-arm64.dmg"],
                "signing": "unsigned",
            })

            # Mount the DMG and inspect the shipped bundle directly.
            mount = Path(tmp) / "mount"
            mount.mkdir()
            subprocess.run(["hdiutil", "attach", str(dmg), "-nobrowse", "-quiet",
                            "-mountpoint", str(mount)], check=True)
            try:
                shipped = mount / "AirScout-RS.app"
                contents = shipped / "Contents"
                self.assertEqual({p.name for p in (contents / "MacOS").iterdir()}, PROGRAMS)
                self.assertEqual({p.name for p in (contents / "Resources").iterdir()},
                                 {"AirScout-RS.icns", "README.md", "LICENSE", "NOTICE"})
                plist = plistlib.loads((contents / "Info.plist").read_bytes())
                self.assertEqual(plist["CFBundleIdentifier"], APP_ID)
                self.assertEqual(plist["CFBundleExecutable"], "airscout")
                self.assertEqual(plist["CFBundleIconFile"], "AirScout-RS.icns")
                self.assertEqual(plist["CFBundleShortVersionString"], version)
                self.assertEqual(plist["CFBundleVersion"], version)
                # World-readable bundle: dirs 0755, resources 0644, binaries 0755.
                for path in (shipped, contents, contents / "MacOS", contents / "Resources"):
                    self.assertEqual(path.stat().st_mode & 0o777, 0o755, path)
                self.assertEqual((contents / "Info.plist").stat().st_mode & 0o777, 0o644)
                for name in ("README.md", "LICENSE", "NOTICE", "AirScout-RS.icns"):
                    self.assertEqual((contents / "Resources" / name).stat().st_mode & 0o777,
                                     0o644, name)
                for name in PROGRAMS:
                    self.assertEqual((contents / "MacOS" / name).stat().st_mode & 0o777,
                                     0o755, name)
                self.assertTrue((mount / "Applications").is_symlink())
            finally:
                subprocess.run(["hdiutil", "detach", str(mount), "-quiet"], check=False)

    @unittest.skipUnless(sys.platform == "darwin" and shutil.which("plutil"),
                         "requires macOS plist tools")
    def test_redaction_on_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            stage = base / "stage"
            output = base / "package"
            macos_stage_fixture(stage)

            # A fake hdiutil that dumps every secret it can see (single-line and
            # multiline) into its captured log, then fails, so run_checked has to
            # print a redacted log for the failing command.
            fake_bin = base / "bin"
            fake_bin.mkdir()
            hdiutil = fake_bin / "hdiutil"
            hdiutil.write_text(
                '#!/bin/sh\n'
                'echo "password=$MACOS_CERT_PASSWORD"\n'
                'echo "key-id=$MACOS_NOTARY_KEY_ID"\n'
                'echo "issuer=$MACOS_NOTARY_ISSUER"\n'
                'printf "%s\\n" "$MACOS_NOTARY_KEY"\n'
                'exit 1\n'
            )
            hdiutil.chmod(0o755)

            env = {k: v for k, v in os.environ.items() if not k.startswith("MACOS_")}
            env.update(REDACTION_SECRETS)
            env["PATH"] = f"{fake_bin}:{os.environ['PATH']}"

            result = run("bash", MACOS / "build-signed-dmg.sh", stage, "v0.1.0", output,
                         env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("[redacted]", result.stderr)
            for value in REDACTION_SECRETS.values():
                for line in value.splitlines():
                    if line:
                        self.assertNotIn(line, result.stderr)


if __name__ == "__main__":
    unittest.main()
