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

try:
    import yaml
except ImportError:
    yaml = None


ROOT = Path(__file__).resolve().parents[2]
RELEASE = ROOT / "packaging/release"
PROGRAMS = {
    "airscout", "airscout-planeserver", "airscout-cli", "catcheck",
    "elevation-tile-gen", "maptile-gen",
}
MATRIX = {
    ("linux-x86_64", "x86_64-unknown-linux-gnu", "ubuntu-22.04"),
    ("linux-arm64", "aarch64-unknown-linux-gnu", "ubuntu-22.04-arm"),
    ("linux-armhf", "armv7-unknown-linux-gnueabihf", "ubuntu-22.04"),
    ("linux-i686", "i686-unknown-linux-gnu", "ubuntu-22.04"),
    ("windows-x86_64", "x86_64-pc-windows-msvc", "windows-latest"),
    ("windows-i686", "i686-pc-windows-msvc", "windows-latest"),
    ("windows-arm64", "aarch64-pc-windows-msvc", "windows-11-arm"),
    ("macos-arm64", "aarch64-apple-darwin", "macos-latest"),
    ("macos-x86_64", "x86_64-apple-darwin", "macos-15-intel"),
}


def shell(script, *args, cwd=ROOT, env=None):
    return subprocess.run(
        ["bash", str(script), *args], cwd=cwd, env=env,
        capture_output=True, text=True,
    )


class StageTests(unittest.TestCase):
    def test_tag_contract(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "github-output"
            env = dict(os.environ, GITHUB_OUTPUT=str(output))

            def classify(tag):
                output.write_text("")
                result = shell(RELEASE / "check-tag.sh", tag, env=env)
                self.assertEqual(result.returncode, 0, result.stderr)
                values = dict(line.split("=", 1) for line in output.read_text().splitlines())
                self.assertEqual(values["tag"], tag)
                self.assertEqual(values["prerelease"],
                                 "false" if values["channel"] == "stable" else "true")
                return values["channel"], values["version"]

            self.assertEqual(classify("v0.1.0"), ("stable", "0.1.0"))
            self.assertEqual(classify("beta-v0.1.0-rc.2"), ("beta", "0.1.0"))
            self.assertEqual(classify("alpha-v0.1.0-preview1"), ("alpha", "0.1.0"))
            for tag in ("v0.2.0", "beta-v0.1.0-bad-suffix", "alpha-v0.1.0-"):
                with self.subTest(tag=tag):
                    output.write_text("")
                    self.assertNotEqual(shell(RELEASE / "check-tag.sh", tag, env=env).returncode, 0)
                    self.assertEqual(output.read_text(), "")

    def test_stage_requires_six_programs(self):
        with tempfile.TemporaryDirectory() as tmp:
            build = Path(tmp) / "build"
            target = "aarch64-apple-darwin"
            release = build / target / "release"
            release.mkdir(parents=True)
            env = dict(os.environ, CARGO_TARGET_DIR=str(build))
            for program in PROGRAMS:
                path = release / program
                path.write_text("#!/bin/sh\nexit 0\n")
                path.chmod(0o751)
            (release / "airscout.d").touch()

            output = Path(tmp) / "staged files"
            result = shell(RELEASE / "stage.sh", target, "macos-arm64", str(output), env=env)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual({p.name for p in output.iterdir()},
                             {"bin", "README.md", "LICENSE", "NOTICE", "stage.json"})
            self.assertEqual({p.name for p in (output / "bin").iterdir()}, PROGRAMS)
            for program in PROGRAMS:
                self.assertEqual(stat.S_IMODE((output / "bin" / program).stat().st_mode), 0o751)
            for name in ("README.md", "LICENSE", "NOTICE"):
                self.assertEqual((output / name).read_bytes(), (ROOT / name).read_bytes())
            stage = json.loads((output / "stage.json").read_text())
            self.assertEqual((stage["target"], stage["slug"], stage["version"]),
                             (target, "macos-arm64", "0.1.0"))

            missing = release / "maptile-gen"
            missing.unlink()
            invalid = Path(tmp) / "incomplete"
            result = shell(RELEASE / "stage.sh", target, "macos-arm64", str(invalid), env=env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(missing.name, result.stderr)
            self.assertFalse(invalid.exists())

    def test_release_matrix(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        if yaml:
            jobs = yaml.load(workflow, Loader=yaml.BaseLoader)["jobs"]
            rows = jobs["build"]["strategy"]["matrix"]["include"]
            self.assertEqual(jobs["build"]["needs"], "test")
            self.assertEqual(jobs["test"]["needs"], "check-tag")
            self.assertEqual(jobs["build"]["runs-on"], "${{ matrix.os }}")
        else:
            jobs = dict(re.findall(r"^  ([\w-]+):\n((?:^(?:    .*|)$\n)*)", workflow, re.M))
            build = jobs["build"]
            matrix = build.split("      matrix:\n", 1)[1].split("    runs-on:", 1)[0]
            rows = [
                dict(re.findall(r"^\s*(?:- )?(name|target|os): (\S+)$", row, re.M))
                for row in re.split(r"(?=^          - name:)", matrix, flags=re.M)[1:]
            ]
            self.assertIn("    needs: test\n", build)
            self.assertIn("    needs: check-tag\n", jobs["test"])
            self.assertIn("    runs-on: ${{ matrix.os }}\n", build)
        self.assertEqual(len(rows), len(MATRIX))
        self.assertEqual({(row["name"], row["target"], row["os"]) for row in rows}, MATRIX)
        self.assertIn("name: stage-${{ matrix.name }}", workflow)
        self.assertNotIn("archive-", workflow)
        assets_workflow = (ROOT / ".github/workflows/release-assets.yml").read_text()
        self.assertIn("pattern: package-*", assets_workflow)

    def test_linux_stage_checks_elf_and_glibc_floor(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            readelf = root / "readelf"
            readelf.write_text('#!/bin/sh\ncat "$3"\n')
            readelf.chmod(0o755)
            target = "x86_64-unknown-linux-gnu"
            release = root / target / "release"
            release.mkdir(parents=True)
            env = dict(os.environ, PATH=f"{root}{os.pathsep}{os.environ['PATH']}",
                       CARGO_TARGET_DIR=str(root))
            elf = """ELF Header:
  Class:                             ELF64
  Data:                              2's complement, little endian
  Machine:                           Advanced Micro Devices X86-64
  Flags:                             0x0
      [Requesting program interpreter: /lib64/ld-linux-x86-64.so.2]
Version definition section '.gnu.version_d' contains 1 entry:
  0x0010:   Name: GLIBC_2.99
Version needs section '.gnu.version_r' contains 1 entry:
  0x0010:   Name: GLIBC_2.9  Flags: none  Version: 2
  0x0020:   Name: GLIBC_2.2.5  Flags: none  Version: 3
"""
            for program in PROGRAMS:
                path = release / program
                path.write_text(elf)
                path.chmod(0o755)
            spot = release / "catcheck"
            spot.write_text(elf.replace("GLIBC_2.9 ", "GLIBC_2.34 "))
            output = root / "stage"
            result = shell(RELEASE / "stage.sh", target, "linux-x86_64", str(output), env=env)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads((output / "stage.json").read_text())["glibc_floor"], "2.34")

            for binary, error in (
                (elf.replace("Advanced Micro Devices X86-64", "AArch64"), "Wrong ELF"),
                (elf.replace("/lib64/ld-linux-x86-64.so.2", "/lib/ld-linux.so.2"), "Wrong ELF"),
                (elf.replace("GLIBC_2.9 ", "GLIBC_2.36 "), "above the 2.35 limit"),
                (elf.replace("GLIBC_2.9 ", "GLIBC_ABI_DT_RELR "), "above the 2.35 limit"),
            ):
                with self.subTest(error=error, binary=binary):
                    spot.write_text(binary)
                    invalid = root / "invalid"
                    result = shell(RELEASE / "stage.sh", target, "linux-x86_64",
                                   str(invalid), env=env)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(error, result.stderr)
                    self.assertFalse(invalid.exists())

    def test_arm64_stage_rejects_glibc_above_floor(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            readelf = root / "readelf"
            readelf.write_text('#!/bin/sh\ncat "$3"\n')
            readelf.chmod(0o755)
            target = "aarch64-unknown-linux-gnu"
            release = root / target / "release"
            release.mkdir(parents=True)
            for name in PROGRAMS:
                path = release / name
                path.write_text("""ELF Header:
  Class: ELF64
  Data: 2's complement, little endian
  Machine: AArch64
      [Requesting program interpreter: /lib/ld-linux-aarch64.so.1]
Version needs section '.gnu.version_r' contains 1 entry:
  0x0010:   Name: GLIBC_2.36  Flags: none  Version: 2
""")
                path.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}{os.pathsep}{os.environ['PATH']}",
                       CARGO_TARGET_DIR=str(root))
            result = shell(RELEASE / "stage.sh", target, "linux-arm64", str(root / "stage"), env=env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("above the 2.35 limit", result.stderr)

    def test_fetch_rejects_wrong_digest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "packaging"
            (root / "release").mkdir(parents=True)
            fetch = root / "release/fetch-tool.py"
            shutil.copyfile(RELEASE / "fetch-tool.py", fetch)
            source = root / "tool"
            source.write_bytes(b"fixture download\n")
            lock = root / "tools.lock.json"
            pin = {"version": "1.0.0", "url": source.as_uri(), "sha256": "0" * 64}
            lock.write_text(json.dumps({"fixture": {"linux-x86_64": pin}}))
            output = root / "download"
            command = [sys.executable, str(fetch), "fixture", "linux-x86_64", str(output)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SHA-256", result.stderr)
            self.assertFalse(output.exists())

            pin["sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
            lock.write_text(json.dumps({"fixture": {"linux-x86_64": pin}}))
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output.read_bytes(), source.read_bytes())
