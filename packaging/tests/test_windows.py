from base64 import b32encode
from itertools import product
import json
import os
from pathlib import Path
import re
import subprocess
from textwrap import dedent
import tempfile
import unittest
import xml.etree.ElementTree as ET

from test_linux import PROGRAMS, ROOT, run, yaml


WINDOWS = ROOT / "packaging/windows"
WORKFLOW = ROOT / ".github/workflows/release-windows.yml"
SECRETS = ("CERTUM_ACCOUNT_EMAIL", "CERTUM_TOTP_SEED", "CERTUM_CERT_FINGERPRINT")
WIX = {"w": "http://wixtoolset.org/schemas/v4/wxs"}


class WindowsTests(unittest.TestCase):
    def test_msi_contract(self):
        root = ET.parse(WINDOWS / "AirScoutRS.wxs")
        package = root.find("w:Package", WIX)
        self.assertEqual(package.get("Scope"), "perMachine")
        self.assertEqual(package.get("Manufacturer"), "HamRadioOnline")
        self.assertEqual([item.get("UpgradeCode") for item in root.iter()
                          if "UpgradeCode" in item.attrib],
                         ["{5CEC7FA4-CCAD-467A-91D1-759E51F78ADD}"])
        upgrade = package.find("w:MajorUpgrade", WIX)
        self.assertEqual(upgrade.get("AllowSameVersionUpgrades"), "yes")
        self.assertIn("newer", upgrade.get("DowngradeErrorMessage"))
        directory = package.find("w:StandardDirectory[@Id='ProgramFiles6432Folder']/w:Directory", WIX)
        self.assertEqual(directory.get("Name"), "AirScout-RS")
        files = [item.get("Source").rsplit("\\", 1)[-1]
                 for item in directory.findall(".//w:File", WIX)]
        self.assertCountEqual(files, {f"{name}.exe" for name in PROGRAMS} |
                              {"README.md", "LICENSE", "NOTICE"})
        shortcut, = package.findall(".//w:Shortcut", WIX)
        self.assertEqual(shortcut.get("Name"), "AirScout-RS")
        self.assertIsNotNone(package.find(".//w:File[@Id='AirScoutExe']/w:Shortcut", WIX))
        path, = package.findall(".//w:Environment", WIX)
        self.assertEqual(path.attrib["Name"], "PATH")
        self.assertEqual(path.attrib["System"], "yes")
        self.assertEqual(path.attrib["Part"], "last")
        ids = [item.get("Id") for item in package.findall(".//w:Component", WIX)]
        self.assertCountEqual(ids, [item.get("Id") for item in
                                   package.findall(".//w:ComponentRef", WIX)])

    def test_signing_order(self):
        workflow = WORKFLOW.read_text()
        if yaml:
            jobs = yaml.load(workflow, Loader=yaml.BaseLoader)["jobs"]
            self.assertEqual(jobs["assemble"]["needs"], "sign-exes")
            self.assertCountEqual(jobs["finalize"]["needs"], ["assemble", "sign-exes"])
            for job in ("sign-exes", "finalize"):
                self.assertTrue(jobs[job]["runs-on"].startswith("ubuntu-"))
                self.assertEqual(jobs[job]["environment"], "release")
            self.assertTrue(jobs["assemble"]["runs-on"].startswith("windows-"))
            steps = [[step["id"] for step in jobs[job]["steps"] if "id" in step]
                     for job in ("sign-exes", "assemble", "finalize")]
        else:
            jobs = dict(re.findall(r"(?ms)^  ([\w-]+):\n(.*?)(?=^  [\w-]+:|\Z)",
                                   workflow.split("\njobs:\n", 1)[1]))
            self.assertIn("    needs: sign-exes\n", jobs["assemble"])
            self.assertIn("    needs: [assemble, sign-exes]\n", jobs["finalize"])
            for job in ("sign-exes", "finalize"):
                self.assertRegex(jobs[job], r"(?m)^    runs-on: ubuntu-")
                self.assertIn("\n    environment: release\n", jobs[job])
            self.assertRegex(jobs["assemble"], r"(?m)^    runs-on: windows-")
            steps = [re.findall(r"(?m)^        id: ([\w-]+)$", jobs[job])
                     for job in ("sign-exes", "assemble", "finalize")]
        self.assertIn("sign-exes", steps[0])
        self.assertIn("build-msi", steps[1])
        self.assertIn("build-zip", steps[1])
        self.assertLess(steps[2].index("sign-msi"), steps[2].index("package-result"))
        for secret in SECRETS:
            self.assertNotIn(f"secrets.{secret}", str(jobs["assemble"]))
            for job in ("sign-exes", "finalize"):
                self.assertIn(f"secrets.{secret}", str(jobs[job]))

    def test_rfc3161_timestamp_flag(self):
        script = (WINDOWS / "sign.sh").read_text()
        sign_msi = re.search(r"(?ms)^sign_msi\(\) \{(.*?)^\}", script).group(1)
        self.assertIn('-ts "$timestamp"', sign_msi)
        self.assertNotIn('-t "$timestamp"', sign_msi)

    def test_osslsigncode_curl_guard(self):
        script = (WINDOWS / "sign.sh").read_text()
        self.assertIn("cURL support disabled", script)
        self.assertIn("fail 'osslsigncode was built without libcurl", script)

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

    def test_signing_dependencies(self):
        workflow = WORKFLOW.read_text()
        packages = "pkg-config libssl-dev libcurl4-openssl-dev zlib1g-dev"
        if yaml:
            jobs = yaml.load(workflow, Loader=yaml.BaseLoader)["jobs"]
            for job in ("sign-exes", "finalize"):
                steps = jobs[job]["steps"]
                self.assertTrue(any(packages in (step.get("run") or "") for step in steps), job)
                self.assertTrue(any("steps.certum.outputs.state == 'configured'"
                                    in (step.get("if") or "") for step in steps), job)
            self.assertTrue(any("rustup toolchain install stable --profile minimal" in step.get("run", "")
                                for step in jobs["sign-exes"]["steps"]))
            self.assertFalse(any("rustup toolchain install stable --profile minimal" in step.get("run", "")
                                 for step in jobs["finalize"]["steps"]))
        else:
            jobs = dict(re.findall(r"(?ms)^  ([\w-]+):\n(.*?)(?=^  [\w-]+:|\Z)",
                                   workflow.split("\njobs:\n", 1)[1]))
            for job in ("sign-exes", "finalize"):
                self.assertIn(packages, jobs[job])
                self.assertIn("steps.certum.outputs.state == 'configured'", jobs[job])
            self.assertIn("rustup toolchain install stable --profile minimal", jobs["sign-exes"])
            self.assertNotIn("rustup toolchain install stable --profile minimal", jobs["finalize"])

    def test_secret_sets(self):
        values = ("TE1ST@localhost", b32encode(b"TE1ST JO50AA").decode(), "ab" * 32)
        with tempfile.TemporaryDirectory() as temporary:
            for present in product((False, True), repeat=3):
                with self.subTest(present=present):
                    env = {k: v for k, v in os.environ.items() if not k.startswith("CERTUM_")}
                    env.update({key: value for key, value, set_ in zip(SECRETS, values, present) if set_})
                    env["TMPDIR"] = temporary
                    result = run("bash", WINDOWS / "sign.sh", "check", env=env, check=False)
                    if all(present):
                        self.assertEqual((result.returncode, result.stdout.strip()), (0, "configured"))
                    elif any(present):
                        self.assertNotEqual(result.returncode, 0)
                        self.assertIn("partial", result.stderr.lower())
                    else:
                        self.assertEqual((result.returncode, result.stdout.strip()), (0, "missing"))
                    for value in values:
                        self.assertNotIn(value, result.stdout + result.stderr)

    def test_package_result(self):
        workflow = WORKFLOW.read_text()
        if yaml:
            steps = yaml.load(workflow, Loader=yaml.BaseLoader)["jobs"]["finalize"]["steps"]
            script = next(step["run"] for step in steps if step.get("id") == "package-result")
        else:
            step = re.search(r"(?ms)^        id: package-result\n(.*?)(?=^      - )", workflow)
            self.assertIsNotNone(step)
            script = dedent(step[1].split("        run: |\n", 1)[1])
        version = re.search(r'(?m)^\[workspace\.package\]\nversion = "([^"]+)"',
                            (ROOT / "Cargo.toml").read_text())[1]
        tag = f"beta-v{version}-TE1ST"
        for signing in ("missing", "verified"):
            with self.subTest(signing=signing), tempfile.TemporaryDirectory() as directory:
                package_dirs = []
                for slug in ("windows-x86_64", "windows-i686", "windows-arm64"):
                    for fmt in ("zip", "msi"):
                        package = Path(directory) / "candidate" / slug / fmt
                        asset = f"AirScout-RS-{tag}-{slug}.{fmt}"
                        (package / "assets").mkdir(parents=True)
                        (package / "assets" / asset).touch()
                        package_dirs.append((package, fmt, slug, asset))
                env = dict(os.environ, TAG=tag, EXE_SIGNING=signing, MSI_SIGNING="")
                rejected = subprocess.run(["bash", "-ec", script], cwd=directory, env=env,
                                          capture_output=True, text=True)
                self.assertNotEqual(rejected.returncode, 0)
                self.assertFalse(list(Path(directory).rglob("result.json")))
                env["MSI_SIGNING"] = signing
                accepted = subprocess.run(["bash", "-ec", script], cwd=directory, env=env,
                                          capture_output=True, text=True)
                self.assertEqual(accepted.returncode, 0, accepted.stderr)
                for package, fmt, slug, asset in package_dirs:
                    self.assertEqual(json.loads((package / "result.json").read_text()), {
                        "format": fmt, "slug": slug, "assets": [asset], "signing": signing,
                    })


if __name__ == "__main__":
    unittest.main()
