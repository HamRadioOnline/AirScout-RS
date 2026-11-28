import json
import os
from pathlib import Path
import posixpath
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

from test_release import ROOT, WORKFLOWS, workflow_jobs

HOSTED = ROOT / "packaging/tests/hosted"
TOOLS = ("airscout", "airscout-planeserver", "airscout-cli", "catcheck",
         "elevation-tile-gen", "maptile-gen")

FAKE_TOOL = r"""
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import sys

assert not ({"DISPLAY", "WAYLAND_DISPLAY", "HTTP_PROXY", "SMOKE_SECRET"} & os.environ.keys())
root = Path(os.environ["HOME"]).parent
for key in ("HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "TMPDIR"):
    assert Path(os.environ[key]).parent == root and Path(os.environ[key]).is_dir()
with Path(__file__).with_name("calls").open("a") as calls:
    calls.write(json.dumps([Path(__file__).name, sys.argv[1:]]) + "\n")

if sys.argv[1] == "--version":
    print(Path(__file__).name, "1.2.3")
elif sys.argv[1] == "--help":
    print("Usage: " + Path(__file__).name)
else:
    import time
    assert sys.argv[1] == "--headless"
    opts = dict(zip(sys.argv[2::2], sys.argv[3::2]))
    config = Path(opts["--config"])
    assert config.is_relative_to(root) and "[none]" in config.read_text()
    assert Path(opts["--data"]).is_relative_to(root)
    assert opts["--bind"] == "127.0.0.1" and opts["--port"] == "0"
    assert 0 < int(opts["--run-for"]) <= 15
    mode = Path(__file__).with_name("mode").read_text()
    Path(__file__).with_name("daemon").write_text(json.dumps([os.getpid(), str(root)]))

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            assert self.path == "/planes.json"
            self.send_response(503 if mode == "status" else 200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b"{}" if mode == "shape" else b'{"version":1,"full_count":0}')

    if mode == "slow":
        time.sleep(8.5)

    with HTTPServer((opts["--bind"], int(opts["--port"])), Handler) as server:
        print(f"airscout-planeserver: serving /planes.json on http://127.0.0.1:{server.server_port}/ (no authentication)",
              file=sys.stderr, flush=True)
        if mode != "timeout":
            server.handle_request()
        if mode not in {"ok", "slow"}:
            time.sleep(60)
"""


class HostedContractTests(unittest.TestCase):
    def test_direct_script_calls_use_executable_git_modes(self):
        index = subprocess.check_output(["git", "ls-files", "-s", "-z"], cwd=ROOT, text=True)
        modes = {path: info.split()[0]
                 for info, path in (entry.split("\t", 1) for entry in index.split("\0") if entry)}
        # Recognize command position in workflow run steps and shell scripts,
        # including env assignments and command substitutions. An interpreter
        # such as python3 or bash in command position needs no mode on its input.
        directories = {
            "packaging": "packaging",
            "linux_dir": "packaging/linux",
            "RELEASE_DIR": "packaging/release",
            "CONTAINER_DIR": "packaging/container",
            "hosted": "packaging/tests/hosted",
        }
        directory = "|".join(directories)
        call = re.compile(
            r'(?:^|\$\(|[;&|])\s*(?:-?\s*run:\s*)?(?:(?:if|then|do|exec|!)\s+)*'
            r'(?:[A-Za-z_][A-Za-z_0-9]*=(?:"[^"]*"|[^\s"\'(]+)\s+)*'
            rf'["\']?(?P<script>(?:\./|packaging/|tools/|\$(?:{directory})/)[\w./-]+)'
            r'(?=["\'\s);&|]|$)'
        )
        invocations = set()
        callers = [path for path in modes
                   if path.startswith((".github/workflows/", "packaging/", "tools/"))
                   and (Path(path).suffix in {".sh", ".ps1", ".yml", ".yaml"}
                        or Path(path).name in {"PKGBUILD", "AppRun"})]
        for caller in callers:
            for line in (ROOT / caller).read_text().splitlines():
                if line.lstrip().startswith("#"):
                    continue
                for match in call.finditer(line):
                    script = match["script"]
                    if script.startswith("$"):
                        variable, suffix = script[1:].split("/", 1)
                        script = f"{directories[variable]}/{suffix}"
                    script = posixpath.normpath(script)
                    if Path(script).suffix in {".sh", ".py", ".ps1"} or script in modes:
                        invocations.add((caller, script))
        self.assertIn((".github/workflows/release-flatpak.yml", "packaging/flatpak/stage.sh"), invocations)
        self.assertIn(("packaging/container/build.sh", "packaging/tests/hosted/verify-container.sh"), invocations)
        self.assertNotIn(("packaging/container/build.sh", "packaging/container/oci_archive.py"), invocations)
        for caller, script in sorted(invocations):
            with self.subTest(caller=caller, script=script):
                self.assertEqual(modes.get(script), "100755")

    def test_armhf_builds_register_emulation_before_using_cross_image(self):
        for filename, job, matrix_key, build in (
            ("release.yml", "build", "name", "cross build"),
            ("release-appimage.yml", "package", "slug", "docker build"),
        ):
            with self.subTest(workflow=filename):
                workflow = (WORKFLOWS / filename).read_text()
                job_text = re.search(rf"(?ms)^  {job}:\n(.*?)(?=^  [\w-]+:|\Z)", workflow)[1]
                self.assertRegex(
                    job_text, rf"(?m)^      - name: Register armhf emulation\n"
                    rf"        if: matrix\.{matrix_key} == 'linux-armhf'\n"
                    r"        run: packaging/cross/register-arm-emulation\.sh arm$",
                )
                self.assertLess(job_text.index("Register armhf emulation"), job_text.index(build))

    def test_armhf_appimage_smoke_invokes_qemu_explicitly(self):
        script = (HOSTED / "verify-package.sh").read_text()
        self.assertIn('docker cp "$emulator_container:/usr/bin/qemu-arm"', script)
        self.assertIn('--env APPIMAGE_QEMU=/qemu-arm', script)
        self.assertIn('"$APPIMAGE_QEMU" ./image ./image --appimage-extract', script)

    @unittest.skipIf(os.name == "nt", "Fake Docker requires a POSIX shebang")
    def test_native_package_smoke_uses_pin_for_target_platform(self):
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        workflow = (WORKFLOWS / "release-linux.yml").read_text()
        targets = re.findall(r"- format: (deb|rpm)\s+slug: (\S+)", workflow)
        self.assertTrue(targets)
        platforms = {
            "linux-x86_64": "linux/amd64",
            "linux-arm64": "linux/arm64",
            "linux-armhf": "linux/arm/v7",
            "linux-i686": "linux/386",
        }
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            docker = root / "docker"
            docker.write_text(
                "#!/usr/bin/env bash\n"
                'if [[ "$2" != --rm || "$3" != --privileged ]]; then printf "%s\\n" "$@"; fi\n'
            )
            docker.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}{os.pathsep}{os.environ['PATH']}", VERIFY_MODE="")
            for fmt, slug in targets:
                with self.subTest(format=fmt, slug=slug):
                    asset = root / f"AirScout-RS-alpha-v1.2.3-{slug}.{fmt}"
                    asset.touch()
                    result = subprocess.run(
                        ["bash", HOSTED / "verify-package.sh", fmt, asset, "1.2.3"],
                        stdin=subprocess.DEVNULL, env=env, capture_output=True, text=True,
                        timeout=5,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    args = result.stdout.splitlines()
                    self.assertEqual(args[:4], ["run", "--rm", "--platform", platforms[slug]])
                    if fmt == "rpm":
                        pins = lock["rpm-version-check"]
                    elif slug == "linux-i686":
                        pins = lock.get("package-smoke-base", {})
                    else:
                        pins = lock["container-base"]
                    self.assertIn(slug, pins)
                    pin = pins[slug]
                    self.assertEqual(pin.get("platform"), platforms[slug])
                    self.assertRegex(pin["digest"], r"^sha256:[0-9a-f]{64}$")
                    self.assertTrue(pin["source"].endswith("/" + pin["digest"]))
                    self.assertEqual(args[-6:], [
                        f'{pin["url"]}@{pin["digest"]}', "bash",
                        "/workspace/packaging/tests/hosted/verify-package.sh",
                        fmt, f"/asset/{asset.name}", "1.2.3",
                    ])

    @unittest.skipIf(os.name == "nt", "Fake Docker requires a POSIX shebang")
    def test_foreign_arch_payload_unpacks_with_host_tar(self):
        # Ubuntu's patched GNU tar resolves parents with openat2(), which the
        # musl-built binfmt qemu-aarch64 breaks with EINVAL. The emulated target
        # userspace must inspect a payload unpacked natively on the host.
        script = (HOSTED / "verify-package.sh").read_text()
        container_arch = script.split('elif [[ "$format" == archlinux ]]; then', 1)[1].split(
            'elif [[ "$format" == appimage ]]; then', 1)[0]
        self.assertNotRegex(container_arch, r"\btar\b")
        self.assertIn('root=/payload/usr', container_arch)
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            log = root / "calls"
            (root / "docker").write_text(
                "#!/usr/bin/env bash\n"
                'if [[ "$2" != --rm || "$3" != --privileged ]]; then\n'
                '  for arg in "$@"; do printf "docker %s\\n" "$arg"; done >> "$CALLS"\n'
                '  for arg in "$@"; do\n'
                '    case "$arg" in type=bind,source=*,target=/payload,readonly)\n'
                '      dir=${arg#type=bind,source=}; dir=${dir%%,*}\n'
                '      [[ -f "$dir/.PKGINFO" ]] && printf "payload %s\\n" "$(stat -c %a "$dir" 2>/dev/null'
                ' || stat -f %Lp "$dir")" >> "$CALLS" ;;\n'
                '    esac\n'
                '  done\n'
                "fi\n"
            )
            (root / "tar").write_text(
                "#!/usr/bin/env bash\n"
                'printf "tar %s\\n" "$*" >> "$CALLS"\n'
                'while (($#)); do [[ "$1" == -C ]] && { touch "$2/.PKGINFO"; exit 0; }; shift; done\n'
                "exit 1\n"
            )
            for tool in ("docker", "tar"):
                (root / tool).chmod(0o755)
            env = dict(os.environ, PATH=f"{root}{os.pathsep}{os.environ['PATH']}", VERIFY_MODE="",
                       CALLS=str(log), TMPDIR=str(root))
            for slug in ("linux-arm64", "linux-armhf", "linux-i686", "linux-x86_64"):
                with self.subTest(slug=slug):
                    log.write_text("")
                    asset = root / f"AirScout-RS-alpha-v1.2.3-{slug}.pkg.tar.zst"
                    asset.touch()
                    result = subprocess.run(
                        ["bash", HOSTED / "verify-package.sh", "archlinux", asset, "1.2.3"],
                        stdin=subprocess.DEVNULL, env=env, capture_output=True, text=True,
                        timeout=5,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    calls = log.read_text().splitlines()
                    tar_calls = [c for c in calls if c.startswith("tar ")]
                    mounts = [c for c in calls if c.endswith(",target=/payload,readonly")]
                    leftovers = list(root.glob("airscout-payload.*"))
                    self.assertEqual(leftovers, [])
                    if slug == "linux-x86_64":
                        # The x86_64 package installs natively with pacman.
                        self.assertEqual(tar_calls, [])
                        self.assertEqual(mounts, [])
                        continue
                    self.assertEqual(len(tar_calls), 1)
                    self.assertRegex(tar_calls[0], rf"^tar --zstd -xf {re.escape(str(asset.resolve()))} -C ")
                    self.assertEqual(len(mounts), 1)
                    # runuser -u nobody executes the payload, so it must be traversable.
                    self.assertIn("payload 755", calls)
                    self.assertLess(calls.index(tar_calls[0]), calls.index(mounts[0]))

    def test_rpm_smoke_installs_runuser(self):
        script = (HOSTED / "verify-package.sh").read_text()
        self.assertRegex(script, r"dnf -y install [^\n]*\butil-linux(?=\s|$)")

    def test_flatpak_build_installs_elfutils(self):
        workflow = (WORKFLOWS / "release-flatpak.yml").read_text()
        build = workflow.split("\n  build:\n", 1)[1].split("\n  locate-base:\n", 1)[0]
        install = build.split("      - name: Install pinned Flatpak tooling\n", 1)[1]
        install = install.split("      - ", 1)[0]
        self.assertNotIn("        if:", install)
        self.assertRegex(install, r"apt-get install [^\n]*\belfutils\b")

    def test_release_build_package_and_verify_matrices_disable_fail_fast(self):
        for workflow in sorted(WORKFLOWS.glob("release*.yml")):
            jobs = workflow.read_text().split("\njobs:\n", 1)[1]
            blocks = re.split(r"(?m)^  ([\w-]+):\n", jobs)
            for job, block in zip(blocks[1::2], blocks[2::2]):
                if (workflow.name, job) == ("release.yml", "test"):
                    continue
                strategy = re.search(r"(?m)^    strategy:\n((?:^ {6}[^\n]*\n)+)", block)
                if strategy and re.search(r"(?m)^      matrix:", strategy[1]):
                    with self.subTest(workflow=workflow.name, job=job):
                        self.assertRegex(strategy[1], r"(?m)^      fail-fast: false$")

    def test_external_actions_have_versioned_commit_pins(self):
        pins = re.compile(r"[\w.-]+/[\w./-]+@[0-9a-f]{40} +# v\d+\.\d+\.\d+")
        for path in sorted(WORKFLOWS.glob("*.y*ml")):
            for number, line in enumerate(path.read_text().splitlines(), 1):
                match = re.match(r"^\s*(?:-\s+)?uses:\s*(.+?)\s*$", line)
                if match and not match[1].startswith("./"):
                    with self.subTest(workflow=path.name, line=number):
                        self.assertIsNotNone(pins.fullmatch(match[1]), match[1])

    def test_packaging_tests_gate_ci_and_release(self):
        for name, job in (("ci.yml", "linux"), ("release.yml", "test")):
            with self.subTest(workflow=name):
                workflow = (WORKFLOWS / name).read_text()
                block = re.search(rf"(?ms)^  {job}:\n(.*?)(?=^  [a-z][a-z-]*:|\Z)", workflow)[1]
                self.assertIn("ubuntu-22.04", block)
                self.assertRegex(block, r"apt-get install .*\bgnupg\b")
                self.assertIn("python3 -m unittest discover -s packaging/tests -v </dev/null", block)
                if name == "release.yml":
                    self.assertRegex(block, r"(?m)^ +if: runner.os == 'Linux'\n +run: python3 -m unittest")

    def test_package_smoke_matrix(self):
        jobs = workflow_jobs("release.yml")
        needs = jobs["release"]["needs"]
        if isinstance(needs, str):
            needs = re.findall(r"[\w-]+", needs)
        linux = {"linux-x86_64", "linux-arm64", "linux-armhf", "linux-i686"}
        cases = {
            "linux": ("package", "verify-package.sh", "slug", linux),
            "appimage": ("package", "verify-package.sh", "slug", linux),
            "windows": ("verify", "verify-windows.ps1", "slug",
                        {"windows-x86_64", "windows-arm64", "windows-i686"}),
            "macos": ("sign", "verify-macos.sh", "slug", {"macos-x86_64", "macos-arm64"}),
            "flatpak": ("build", "verify-flatpak.sh", "arch", {"x86_64", "aarch64"}),
            "container": ("build", "verify-container.sh", None,
                          {"linux/amd64", "linux/arm64", "linux/arm/v7"}),
        }
        self.assertNotIn("if", jobs["release"])
        for name, (job, smoke, matrix_key, targets) in cases.items():
            with self.subTest(package=name):
                self.assertIn(name, needs)
                self.assertEqual(jobs[name]["uses"], f"./.github/workflows/release-{name}.yml")
                self.assertNotIn("if", jobs[name])
                filename = f"release-{name}.yml"
                workflow = (WORKFLOWS / filename).read_text()
                job_text = re.search(rf"(?ms)^  {job}:\n(.*?)(?=^  [a-z][a-z-]*:|\Z)", workflow)[1]
                hooks = job_text
                if matrix_key:
                    matrix = job_text.split("strategy:", 1)[1].split("    steps:", 1)[0]
                    actual = set(re.findall(rf"(?m)^ +(?:- )?{matrix_key}: ([\w-]+)$", matrix))
                    self.assertEqual(actual, targets)
                    self.assertIn(smoke, hooks)
                    if name != "windows":
                        self.assertLess(hooks.index(smoke), hooks.index("actions/upload-artifact"))
                if name == "windows":
                    self.assertIn("finalize", workflow_jobs(filename)[job]["needs"])
                    for fmt in ("msi", "zip"):
                        self.assertIn(f"name: package-{fmt}-", job_text)
                if name == "container":
                    self.assertIn("packaging/container/build.sh", hooks)
                    self.assertLess(hooks.index("packaging/container/build.sh"),
                                    hooks.index("actions/upload-artifact"))
                    hooks += (ROOT / "packaging/container/build.sh").read_text()
                    self.assertIn(f"{smoke}\" probe", hooks)
                    for target in targets:
                        self.assertIn(target, hooks)
                if name in {"linux", "appimage", "container"}:
                    self.assertNotRegex(hooks + (HOSTED / smoke).read_text(),
                                        r"\b(?:cargo|cross)\s+(?:build|install|run)\b")
        workflow = (WORKFLOWS / "release-linux.yml").read_text()
        matrix = {name: set(re.findall(rf"- format: {name}\s+slug: (\S+)", workflow))
                  for name in ("deb", "rpm", "archlinux")}
        self.assertEqual(matrix, {"deb": linux, "rpm": {"linux-x86_64", "linux-arm64"}, "archlinux": linux})
        for channel in ("homebrew", "winget", "flatpak", "container", "aur"):
            publisher = jobs[f"publish-{channel}"]
            self.assertIn("release", publisher["needs"])
            self.assertIn(f"needs.release.outputs.{channel}_eligible == 'true'", publisher["if"])

    @unittest.skipUnless(shutil.which("bash"), "Bash unavailable")
    def test_aur_publish_selects_one_primary_package(self):
        workflow = (WORKFLOWS / "aur-publish.yml").read_text()
        selection = workflow.split("makepkg -s --noconfirm\n", 1)[1].split(
            '              for tool in ', 1)[0]
        script = r'''
runuser() { printf '%s\n' "$PACKAGES"; return "$LIST_STATUS"; }
bsdtar() { test -s "$2" || return 1; printf '%s\n' "$2"; }
pkg=airscout-rs-bin
''' + textwrap.dedent(selection) + "cat members\n"
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            main = "airscout-rs-bin-1.2.3-1-x86_64.pkg.tar.zst"
            debug = "airscout-rs-bin-debug-1.2.3-1-x86_64.pkg.tar.zst"
            extra = "airscout-rs-bin-doc-1.2.3-1-x86_64.pkg.tar.zst"
            for name in (main, debug, extra):
                (root / name).write_text("fixture")
            for packages, list_status, accepted in (
                ([main, debug], "0", True),
                ([main, debug, extra], "0", False),
                ([debug], "0", False),
                (["missing.pkg.tar.zst", debug], "0", False),
                ([main, debug], "1", False),
            ):
                with self.subTest(packages=packages, list_status=list_status):
                    result = subprocess.run(
                        ["bash", "-euc", script], cwd=root, stdin=subprocess.DEVNULL,
                        env=dict(os.environ, PACKAGES="\n".join(packages), LIST_STATUS=list_status),
                        capture_output=True, text=True, timeout=5,
                    )
                    if accepted:
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertEqual(result.stdout.splitlines(), [main])
                    else:
                        self.assertNotEqual(result.returncode, 0)

    def test_appimage_smoke_installs_only_host_libraries(self):
        script = (HOSTED / "verify-package.sh").read_text()
        appimage = re.search(r'(?ms)^        elif \[\[ "\$format" == appimage \]\]; then\n'
                             r'(.*?)(?=^        (?:elif|else|fi)\b)', script)
        self.assertIsNotNone(appimage, "AppImage must have a separate install branch")
        packages = re.search(r"apt-get install -y --no-install-recommends (.+)$",
                             appimage[1].replace("\\\n", " "), re.M)
        self.assertIsNotNone(packages)
        self.assertEqual(set(packages[1].split()), {
            "libx11-6", "libx11-xcb1", "libxcb-render0", "libxcb-shm0", "libxcb-xkb1",
            "libwayland-client0", "libwayland-cursor0", "libwayland-egl1",
            "libgl1", "libegl1", "libgl1-mesa-dri", "libvulkan1",
        })
        arch = script.split('elif [[ "$format" == archlinux ]]; then', 1)[1].split(
            'elif [[ "$format" == appimage ]]; then', 1)[0]
        self.assertIn('apt-get install -y --no-install-recommends libgtk-3-0 libasound2', arch)
        self.assertIn("libayatana-appindicator3-1 libxdo3 libxkbcommon-x11-0 librsvg2-common", arch)

    def test_ghcr_acceptance_has_read_only_token(self):
        workflow = (WORKFLOWS / "release-container.yml").read_text()
        job = workflow.split("\n  verify-published:\n", 1)
        self.assertEqual(len(job), 2, "GHCR verification requires its own read-only job")
        self.assertIn("published: ${{ steps.publish.outputs.published }}", job[0])
        self.assertIn("    needs: publish\n", job[1])
        self.assertIn("needs.publish.outputs.published == 'true'", job[1])
        self.assertIn("    permissions:\n      contents: read\n      packages: read\n", job[1])
        self.assertIn("GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}", job[1])
        self.assertNotIn("packages: write", job[1])

    @unittest.skipIf(os.name == "nt", "Fake Docker requires a POSIX shebang")
    def test_ghcr_verifier_isolates_login_and_cleans_up(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            caller_config = root / "caller-config"
            caller_config.mkdir()
            (caller_config / "config.json").write_text("caller fixture")
            docker = root / "docker"
            docker.write_text(r'''#!/usr/bin/env bash
set -eu
if [[ "$1" == login ]]; then
    test "$*" = 'login ghcr.io --username fixture-actor --password-stdin'
    test "$DOCKER_CONFIG" != "$CALLER_CONFIG"
    test -d "$DOCKER_CONFIG"
    printf '%s\n' "$DOCKER_CONFIG" > "$FIXTURE/docker-config"
    test "$(cat "$DOCKER_CONFIG/config.json")" = '{"auths":{"ghcr.io":{}}}'
    test -z "${DOCKER_AUTH_CONFIG+x}"
    IFS= read -r password
    test "$password" = fixture-token
    printf '%s\n' "$password" >&2
    test "$LOGIN_FAIL" = 0
    touch "$DOCKER_CONFIG/login-ok"
elif [[ "$1" == pull ]]; then
    test -f "$DOCKER_CONFIG/login-ok"
    test -z "${GITHUB_TOKEN+x}"
    test -z "${DOCKER_AUTH_CONFIG+x}"
    touch "$FIXTURE/pulled"
    exit 42
else
    exit 2
fi
''')
            docker.chmod(0o755)
            (root / "result.json").write_text(json.dumps({"images": {
                platform: {"platform": platform, "digest": "sha256:" + "a" * 64}
                for platform in ("linux/amd64", "linux/arm64", "linux/arm/v7")
            }}))
            for login_fail in ("1", "0"):
                with self.subTest(login_fail=login_fail):
                    env = dict(os.environ, PATH=f"{root}{os.pathsep}{os.environ['PATH']}",
                               DOCKER_CONFIG=str(caller_config), CALLER_CONFIG=str(caller_config),
                               FIXTURE=str(root), LOGIN_FAIL=login_fail,
                               DOCKER_AUTH_CONFIG="caller auth fixture", GITHUB_ACTOR="fixture-actor", GITHUB_TOKEN="fixture-token")
                    result = subprocess.run(
                        ["bash", HOSTED / "verify-container.sh", "published", root],
                        stdin=subprocess.DEVNULL, env=env, capture_output=True, text=True, timeout=5,
                    )
                    self.assertNotIn(env["GITHUB_TOKEN"], result.stdout + result.stderr)
                    self.assertTrue((root / "docker-config").is_file(), "Docker login was never called")
                    self.assertEqual((root / "pulled").exists(), login_fail == "0")
                    if login_fail == "1":
                        self.assertIn("GHCR login failed", result.stderr)
                    else:
                        self.assertEqual(result.returncode, 42, result.stderr)
                    config = Path((root / "docker-config").read_text().strip())
                    self.assertFalse(config.exists())
                    self.assertEqual((caller_config / "config.json").read_text(), "caller fixture")

    def test_flatpak_pages_uses_prerewrite_base_archive(self):
        workflow = (WORKFLOWS / "release-flatpak.yml").read_text()
        publish, pages = workflow.split("\n  verify-pages:\n", 1)
        self.assertLess(publish.index("git archive FETCH_HEAD | tar -x -C previous"),
                        publish.index("packaging/flatpak/publish-repo.sh prepared/repo"))
        self.assertIn("tar -xzf published/previous.tar.gz -C previous", pages)
        self.assertNotIn("git fetch", pages)
        self.assertIn("persist-credentials: false", pages)

    def test_windows_signing_rejects_unknown_states(self):
        script = (HOSTED / "verify-windows.ps1").read_text()
        self.assertRegex(
            script, r"(?m)^if \(\[string\]\$env:WINDOWS_SIGNING -cnotin 'verified', 'missing', ''\) \{\n"
                    r"    throw 'Unexpected WINDOWS_SIGNING value'\n\}\n\$signed = ",
        )

    @unittest.skipIf(os.name == "nt", "Fake Flatpak requires a POSIX shebang")
    def test_flatpak_smoke_requires_isolated_user_directory(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            flatpak = root / "flatpak"
            flatpak.write_text("#!/bin/sh\nexit 1\n")
            flatpak.chmod(0o755)
            for value in (None, ""):
                with self.subTest(value=value):
                    env = dict(os.environ, PATH=str(root))
                    env.pop("FLATPAK_USER_DIR", None)
                    if value is not None:
                        env["FLATPAK_USER_DIR"] = value
                    result = subprocess.run(
                        [sys.executable, HOSTED / "smoke-stage.py", root, "1.2.3",
                         "--flatpak-ref", "app/example.App/x86_64/stable"],
                        env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=5,
                    )
                    self.assertEqual(result.returncode, 1)
                    self.assertEqual(result.stderr,
                                     "smoke-stage: Set FLATPAK_USER_DIR for --flatpak-ref\n")

    @unittest.skipIf(os.name == "nt", "Fake packaged commands require a POSIX shebang")
    def test_smoke_headless_is_isolated(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            for name in TOOLS:
                binary = root / name
                binary.write_text(f"#!{sys.executable}\n" + FAKE_TOOL)
                binary.chmod(0o755)
            env = dict(os.environ, DISPLAY=":99", WAYLAND_DISPLAY="fake", HTTP_PROXY="http://bad",
                       SMOKE_SECRET="fixture")
            for mode in ("ok", "slow", "status", "shape", "timeout"):
                with self.subTest(mode=mode):
                    (root / "mode").write_text(mode)
                    (root / "calls").write_text("")
                    result = subprocess.run(
                        [sys.executable, HOSTED / "smoke-stage.py", root, "1.2.3"],
                        env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=30,
                    )
                    if mode in {"ok", "slow"}:
                        self.assertEqual(result.returncode, 0, result.stderr)
                    else:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertIn({"status": "503", "shape": "shape", "timeout": "timed out"}[mode],
                                      result.stderr)
                    invocations = [json.loads(line) for line in (root / "calls").read_text().splitlines()]
                    self.assertEqual({(name, flag) for name, (flag, *_) in invocations[:-1]},
                                     {(name, flag) for name in TOOLS for flag in ("--version", "--help")})
                    self.assertEqual(invocations[-1][0], "airscout-planeserver")
                    pid, home = json.loads((root / "daemon").read_text())
                    self.assertFalse(Path(home).exists())
                    with self.assertRaises(ProcessLookupError):
                        os.kill(pid, 0)
