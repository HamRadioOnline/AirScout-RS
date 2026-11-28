from configparser import ConfigParser
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

try:
    import yaml
except ImportError:
    yaml = None


ROOT = Path(__file__).resolve().parents[2]
LINUX = ROOT / "packaging/linux"
APP_ID = "io.github.hamradioonline.AirScoutRS"
SERVICE = "airscout-planeserver.service"
CONFIG = "etc/airscout-planeserver/planeserver.toml"
PROGRAMS = {
    "airscout", "airscout-planeserver", "airscout-cli", "catcheck",
    "elevation-tile-gen", "maptile-gen",
}
ARCHES = {"x86_64", "arm64", "armhf", "i686"}
MATRIX = {"deb": ARCHES, "archlinux": ARCHES, "rpm": {"x86_64", "arm64"}}


def workspace_version():
    match = re.search(r'(?m)^\[workspace\.package\]\nversion = "([^"]+)"',
                      (ROOT / "Cargo.toml").read_text())
    assert match, "no [workspace.package] version in Cargo.toml"
    return match[1]


VERSION = workspace_version()


def stage_fixture(stage):
    (stage / "bin").mkdir(parents=True)
    for name in PROGRAMS | {"not-packaged"}:
        program = stage / "bin" / name
        program.write_text("#!/bin/sh\nexit 0\n")
        program.chmod(0o755)
    for name in ("README.md", "LICENSE", "NOTICE"):
        (stage / name).write_text(f"{name} fixture\n")
    (stage / "stage.json").write_text(json.dumps({
        "target": "armv7-unknown-linux-gnueabihf", "slug": "linux-armhf",
        "version": VERSION, "glibc_floor": "2.34",
    }))


def run(*args, env=None, check=True):
    return subprocess.run(args, cwd=ROOT, env=env, check=check, text=True, capture_output=True)


class LinuxTests(unittest.TestCase):
    def test_root_layout(self):
        with tempfile.TemporaryDirectory() as tmp:
            stage, root = Path(tmp) / "staged inputs", Path(tmp) / "install root"
            stage_fixture(stage)
            run(LINUX / "install-root.sh", stage, root)
            bins = list((root / "usr/bin").iterdir())
            installed_bins = {path.name for path in bins if path.is_file() and os.access(path, os.X_OK)}
            self.assertEqual(len(bins), len(PROGRAMS))
            desktop_dir = root / "usr/share/applications"
            self.assertEqual({path.name for path in desktop_dir.iterdir()}, {f"{APP_ID}.desktop"})
            desktop = ConfigParser()
            desktop.read(desktop_dir / f"{APP_ID}.desktop")
            desktop_execs = [section["Exec"] for section in desktop.values() if "Exec" in section]
            self.assertTrue(installed_bins == PROGRAMS and desktop_execs == ["airscout"])
            self.assertEqual(desktop["Desktop Entry"]["Icon"], APP_ID)

            icons = root / "usr/share/icons/hicolor"
            sizes = {16, 32, 48, 64, 128, 256}
            expected_icons = {Path(f"{size}x{size}/apps/{APP_ID}.png") for size in sizes}
            self.assertEqual({path.relative_to(icons) for path in icons.rglob("*.png")}, expected_icons)
            for size in sizes:
                path = icons / f"{size}x{size}/apps/{APP_ID}.png"
                self.assertEqual(path.read_bytes(), (LINUX.parent / f"icons/hicolor/{size}.png").read_bytes())

            metainfo = ET.parse(root / f"usr/share/metainfo/{APP_ID}.metainfo.xml")
            self.assertEqual(metainfo.findtext("id"), APP_ID)
            launchable = metainfo.find("launchable")
            self.assertEqual((launchable.attrib["type"], launchable.text), ("desktop-id", f"{APP_ID}.desktop"))
            for name in ("LICENSE", "NOTICE"):
                self.assertEqual((root / "usr/share/licenses/airscout-rs" / name).read_bytes(),
                                 (stage / name).read_bytes())

            (stage / "bin/catcheck").unlink()
            missing = Path(tmp) / "incomplete root"
            self.assertNotEqual(run(LINUX / "install-root.sh", stage, missing, check=False).returncode, 0)
            self.assertFalse(missing.exists())

    def test_format_matrix(self):
        workflow = (ROOT / ".github/workflows/release-linux.yml").read_text()
        if yaml:
            parsed = yaml.load(workflow, Loader=yaml.BaseLoader)
            self.assertIn("workflow_call", parsed["on"])
            rows = parsed["jobs"]["package"]["strategy"]["matrix"]["include"]
        else:
            jobs = dict(re.findall(r"^  ([\w-]+):\n((?:^(?:    .*|)$\n)*)", workflow, re.M))
            package = jobs["package"]
            self.assertIn("  workflow_call:\n", workflow)
            self.assertRegex(package, r"    strategy:\n(?:      fail-fast: false\n)?      matrix:\n        include:\n")
            rows = [{"format": fmt, "slug": slug} for fmt, slug in re.findall(
                r"^          - format: (\w+)\n            slug: (linux-[\w]+)$", package, re.M)]
        self.assertCountEqual([(row["format"], row["slug"]) for row in rows],
                              [(fmt, f"linux-{arch}") for fmt, arches in MATRIX.items() for arch in arches])

    def test_service_fresh_install(self):
        with tempfile.TemporaryDirectory() as tmp:
            stage, root = Path(tmp) / "stage", Path(tmp) / "root"
            stage_fixture(stage)
            run(LINUX / "install-root.sh", stage, root)
            unit = ConfigParser()
            self.assertEqual(unit.read(root / "usr/lib/systemd/system" / SERVICE),
                             [str(root / "usr/lib/systemd/system" / SERVICE)])
            service = unit["Service"]
            self.assertEqual(service["User"], "airscout-planeserver")
            self.assertEqual(service["StateDirectory"], "airscout-planeserver")
            self.assertEqual(service["ExecStart"],
                             "/usr/bin/airscout-planeserver --headless --config "
                             "/etc/airscout-planeserver/planeserver.toml --data /var/lib/airscout-planeserver")
            for key, value in {"NoNewPrivileges": "true", "ProtectSystem": "strict",
                               "PrivateTmp": "true", "Restart": "on-failure"}.items():
                self.assertEqual(service[key], value)

            self.assertEqual((root / "usr/lib/systemd/system-preset/00-airscout-planeserver.preset").read_text(),
                             f"disable {SERVICE}\n")
            self.assertRegex((root / "usr/lib/sysusers.d/airscout-planeserver.conf").read_text(),
                             r"(?m)^u\s+airscout-planeserver\s+-\s")
            self.assertEqual((root / CONFIG).read_bytes(), (LINUX / "planeserver.toml").read_bytes())

            env, log = self.fake_systemd(tmp)
            for hook, args in (
                ("deb-postinstall", ("configure",)),
                ("rpm-postinstall", ("1",)),
                ("arch-install", ("post_install", f"{VERSION}-1")),
            ):
                with self.subTest(hook=hook):
                    log.write_text("")
                    self.run_hook(hook, args, env)
                    calls = log.read_text().splitlines()
                    self.assertIn("systemd-sysusers airscout-planeserver.conf", calls)
                    self.assertIn(f"systemctl preset {SERVICE}", calls)
                    self.assertTrue(all(line.split()[1] in {"preset", "daemon-reload", "airscout-planeserver.conf"}
                                        for line in calls))

    def test_upgrade_keeps_enablement(self):
        with tempfile.TemporaryDirectory() as tmp:
            env, log = self.fake_systemd(tmp)
            for hook, args in (
                ("deb-postinstall", ("configure", f"{VERSION}~beta.1")),
                ("deb-preremove", ("upgrade", VERSION)),
                ("rpm-postinstall", ("2",)),
                ("rpm-preremove", ("1",)),
                ("arch-install", ("post_upgrade", f"{VERSION}-1", f"{VERSION}beta.1-1")),
            ):
                with self.subTest(hook=hook):
                    log.write_text("")
                    self.run_hook(hook, args, env, systemd_running=True)
                    calls = log.read_text().splitlines()
                    if hook in {"deb-postinstall", "rpm-postinstall"}:
                        self.assertEqual(calls[-2:], ["systemctl daemon-reload",
                                                      f"systemctl try-restart {SERVICE}"])
                        calls.pop()
                    self.assertTrue(all(line in {"systemctl daemon-reload",
                                                 "systemd-sysusers airscout-planeserver.conf"}
                                        for line in calls), calls)

    def fake_systemd(self, tmp):
        fake_bin = Path(tmp) / "bin"
        fake_bin.mkdir()
        log = Path(tmp) / "systemd-calls"
        for name in ("systemctl", "systemd-sysusers"):
            cmd = fake_bin / name
            cmd.write_text(f'#!/bin/sh\nprintf "%s %s\\n" {name} "$*" >> "$HOOK_LOG"\n')
            cmd.chmod(0o755)
        return dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}", HOOK_LOG=str(log)), log

    def run_hook(self, hook, args, env, systemd_running=False):
        script = LINUX / "scripts" / hook
        if systemd_running:
            with tempfile.TemporaryDirectory() as tmp:
                relocated = Path(tmp) / hook
                relocated.write_text(script.read_text().replace("/run/systemd/system", tmp))
                return self.execute_hook(relocated, args, env)
        self.execute_hook(script, args, env)

    def execute_hook(self, script, args, env):
        hook = script.name
        if hook == "arch-install":
            run("bash", "-c", '. "$1"; shift; "$@"', "test", script, *args, env=env)
        else:
            run("sh", script, *args, env=env)

    def test_package_contract(self):
        with tempfile.TemporaryDirectory() as tmp:
            temp = Path(tmp)
            stage = temp / "stage"
            stage_fixture(stage)
            fake = temp / "nfpm"
            fake.write_text(
                '#!/bin/sh\n'
                'set -eu\n'
                '[ "$1" = package ] && [ "$2" = --config ] && [ -f "$3" ] && '
                '[ "$4" = --packager ] && [ "$6" = --target ] || exit 2\n'
                'printf "%s|%s|%s|%s\\n" "$5" "$PKG_ARCH" "$PKG_VERSION" "$PKG_RELEASE" > "$7"\n'
            )
            fake.chmod(0o755)
            env = dict(os.environ, PATH=f"{temp}:{os.environ['PATH']}")
            for fmt, extension, arch, version, release in (
                ("deb", "deb", "arm7", f"{VERSION}~beta.rc.2", ""),
                ("archlinux", "pkg.tar.zst", "arm7", f"{VERSION}beta.rc.2", "1"),
                ("rpm", "rpm", "arm64", VERSION, "0.beta.rc.2"),
            ):
                with self.subTest(fmt=fmt):
                    if fmt == "rpm":
                        (stage / "stage.json").write_text(json.dumps({
                            "target": "aarch64-unknown-linux-gnu", "slug": "linux-arm64",
                            "version": VERSION, "glibc_floor": "2.34",
                        }))
                    slug = "linux-arm64" if fmt == "rpm" else "linux-armhf"
                    tag = f"beta-v{VERSION}-rc.2"
                    output = temp / f"output {fmt}"
                    run(LINUX / "package.sh", stage, tag, fmt, output, env=env)
                    asset = f"AirScout-RS-{tag}-{slug}.{extension}"
                    self.assertEqual((output / "assets" / asset).read_text().strip(),
                                     f"{fmt}|{arch}|{version}|{release}")
                    self.assertEqual(json.loads((output / "result.json").read_text()), {
                        "format": fmt, "slug": slug, "assets": [asset],
                        "signing": "not_applicable", "glibc_floor": "2.34",
                    })
            fake.write_text("#!/bin/sh\nexit 1\n")
            failed = temp / "failed output"
            self.assertNotEqual(run(LINUX / "package.sh", stage, f"v{VERSION}", "deb", failed,
                                    env=env, check=False).returncode, 0)
            self.assertFalse(failed.exists())
