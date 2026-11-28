import json
import os
from pathlib import Path
import re
import shutil
import tempfile
import unittest

from test_linux import PROGRAMS, ROOT, run, yaml


class AppImageTests(unittest.TestCase):
    def test_apprun_dispatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            appdir = Path(tmp) / "AirScout RS.AppDir"
            binary_dir = appdir / "usr/bin"
            binary_dir.mkdir(parents=True)
            launcher = appdir / "AppRun"
            shutil.copy2(ROOT / "packaging/appimage/AppRun", launcher)

            for name in PROGRAMS:
                binary = binary_dir / name
                binary.write_text('#!/bin/sh\nprintf "%s\\n" "${0##*/}" "$@" "$PATH"\n')
                binary.chmod(0o755)

            # If the launcher's tool selection traverses usr/bin, this would run.
            outside = binary_dir.parent / "outside"
            shutil.copy2(binary_dir / "airscout", outside)
            env = dict(os.environ, PATH="/usr/bin:/bin")

            def dispatch(*args):
                *output, path = run(launcher, *args, env=env).stdout.splitlines()
                self.assertEqual(path, env["PATH"])
                return output

            self.assertEqual(dispatch("--tool", "airscout-cli", "--help"),
                             ["airscout-cli", "--help"])
            self.assertEqual(dispatch("--help"), ["airscout", "--help"])
            link = Path(tmp) / "linked launcher"
            link.symlink_to(launcher)
            self.assertEqual(run(link, "--help", env=env).stdout.splitlines(),
                             ["airscout", "--help", env["PATH"]])
            for name in PROGRAMS:
                self.assertEqual(dispatch("--tool", name, "arg with spaces"),
                                 [name, "arg with spaces"])
            for args in (("--tool", "../outside"), ("--tool",), ("--tool", "unknown")):
                with self.subTest(args=args):
                    result = run(launcher, *args, env=env, check=False)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(result.stdout, "")

    def test_gtk_caches_follow_relocated_appdir(self):
        with tempfile.TemporaryDirectory() as tmp:
            temp = Path(tmp)
            appdir = temp / "Extracted AppDir"
            libexec = appdir / "usr/libexec"
            libexec.mkdir(parents=True)
            launcher = appdir / "AppRun"
            shutil.copy2(ROOT / "packaging/appimage/AppRun", launcher)

            for helper, module in (
                ("gtk-query-immodules-3.0", "gtk-3.0/3.0.0/immodules/im-xim.so"),
                ("gdk-pixbuf-query-loaders", "gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.so"),
            ):
                script = libexec / helper
                script.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
                script.chmod(0o755)
                path = appdir / "usr/lib" / module
                path.parent.mkdir(parents=True)
                path.touch()

            server = appdir / "usr/bin/airscout-planeserver"
            server.parent.mkdir()
            server.write_text(
                '#!/bin/sh\nset -eu\n'
                'if [ -d /proc/self/fd ]; then\n'
                '    cat "$GTK_IM_MODULE_FILE" "$GDK_PIXBUF_MODULE_FILE"\n'
                'else\n'
                '    cat <&3; cat <&4\n'
                'fi\n'
            )
            server.chmod(0o755)
            env = dict(os.environ, TMPDIR=tmp)
            result = run(launcher, "--tool", "airscout-planeserver", env=env)
            self.assertEqual(result.stdout.splitlines(), [
                str(appdir.resolve() / "usr/lib/gtk-3.0/3.0.0/immodules/im-xim.so"),
                str(appdir.resolve() / "usr/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.so"),
            ])
            self.assertEqual(list(temp.iterdir()), [appdir])

    def test_planeserver_isolates_module_search(self):
        with tempfile.TemporaryDirectory() as tmp:
            appdir = Path(tmp) / "Extracted AppDir"
            server = appdir / "usr/bin/airscout-planeserver"
            server.parent.mkdir(parents=True)
            server.write_text(
                '#!/bin/sh\nprintf "%s\\n" "${GIO_MODULE_DIR-}" "${GTK_EXE_PREFIX-}" '
                '"${GTK_PATH-}" "${GIO_EXTRA_MODULES+set}"\n'
            )
            server.chmod(0o755)
            launcher = appdir / "AppRun"
            shutil.copy2(ROOT / "packaging/appimage/AppRun", launcher)
            env = dict(os.environ, GIO_MODULE_DIR="/host/gio/modules",
                       GTK_EXE_PREFIX="/host", GTK_PATH="/host/gtk-3.0",
                       GIO_EXTRA_MODULES="/host/extra-modules")
            result = run(launcher, "--tool", "airscout-planeserver", env=env)
            usr = appdir.resolve() / "usr"
            self.assertEqual(result.stdout.splitlines(), [
                str(usr / "lib/gio/modules"), str(usr), str(usr / "lib/gtk-3.0"), "",
            ])

    def test_common_license_references_exclude_sentence_periods(self):
        build = (ROOT / "packaging/appimage/build.sh").read_text()
        pattern = re.search(r're\.findall\(r"(/usr/share/common-licenses/[^\n"]+)"', build)
        self.assertIsNotNone(pattern)
        copyright_text = (
            'See /usr/share/common-licenses/LGPL-2.1.\n'
            'See /usr/share/common-licenses/GPL-2.\n'
            'See "/usr/share/common-licenses/Apache-2.0".\n'
        )
        self.assertEqual(re.findall(pattern[1], copyright_text), ["LGPL-2.1", "GPL-2", "Apache-2.0"])

    def test_workflow_structure(self):
        workflow = (ROOT / ".github/workflows/release-appimage.yml").read_text()
        expected_rows = [
            {"slug": "linux-x86_64", "os": "ubuntu-22.04"},
            {"slug": "linux-arm64", "os": "ubuntu-22.04-arm"},
            {"slug": "linux-armhf", "os": "ubuntu-22.04", "dockerfile": "armhf", "deb_arch": "armhf"},
            {"slug": "linux-i686", "os": "ubuntu-22.04", "dockerfile": "i686", "deb_arch": "i386"},
        ]
        runner = "${{ matrix.os }}"
        artifacts = {
            "download-artifact": "stage-${{ matrix.slug }}",
            "upload-artifact": "package-appimage-${{ matrix.slug }}",
        }
        if yaml:
            parsed = yaml.load(workflow, Loader=yaml.BaseLoader)
            tag = parsed["on"]["workflow_call"]["inputs"]["tag"]
            self.assertEqual(tag, {"required": "true", "type": "string"})
            package = parsed["jobs"]["package"]
            self.assertEqual(package["runs-on"], runner)
            rows = package["strategy"]["matrix"]["include"]
            steps = package["steps"]
            for action, artifact in artifacts.items():
                self.assertEqual([
                    step["with"]["name"] for step in steps
                    if step.get("uses", "").startswith(f"actions/{action}@")
                ], [artifact])
        else:
            self.assertRegex(workflow, r"(?m)^on:\n  workflow_call:\n    inputs:\n"
                             r"      tag:\n        required: true\n        type: string$")
            package = re.search(r"(?m)^jobs:\n  package:\n((?:^    .*\n|^\n)*)", workflow)
            self.assertIsNotNone(package)
            self.assertIn(f"\n    runs-on: {runner}\n", package[1])
            matrix = re.search(r"(?m)^    strategy:\n(?:      fail-fast: false\n)?      matrix:\n        include:\n"
                               r"((?:^          .+\n)+)", package[1])
            self.assertIsNotNone(matrix)
            rows = [dict(line.strip().split(": ", 1) for line in row.splitlines())
                    for row in matrix[1].split("          - ")[1:]]
            for action, artifact in artifacts.items():
                self.assertEqual(re.findall(
                    rf"(?m)^      - uses: actions/{action}@\S+ +# v\d+\.\d+\.\d+\n        with:\n          name: (.+)$",
                    package[1],
                ), [artifact])
        self.assertCountEqual(rows, expected_rows)

    def test_pinned_appimagetool_and_runtime_license(self):
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        for tool, platforms in (
            ("appimagetool", {"linux-x86_64", "linux-arm64"}),
            ("appimage-runtime-license", {"all"}),
        ):
            self.assertEqual(set(lock[tool]), platforms)
            for platform, pin in lock[tool].items():
                with self.subTest(tool=tool, platform=platform):
                    self.assertRegex(pin["sha256"], r"^[0-9a-f]{64}$")
                    self.assertNotEqual(pin["sha256"], "0" * 64)

    def test_pinned_runtimes(self):
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        runtimes = lock["appimage-runtime"]
        self.assertEqual(set(runtimes), {"x86_64", "aarch64", "armhf", "i686"})
        for arch, pin in runtimes.items():
            with self.subTest(arch=arch):
                self.assertRegex(pin["sha256"], r"^[0-9a-f]{64}$")
                self.assertNotEqual(pin["sha256"], "0" * 64)
                self.assertEqual(
                    pin["url"],
                    f'https://github.com/AppImage/type2-runtime/releases/download/{pin["version"]}/runtime-{arch}',
                )
                self.assertRegex(pin["version"], r"^[0-9]{8}$")
