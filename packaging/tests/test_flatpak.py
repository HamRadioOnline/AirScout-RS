import base64
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import unittest

from test_linux import PROGRAMS, ROOT, VERSION, run, yaml


FLATPAK = ROOT / "packaging/flatpak"
WORKFLOW = ROOT / ".github/workflows/release-flatpak.yml"
MANIFEST = FLATPAK / "io.github.hamradioonline.AirScoutRS.yml"
APP_ID = "io.github.hamradioonline.AirScoutRS"
SECRETS = ("FLATPAK_GPG_PRIVATE_KEY",)
PAGES_URL = "https://hamradioonline.github.io/AirScout-RS/"
# Real Flatpak/OSTree refs, relative to the repository's `refs/` directory
# (see controller ruling #1: refs are app/<id>/<arch>/<channel>, branch has no
# slash). They are stored on disk under refs/heads/.
REFS = {
    f"heads/app/{APP_ID}/x86_64/stable",
    f"heads/app/{APP_ID}/aarch64/stable",
    f"heads/app/{APP_ID}/x86_64/beta",
    f"heads/app/{APP_ID}/aarch64/beta",
}
ARCHES = {"x86_64", "aarch64"}
TEST_KEY = "TE1ST flatpak public key"
ZERO_SHA = "0000000000000000000000000000000000000000"


def parse_manifest():
    """Parse the Flatpak manifest into a dict (via Ruby's YAML, since PyYAML is
    not installed). Skips only when Ruby is absent; a YAML syntax error in the
    manifest must fail the test, never silently skip it."""
    if shutil.which("ruby") is None:
        raise unittest.SkipTest("ruby unavailable to parse the manifest")
    ruby = subprocess.run(
        ["ruby", "-ryaml", "-rjson", "-e",
         "print YAML.load_file(ARGV[0]).to_json", str(MANIFEST)],
        capture_output=True, text=True)
    if ruby.returncode != 0:
        raise AssertionError("manifest is not valid YAML: " + ruby.stderr)
    return json.loads(ruby.stdout)


# Cache options declared by option() in the pinned upstream CMake projects, plus
# the ENABLE_INTROSPECTION switch added by the checked-in ayatana-ido patch. An
# undeclared -D name (for example ENABLE_VALA) is silently ignored by CMake.
CMAKE_OPTIONS = {
    "ayatana-ido": {"ENABLE_TESTS", "ENABLE_COVERAGE", "ENABLE_WERROR",
                    "ENABLE_INTROSPECTION"},
    "libayatana-indicator": {"ENABLE_TESTS", "ENABLE_COVERAGE", "ENABLE_WERROR",
                             "FLAVOUR_GTK2", "FLAVOUR_GTK3", "ENABLE_LOADER", "ENABLE_IDO"},
    "libayatana-appindicator": {"ENABLE_TESTS", "ENABLE_COVERAGE", "ENABLE_WERROR",
                                "ENABLE_GTKDOC", "ENABLE_BINDINGS_VALA",
                                "ENABLE_BINDINGS_MONO", "FLAVOUR_GTK2", "FLAVOUR_GTK3"},
}


def flatpak_stage_fixture(stage, target="x86_64-unknown-linux-gnu", slug="linux-x86_64",
                          version=VERSION):
    (stage / "bin").mkdir(parents=True)
    for name in PROGRAMS:
        program = stage / "bin" / name
        program.write_text("#!/bin/sh\nexit 0\n")
        program.chmod(0o755)
    for name in ("README.md", "LICENSE", "NOTICE"):
        (stage / name).write_text(f"{name} fixture\n")
    (stage / "stage.json").write_text(json.dumps({
        "target": target, "slug": slug, "version": version, "glibc_floor": "2.35",
    }))


def flatpak_build_artifacts(candidate):
    for arch in ARCHES:
        transport = candidate / arch
        program = transport / "build/files/bin/airscout"
        program.parent.mkdir(parents=True)
        program.write_text("#!/bin/sh\nexit 0\n")
        program.chmod(0o755)
        with tarfile.open(transport / "build.tar.gz", "w:gz") as archive:
            archive.add(transport / "build", arcname="build")
        shutil.rmtree(transport / "build")


def _write_script(path, *lines):
    path.write_text("\n".join(lines) + "\n")
    path.chmod(0o755)


def flatpak_toolchain(directory):
    """Install fake ``flatpak``/``gpg``/``flatpak-builder`` and return the PATH env.

    The fake ``flatpak`` models the real CLI: ``build-export [--arch=X]
    [--gpg-sign=ID] [--gpg-homedir=DIR] LOCATION DIRECTORY [BRANCH]`` writes
    ``refs/heads/app/<id>/<arch>/<branch>``, rejects a slash in BRANCH, and
    rejects signing without ``--gpg-homedir``. ``build-update-repo`` writes
    ``summary`` (and ``summary.sig`` when signed, ``.pruned`` when --prune).
    """
    bin_dir = Path(directory) / "bin"
    bin_dir.mkdir(parents=True)

    (bin_dir / "flatpak").write_text("""#!/usr/bin/env bash
set -eu
APP_ID="io.github.hamradioonline.AirScoutRS"
state_dir="${FAKE_FLATPAK_STATE:-${TMPDIR:-/tmp}/flatpak-fake-state}"
user=""
cmd=$1; shift
if [ "$cmd" = "--user" ]; then user=1; cmd=$1; shift; fi

arch=""; sign=""; homedir=""; prune=""; no_deploy=""; commit=""; gpg_import=""
positionals=()
for a in "$@"; do
  case "$a" in
    --user) user=1 ;;
    --arch=*) arch=${a#--arch=} ;;
    --gpg-sign=*) sign=${a#--gpg-sign=} ;;
    --gpg-homedir=*) homedir=${a#--gpg-homedir=} ;;
    --prune) prune=1 ;;
    --no-deploy) no_deploy=1 ;;
    --commit=*) commit=${a#--commit=} ;;
    --gpg-import=*) gpg_import=${a#--gpg-import=} ;;
    -y) ;;
    --*) ;;
    *) positionals+=("$a") ;;
  esac
done

enc() { printf '%s' "$1" | tr '/' '_'; }

case "$cmd" in
  build-export)
    repo="${positionals[0]:-}"; builddir="${positionals[1]:-}"; branch="${positionals[2]:-}"
    [ -n "$repo" ] && [ -n "$builddir" ] && [ -n "$branch" ] && [ -n "$arch" ] || { echo "flatpak: build-export usage" >&2; exit 2; }
    if [ -n "$sign" ] && [ -z "$homedir" ]; then echo "flatpak: build-export requires --gpg-homedir when --gpg-sign is used" >&2; exit 2; fi
    case "$branch" in */*) echo "flatpak: branch must not contain a slash" >&2; exit 2 ;; esac
    if [ -n "${FAKE_REQUIRE_EXECUTABLE:-}" ] && [ ! -x "$builddir/files/bin/airscout" ]; then
      echo "flatpak: packaged binary is missing or nonexecutable" >&2; exit 2
    fi
    ref="app/$APP_ID/$arch/$branch"
    mkdir -p "$repo/refs/heads/$(dirname "$ref")"
    : > "$repo/refs/heads/$ref"
    ;;
  build-update-repo)
    repo="${positionals[0]:-}"
    [ -n "$repo" ] || { echo "flatpak: build-update-repo usage" >&2; exit 2; }
    if [ -n "$sign" ] && [ -z "$homedir" ]; then echo "flatpak: build-update-repo requires --gpg-homedir when --gpg-sign is used" >&2; exit 2; fi
    mkdir -p "$repo"
    : > "$repo/summary"
    [ -z "$sign" ] || : > "$repo/summary.sig"
    [ -z "$prune" ] || : > "$repo/.pruned"
    ;;
  remote-add)
    name="${positionals[0]:-}"; url="${positionals[1]:-}"
    [ -n "$gpg_import" ] && [ -s "$gpg_import" ] || { echo "flatpak: remote-add requires --gpg-import=FILE" >&2; exit 2; }
    [ -n "$name" ] && [ -n "$url" ] || { echo "flatpak: remote-add NAME URL" >&2; exit 2; }
    repopath=${url#file://}
    [ -d "$repopath" ] && [ -f "$repopath/summary" ] && [ -f "$repopath/summary.sig" ] || { echo "flatpak: remote-add requires a signed repo (summary + summary.sig)" >&2; exit 2; }
    remotes="${FLATPAK_USER_DIR:-$HOME/.local/share/flatpak}/remotes"
    mkdir -p "$remotes"
    if [ -e "$remotes/$name" ]; then echo "flatpak: Remote $name already exists" >&2; exit 2; fi
    : > "$remotes/$name"
    ;;
  remote-ls)
    name="${positionals[0]:-}"
    remotes="${FLATPAK_USER_DIR:-$HOME/.local/share/flatpak}/remotes"
    [ -e "$remotes/$name" ] || { echo "flatpak: remote $name not found" >&2; exit 2; }
    : ;;
  install)
    ref="${positionals[1]:-}"
    [ -n "$ref" ] || { echo "flatpak: install REMOTE REF" >&2; exit 2; }
    if [ -n "$no_deploy" ]; then
      mkdir -p "$state_dir/undeployed"
      : > "$state_dir/undeployed/$(enc "$ref")"
    else
      mkdir -p "$state_dir/deployed"
      echo "installed" > "$state_dir/deployed/$(enc "$ref")"
    fi
    ;;
  update)
    ref="${positionals[0]:-}"
    [ -n "$ref" ] || { echo "flatpak: update REF" >&2; exit 2; }
    enc_ref=$(enc "$ref")
    if [ ! -e "$state_dir/deployed/$enc_ref" ]; then
      echo "flatpak: $ref not installed (was it installed with --no-deploy?)" >&2
      exit 2
    fi
    [ -n "$commit" ] || { echo "flatpak: update requires --commit=..." >&2; exit 2; }
    echo "$commit" > "$state_dir/deployed/$enc_ref"
    ;;
  info)
    ref="${positionals[0]:-}"
    [ -n "$ref" ] || { echo "flatpak: info REF" >&2; exit 2; }
    [ -n "$user" ] || { echo "flatpak: info requires --user" >&2; exit 2; }
    enc_ref=$(enc "$ref")
    [ -e "$state_dir/deployed/$enc_ref" ] || { echo "flatpak: $ref not installed" >&2; exit 2; }
    cat "$state_dir/deployed/$enc_ref"
    ;;
  *) echo "flatpak: unexpected $cmd" >&2; exit 2 ;;
esac
""")
    (bin_dir / "flatpak").chmod(0o755)

    _write_script(bin_dir / "gpg",
        "#!/usr/bin/env bash",
        "set -eu",
        'args=("$@")',
        'case "${args[*]}" in',
        '  *--import*) cat >/dev/null ;;',
        '  *--list-secret-keys*) echo "fpr:::::::::0123456789ABCDEF0123456789ABCDEF01234567:" ;;',
        '  *--export*) printf "%s" "TE1ST flatpak public key" ;;',
        '  *--verify*) echo "gpg: --verify rejected (OSTree summary.sig is a GVariant)" >&2; exit 2 ;;',
        '  *) echo "gpg: unexpected ${args[*]}" >&2; exit 2 ;;',
        "esac",
    )

    _write_script(bin_dir / "flatpak-builder",
        "#!/usr/bin/env bash",
        "set -eu",
        "args=()",
        'for a in "$@"; do',
        '  case "$a" in --*) ;; *) args+=("$a");; esac',
        "done",
        "build=${args[0]}",
        # flatpak-builder resolves dir/patch source paths next to the manifest.
        'manifest_dir=$(dirname "${args[1]}")',
        "while read -r source; do",
        '  [ -e "$manifest_dir/$source" ] || { echo "missing source $source" >&2; exit 1; }',
        "done < <(sed -n 's/^ *path: *//p' \"${args[1]}\")",
        'mkdir -p "$build"',
        'touch "$build/.built"',
    )

    return dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}")


def publish_toolchain(directory):
    """Fake toolchain for publish-repo.sh: adds ``git`` (ls-remote only) and
    ``du`` (controlled size) to the flatpak/gpg fakes."""
    env = flatpak_toolchain(directory)
    bin_dir = Path(directory) / "bin"

    _write_script(bin_dir / "git",
        "#!/usr/bin/env bash",
        'if [ "$1" = ls-remote ]; then',
        '  [ -z "${FAKE_LIVE_HEAD:-}" ] || printf "%s\\trefs/heads/flatpak-repo\\n" "$FAKE_LIVE_HEAD"',
        "fi",
        'if [ -n "${FAKE_GIT_PUBLISH_LOG:-}" ]; then',
        '  printf "%s\\n" "$*" >> "$FAKE_GIT_PUBLISH_LOG"',
        '  case "$*" in',
        '    "worktree add "*) mkdir "$5" ;;',
        '    *" write-tree") printf "%040d\\n" 1 ;;',
        '    *" commit-tree "*) printf "%040d\\n" 2 ;;',
        '    *" push origin "*) exit "${FAKE_PUSH_STATUS:-0}" ;;',
        '  esac',
        'fi',
        "exit 0",
    )
    _write_script(bin_dir / "du",
        "#!/usr/bin/env bash",
        'printf "%s\\tcandidate\\n" "${FAKE_DU_KB:-1}"',
    )
    return env


def publish_env(directory, key="TE1ST secret key\n", live_head=None, du_kb=None):
    env = publish_toolchain(directory)
    for name in list(env):
        if name.startswith(("FLATPAK_", "GITHUB_")):
            del env[name]
    env["FLATPAK_GPG_PRIVATE_KEY"] = key
    if live_head is not None:
        env["FAKE_LIVE_HEAD"] = live_head
    if du_kb is not None:
        env["FAKE_DU_KB"] = str(du_kb)
    return env


def signed_candidate_fixture(candidate):
    (candidate / "refs").mkdir(parents=True)
    (candidate / "summary").write_text("summary\n")
    (candidate / "summary.sig").write_text("sig\n")
    (candidate / "airscout-rs.flatpakrepo").write_text("[Flatpak Repo]\n")


PUBLISH_RESULT_BEGIN = "# BEGIN publish-result helpers"
PUBLISH_RESULT_END = "# END publish-result helpers"


def workflow_publish_result_snippet():
    """Return the publish step's sidecar-parsing helpers exactly as the release
    workflow runs them (the run script parsed via Ruby's YAML, then the text
    between the BEGIN/END publish-result markers)."""
    if shutil.which("ruby") is None:
        raise unittest.SkipTest("ruby unavailable to parse the workflow")
    ruby = subprocess.run(
        ["ruby", "-ryaml", "-rjson", "-e",
         "print YAML.load_file(ARGV[0]).to_json", str(WORKFLOW)],
        capture_output=True, text=True)
    if ruby.returncode != 0:
        raise AssertionError("workflow is not valid YAML: " + ruby.stderr)
    steps = json.loads(ruby.stdout)["jobs"]["publish"]["steps"]
    script = next(step["run"] for step in steps if step.get("id") == "publish")
    begin = script.index(PUBLISH_RESULT_BEGIN)
    end = script.index(PUBLISH_RESULT_END)
    return script[begin:end]


def run_publish_result(directory, snippet, command, env=None):
    """Run the workflow's publish-result helpers followed by ``command`` in
    ``directory`` and return (completed process, GITHUB_OUTPUT text)."""
    output = Path(directory) / "github_output"
    output.write_text("")
    script_env = dict(env if env is not None else os.environ)
    script_env["GITHUB_OUTPUT"] = str(output)
    result = subprocess.run(
        ["bash", "-c", "set -euo pipefail\n" + snippet + "\n" + command],
        cwd=directory, env=script_env, text=True, capture_output=True)
    return result, output.read_text()


class FlatpakTests(unittest.TestCase):
    def test_workflow_smokes_both_signed_refs_before_publish(self):
        from test_release import workflow_jobs

        jobs = workflow_jobs(WORKFLOW.name)
        self.assertIn("verify-candidate", jobs["publish"]["needs"])
        workflow = WORKFLOW.read_text()
        verify = workflow.split("\n  verify-candidate:\n", 1)[1].split("\n  publish:\n", 1)[0]
        self.assertIn("arch: x86_64", verify)
        self.assertIn("arch: aarch64", verify)
        self.assertIn("verify-flatpak.sh signed-candidate", verify)

    def test_workflow_archives_executable_build_and_prepare_extracts_it(self):
        workflow = WORKFLOW.read_text()
        self.assertIn("tar -czf package/build.tar.gz -C package build", workflow)
        prepare = (FLATPAK / "prepare-repo.sh").read_text()
        self.assertIn("build.tar.gz", prepare)
        self.assertIn("tar -xzf", prepare)

    def test_channel_refs(self):
        # The prepare step must name the OSTree branch and the flatpakref for the
        # channel, and reject alpha (which never publishes a channel descriptor).
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = root / "base-repo"
            (base / "refs").mkdir(parents=True)
            candidate = root / "candidate"
            flatpak_build_artifacts(candidate)
            env = flatpak_toolchain(root)
            env["FLATPAK_GPG_PRIVATE_KEY"] = "TE1ST secret key\n"
            env["FAKE_REQUIRE_EXECUTABLE"] = "1"
            for channel, ref in (("stable", "io.github.hamradioonline.AirScoutRS.flatpakref"),
                                 ("beta", "io.github.hamradioonline.AirScoutRS.beta.flatpakref")):
                with self.subTest(channel=channel):
                    output = root / f"out-{channel}"
                    result = run("bash", FLATPAK / "prepare-repo.sh", base, candidate,
                                 channel, output, env=env, check=False)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertTrue((output / ".pruned").is_file())
                    self.assertEqual(
                        {p.name for p in output.iterdir() if p.suffix in (".flatpakrepo", ".flatpakref")},
                        {"airscout-rs.flatpakrepo", ref})
                    flatpakref = output / ref
                    self.assertIn(f"Branch={channel}", flatpakref.read_text())
                    self.assertIn(f"Url={PAGES_URL}", flatpakref.read_text())
                    flatpakrepo = output / "airscout-rs.flatpakrepo"
                    self.assertIn(f"Url={PAGES_URL}", flatpakrepo.read_text())
                    # GPGKey= must be base64 of the *public* key only, never the
                    # private key.
                    gpgkey = base64.b64decode(
                        re.search(r"(?m)^GPGKey=(.+)$", flatpakrepo.read_text()).group(1)
                    ).decode()
                    self.assertEqual(gpgkey, TEST_KEY)
                    self.assertNotIn("secret", gpgkey.lower())
                    # Real refs were written for both architectures.
                    for arch in sorted(ARCHES):
                        refpath = output / "refs" / "heads" / "app" / APP_ID / arch / channel
                        self.assertTrue(refpath.is_file(), refpath)

            # Alpha publishes no channel: prepare-repo.sh refuses it.
            alpha = root / "out-alpha"
            result = run("bash", FLATPAK / "prepare-repo.sh", base, candidate, "alpha",
                         alpha, env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(alpha.exists())

    def test_merge_keeps_refs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = root / "base-repo"
            (base / "refs").mkdir(parents=True)
            for ref in ("aarch64/stable", "x86_64/beta", "aarch64/beta"):
                path = base / "refs" / "heads" / "app" / APP_ID / ref
                path.parent.mkdir(parents=True, exist_ok=True)
                path.touch()
            before = {p.relative_to(base / "refs").as_posix()
                      for p in (base / "refs").rglob("*") if p.is_file()}

            candidate = root / "candidate"
            flatpak_build_artifacts(candidate)

            env = flatpak_toolchain(root)
            output = root / "merged-repo"
            result = run("bash", FLATPAK / "prepare-repo.sh", base, candidate, "stable",
                         output, env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            merged = {p.relative_to(output / "refs").as_posix()
                      for p in (output / "refs").rglob("*") if p.is_file()}
            self.assertEqual(merged, REFS)

            # A failed export must not disturb the original base repository.
            failing = flatpak_toolchain(root / "fail")
            (Path(root) / "fail/bin/flatpak").write_text(
                '#!/usr/bin/env bash\n'
                'set -eu\n'
                'if [[ "$1" == build-export ]]; then echo "boom" >&2; exit 1; fi\n'
                'exit 0\n'
            )
            (Path(root) / "fail/bin/flatpak").chmod(0o755)
            broken = root / "broken-repo"
            result = run("bash", FLATPAK / "prepare-repo.sh", base, candidate, "stable",
                         broken, env=failing, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(broken.exists())
            after = {p.relative_to(base / "refs").as_posix()
                     for p in (base / "refs").rglob("*") if p.is_file()}
            self.assertEqual(after, before)

    def test_manifest(self):
        manifest = parse_manifest()
        self.assertEqual(manifest["app-id"], APP_ID)
        self.assertEqual(manifest["runtime"], "org.freedesktop.Platform")
        self.assertEqual(manifest["runtime-version"], "25.08")
        self.assertEqual(manifest["sdk"], "org.freedesktop.Sdk")
        self.assertEqual(manifest["command"], "airscout")
        finish_args = manifest["finish-args"]
        for arg in ("--socket=wayland", "--socket=fallback-x11", "--socket=pulseaudio",
                    "--share=ipc", "--share=network", "--device=dri"):
            self.assertIn(arg, finish_args)
        # Serial devices are a documented opt-in, never granted by default, so
        # --device=all must not appear in the parsed finish-args (a raw-text
        # check would pass on the explanatory comment).
        self.assertNotIn("--device=all", finish_args)

        modules = {mod["name"] for mod in manifest["modules"]}
        self.assertIn("airscout-rs", modules)
        for module in ("ayatana-ido", "libayatana-indicator", "libayatana-appindicator",
                       "dbusmenu", "xdotool"):
            self.assertIn(module, modules)

        airscout = next(mod for mod in manifest["modules"] if mod["name"] == "airscout-rs")
        install_cmd = next(c for c in airscout["build-commands"] if c.startswith("install -m 0755"))
        for program in sorted(PROGRAMS):
            self.assertIn(program, install_cmd)
        # The corresponding-source notice is installed alongside the licenses.
        self.assertTrue(any("third-party-notices.txt" in cmd
                            for cmd in airscout["build-commands"]))

    def test_pinned_module_sources(self):
        manifest = parse_manifest()
        checksums = [s["sha256"] for mod in manifest["modules"]
                     for s in mod.get("sources", []) if "sha256" in s]
        self.assertGreaterEqual(len(checksums), 5)
        self.assertTrue(all(value != "0" * 64 for value in checksums))

    def test_manifest_license_globs(self):
        # The bundled libraries ship suffixed license files (COPYING.2.1,
        # COPYING.LGPL.2.1, COPYING-GPL3, ...) that the plain COPYING/COPYING.LGPL
        # loop misses. The post-install must match COPYING*/LICENSE*/COPYRIGHT*
        # variants and fail (no `; true`) when none matched.
        manifest = parse_manifest()
        for mod in manifest["modules"]:
            # Build-only tools remove everything they install and ship nothing.
            if mod["name"] == "airscout-rs" or mod.get("cleanup") == ["*"]:
                continue
            post = "\n".join(mod.get("post-install", []))
            # Prefix globs match both dot-suffixed (COPYING.LGPL.2.1) and
            # hyphen-suffixed (COPYING-GPL3) license files; a dotted glob
            # (COPYING.*) would miss COPYING-GPL3.
            self.assertIn("COPYING*", post, mod["name"])
            self.assertIn("LICENSE*", post, mod["name"])
            self.assertIn("COPYRIGHT*", post, mod["name"])
            self.assertNotIn("; true", post, mod["name"])
            self.assertIn("found", post, mod["name"])

    def test_tray_modules_build_without_vala_or_introspection(self):
        # The Freedesktop 25.08 SDK has no valac. AirScout-RS links only the C
        # libraries, so no tray module may require Vala or build bindings.
        modules = {mod["name"]: mod for mod in parse_manifest()["modules"]}
        for name, declared in CMAKE_OPTIONS.items():
            with self.subTest(module=name):
                self.assertEqual(modules[name]["buildsystem"], "cmake-ninja")
                for opt in modules[name].get("config-opts", []):
                    key = re.fullmatch(r"-D([A-Za-z0-9_]+)(?::[A-Z]+)?=.*", opt)
                    self.assertIsNotNone(key, opt)
                    if not key[1].startswith("CMAKE_"):
                        self.assertIn(key[1], declared, f"{name} does not declare {opt}")

        ido = modules["ayatana-ido"]
        self.assertIn("-DENABLE_INTROSPECTION=OFF", ido["config-opts"])
        self.assertIn("-DENABLE_TESTS=OFF", ido["config-opts"])
        patches = [s["path"] for s in ido["sources"] if s["type"] == "patch"]
        self.assertEqual(patches, ["patches/ayatana-ido-0.9.3-optional-introspection.patch"])
        patch = (FLATPAK / patches[0]).read_text()
        # The modified source is part of the corresponding-source notice.
        self.assertIn(patches[0].split("/")[-1], (FLATPAK / "third-party-notices.txt").read_text())
        self.assertIn('+option(ENABLE_INTROSPECTION "', patch)
        self.assertRegex(patch, r"\+if \(ENABLE_INTROSPECTION\)\n\+\n find_package\(GObjectIntrospection")

        appindicator = modules["libayatana-appindicator"]["config-opts"]
        self.assertIn("-DENABLE_BINDINGS_VALA=OFF", appindicator)
        self.assertIn("-DENABLE_BINDINGS_MONO=OFF", appindicator)

        dbusmenu = modules["dbusmenu"]
        self.assertIn("--disable-vala", dbusmenu["config-opts"])
        self.assertIn("--enable-introspection=no", dbusmenu["config-opts"])
        self.assertIn("-Wno-error", dbusmenu["build-options"]["cflags"].split())

    def test_cmake_modules_install_into_the_flatpak_libdir(self):
        # GNUInstallDirs picks lib64 on the SDK, but pkg-config and the loader
        # only search /app/lib, so later modules could not find ayatana-ido.
        for mod in parse_manifest()["modules"]:
            if mod.get("buildsystem") == "cmake-ninja":
                with self.subTest(module=mod["name"]):
                    self.assertIn("-DCMAKE_INSTALL_LIBDIR=lib", mod.get("config-opts", []))

    def test_intltool_is_a_pinned_build_only_module(self):
        manifest = parse_manifest()
        names = [mod["name"] for mod in manifest["modules"]]
        self.assertLess(names.index("intltool"), names.index("dbusmenu"))
        intltool = manifest["modules"][names.index("intltool")]
        self.assertEqual(intltool["cleanup"], ["*"])
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        self.assertIn("intltool", lock["flatpak-modules"])
        patches = [s["path"] for s in intltool["sources"] if s["type"] == "patch"]
        self.assertEqual(patches, ["patches/intltool-0.51.0-perl5.26-regex-fixes.patch"])
        self.assertTrue((FLATPAK / patches[0]).is_file())
        # Nothing from intltool ships, so it is not a bundled component.
        self.assertNotIn("intltool", (FLATPAK / "third-party-notices.txt").read_text().lower())

    def test_lockfile_manifest_parity(self):
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        manifest = parse_manifest()
        rt = lock["flatpak-runtime"]["all"]
        self.assertEqual(f"{manifest['runtime']}//{manifest['runtime-version']}", rt["runtime"])
        self.assertEqual(f"{manifest['sdk']}//{manifest['runtime-version']}", rt["sdk"])
        modules = lock["flatpak-modules"]
        manifest_modules = {mod["name"]: mod for mod in manifest["modules"]
                            if mod["name"] in modules}
        self.assertEqual(set(manifest_modules), set(modules))
        for name, pin in modules.items():
            archives = [s for s in manifest_modules[name].get("sources", [])
                        if s.get("type") == "archive"]
            self.assertEqual(len(archives), 1, name)
            self.assertEqual(archives[0]["url"], pin["url"], name)
            self.assertEqual(archives[0]["sha256"], pin["sha256"], name)

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

    def test_workflow_publish_gate(self):
        workflow = WORKFLOW.read_text()
        # The release gate is an explicit boolean input, defaulting to closed.
        self.assertRegex(workflow, r"(?ms)^      publish:\n"
                                   r"        required: false\n"
                                   r"        type: boolean\n"
                                   r"        default: false$")
        # Alpha tags only smoke-test; prepare/publish are gated off them, and
        # publish also requires the release gate.
        self.assertIn("if: ${{ !startsWith(inputs.tag, 'alpha-') }}", workflow)
        self.assertIn("!cancelled() && inputs.publish && needs.locate-base.result == 'success'", workflow)
        self.assertIn("needs.verify-candidate.result == 'success'", workflow)
        # Strict tag grammar: alpha/beta/v, anything else fails the job.
        self.assertIn('alpha-v*) echo "channel=alpha"', workflow)
        self.assertIn("::error::Unrecognized tag grammar", workflow)
        # Serialized flatpak-repo updates.
        self.assertIn("concurrency:", workflow)
        caller = (WORKFLOW.parent / "release.yml").read_text()
        self.assertIn("group: flatpak-release-publication", caller)
        self.assertIn("cancel-in-progress: false", workflow)

    def test_secret_boundary(self):
        workflow = WORKFLOW.read_text()
        if yaml:
            jobs = yaml.load(workflow, Loader=yaml.BaseLoader)["jobs"]
            self.assertNotIn("environment", jobs["build"])
            self.assertNotIn("secrets.FLATPAK_GPG_PRIVATE_KEY", str(jobs["build"]))
            for job in ("prepare", "publish"):
                self.assertEqual(jobs[job]["environment"], "release")
                self.assertIn("secrets.FLATPAK_GPG_PRIVATE_KEY", str(jobs[job]))
            rows = jobs["build"]["strategy"]["matrix"]["include"]
        else:
            jobs = dict(re.findall(r"(?ms)^  ([\w-]+):\n(.*?)(?=^  [\w-]+:|\Z)",
                                   workflow.split("\njobs:\n", 1)[1]))
            self.assertNotIn("\n    environment:", jobs["build"])
            self.assertNotIn("secrets.FLATPAK_GPG_PRIVATE_KEY", jobs["build"])
            for job in ("prepare", "publish"):
                self.assertIn("\n    environment: release\n", jobs[job])
                self.assertIn("secrets.FLATPAK_GPG_PRIVATE_KEY", jobs[job])
            rows = [{"arch": arch, "os": os_} for arch, os_ in re.findall(
                r"^          - slug: linux-[\w]+\n            arch: (x86_64|aarch64)\n"
                r"            os: ([\w.-]+)$", jobs["build"], re.M)]
        self.assertEqual({(row["arch"], row["os"]) for row in rows},
                         {("x86_64", "ubuntu-24.04"), ("aarch64", "ubuntu-24.04-arm")})

    def test_flatpak_jobs_use_noble_tooling(self):
        # flatpak-builder 1.2.x (jammy) still calls appstream-compose, which the
        # 25.08 SDK no longer ships; 1.4.x uses appstreamcli compose.
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        for arch in ("x86_64", "aarch64"):
            with self.subTest(arch=arch):
                tool = lock["flatpak-tooling"][arch]
                self.assertTrue(tool["os"].startswith("ubuntu-24.04"), tool["os"])
                self.assertTrue(tool["flatpak-builder"]["version"].startswith("1.4."))
                self.assertIn("/dists/noble", tool["flatpak-builder"]["source"])
        workflow = (ROOT / ".github/workflows/release-flatpak.yml").read_text()
        self.assertNotIn("ubuntu-22.04", workflow)

    def test_secret_sets(self):
        env = {k: v for k, v in os.environ.items() if not k.startswith("FLATPAK_")}
        self.assertEqual(run("bash", FLATPAK / "prepare-repo.sh", "check", env=env,
                             check=False).stdout.strip(), "missing")
        env["FLATPAK_GPG_PRIVATE_KEY"] = "TE1ST\n-----END PGP PRIVATE KEY BLOCK-----"
        result = run("bash", FLATPAK / "prepare-repo.sh", "check", env=env, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "signed")
        for value in ("TE1ST", "-----END PGP PRIVATE KEY BLOCK-----"):
            self.assertNotIn(value, result.stdout + result.stderr)

    def test_unsigned_prepare_omits_descriptors(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = root / "base-repo"
            (base / "refs").mkdir(parents=True)
            candidate = root / "candidate"
            flatpak_build_artifacts(candidate)
            env = {k: v for k, v in flatpak_toolchain(root).items()
                   if not k.startswith("FLATPAK_")}
            output = root / "out"
            result = run("bash", FLATPAK / "prepare-repo.sh", base, candidate, "stable",
                         output, env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            names = {p.name for p in output.iterdir()}
            self.assertIn(".nojekyll", names)
            self.assertNotIn("airscout-rs.flatpakrepo", names)
            self.assertFalse(any(p.suffix in (".flatpakrepo", ".flatpakref")
                                 for p in output.iterdir()))
            sidecar = json.loads(Path(str(output) + ".result.json").read_text())
            self.assertEqual(sidecar["signing"], "missing")
            self.assertEqual(sidecar["assets"], [])

    def test_fake_flatpak_rejects_slash_branch_and_missing_homedir(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = flatpak_toolchain(root)
            flatpak = root / "bin" / "flatpak"
            slash = run("bash", flatpak, "build-export", "--arch=x86_64", "repo",
                        "build", "stable/x86_64", env=env, check=False)
            self.assertNotEqual(slash.returncode, 0)
            no_homedir = run("bash", flatpak, "build-export", "--arch=x86_64",
                             "--gpg-sign=ABC", "repo", "build", "stable", env=env,
                             check=False)
            self.assertNotEqual(no_homedir.returncode, 0)

    def test_publish_missing_repo_layout(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            candidate.mkdir()
            env = publish_env(root)
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, ZERO_SHA,
                         env=env, check=False)
            self.assertNotEqual(result.returncode, 0)

    def test_publish_unsigned_skip(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            (candidate / "refs").mkdir(parents=True)
            (candidate / "summary").write_text("summary\n")
            env = publish_env(root)
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, ZERO_SHA,
                         env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("unsigned", result.stderr)
            # Reports published=false (never pushes/commits) via the sidecar so
            # the workflow can gate Pages upload/deploy on actual publication.
            sidecar = json.loads(Path(str(candidate) + ".result.json").read_text())
            self.assertEqual(sidecar["published"], False)
            self.assertNotIn("base_sha", sidecar)

    def test_publish_stale_base(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            signed_candidate_fixture(candidate)
            env = publish_env(root, live_head="a" * 40)
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, "b" * 40,
                         env=env, check=False)
            # A distinct exit code (42) lets the workflow re-prepare and retry.
            self.assertEqual(result.returncode, 42)
            self.assertIn("Stale base", result.stderr)

    def test_branch_commit_is_orphan_with_explicit_lease(self):
        source = (FLATPAK / "publish-repo.sh").read_text()
        self.assertIn('commit-tree -S "$tree"', source)
        self.assertNotRegex(source, r"commit-tree [^\n]* -p ")
        self.assertIn('--force-with-lease="refs/heads/$FLATPAK_BRANCH:$live_head"', source)

    def test_lease_requires_recorded_base_and_failed_push_leaves_no_sidecar(self):
        for base in (ZERO_SHA, "c" * 40):
            with self.subTest(base=base), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                candidate = root / "candidate"
                signed_candidate_fixture(candidate)
                live = "" if base == ZERO_SHA else base
                env = publish_env(root, live_head=live)
                log = root / "git.log"
                env.update(GITHUB_REPOSITORY="example/project", FAKE_GIT_PUBLISH_LOG=str(log),
                           FAKE_PUSH_STATUS="1")
                result = run("bash", FLATPAK / "publish-repo.sh", candidate, base,
                             env=env, check=False)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                push = next(line for line in log.read_text().splitlines() if " push origin " in line)
                self.assertTrue(push.endswith(
                    f"--force-with-lease=refs/heads/flatpak-repo:{live} "
                    f"{2:040d}:refs/heads/flatpak-repo"), push)
                self.assertFalse(candidate.with_suffix(".result.json").exists())

    def test_prepare_prunes_to_current_and_previous(self):
        source = (FLATPAK / "prepare-repo.sh").read_text()
        self.assertIn("--prune --prune-depth=1", source)

    def test_publish_preserves_verified_summary_and_writes_sidecar(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            signed_candidate_fixture(candidate)
            env = publish_env(root, live_head="c" * 40)
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, "c" * 40,
                         env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            # The deployed tree must still match the pre-push smoke tree.
            self.assertFalse((candidate / ".pruned").exists())
            self.assertEqual((candidate / "summary.sig").read_text(), "sig\n")
            self.assertFalse((candidate / "result.json").exists())
            sidecar = json.loads(Path(str(candidate) + ".result.json").read_text())
            self.assertIs(sidecar["published"], True)
            self.assertEqual(sidecar["signing"], "verified")
            self.assertEqual(sidecar["base_sha"], "c" * 40)

    def test_publish_size_preflight(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            signed_candidate_fixture(candidate)
            env = publish_env(root, live_head="d" * 40, du_kb=1024 * 1024 + 1)
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, "d" * 40,
                         env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Pages limit", result.stderr)

    def test_publish_first_publish(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            signed_candidate_fixture(candidate)
            env = publish_env(root, live_head="")
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, ZERO_SHA,
                         env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_publish_result_json_after_push(self):
        # The sidecar must be written only after the branch commit/push block, and
        # must record the *pushed* head (published_head), never the pre-push live
        # head, so a failed push cannot produce a stale success sidecar.
        src = (FLATPAK / "publish-repo.sh").read_text()
        push = src.index("push origin")
        sidecar = src.index('python3 - "$candidate" "$published_head"')
        self.assertGreater(sidecar, push)
        self.assertLess(src.index('commit-tree -S "$tree"'), push)
        self.assertIn('"$published_head:refs/heads/$FLATPAK_BRANCH"', src)

    def test_publish_worktree_cleanup(self):
        # A registered git worktree is removed via a trap so a failure mid-publish
        # never leaves a stale .git/worktrees/ entry.
        src = (FLATPAK / "publish-repo.sh").read_text()
        self.assertIn("worktree_cleanup()", src)
        self.assertIn("trap 'worktree_cleanup' EXIT", src)
        self.assertIn("git worktree remove --force -- \"$worktree_dir\"", src)
        self.assertLess(src.index('worktree_dir="$work/tree"'),
                        src.index('commit-tree -S "$tree"'))

    def test_publish_requires_signed_summary(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            signed_candidate_fixture(candidate)
            (candidate / "summary.sig").unlink()
            env = publish_env(root, live_head="e" * 40)
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, "e" * 40,
                         env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("summary.sig", result.stderr)

    def test_fake_gpg_rejects_verify(self):
        # OSTree's summary.sig is a GLib GVariant, not a raw OpenPGP signature:
        # the fake gpg rejects `--verify` so publish-repo.sh cannot fall back to
        # the broken `gpg --verify summary.sig summary` call.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = flatpak_toolchain(root)
            result = run("bash", root / "bin" / "gpg", "--verify", "summary.sig",
                         "summary", env=env, check=False)
            self.assertNotEqual(result.returncode, 0)

    def test_fake_flatpak_remote_verify(self):
        # flatpak remote-add --gpg-import verifies the prepared repo carries a
        # signed summary (summary + summary.sig) and a non-empty public key.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = flatpak_toolchain(root)
            flatpak = root / "bin" / "flatpak"
            repo = root / "repo"
            repo.mkdir()
            (repo / "summary").write_text("summary\n")
            (repo / "summary.sig").write_text("sig\n")
            key = root / "pub.gpg"
            key.write_text("TE1ST flatpak public key\n")
            def remote_add(user_dir):
                e = dict(env)
                e["FLATPAK_USER_DIR"] = user_dir
                return run("bash", flatpak, "remote-add", "--user",
                           f"--gpg-import={key}", "verify", f"file://{repo}",
                           env=e, check=False)
            ok = remote_add(str(root / "ud1"))
            self.assertEqual(ok.returncode, 0, ok.stderr)
            (repo / "summary.sig").unlink()
            bad = remote_add(str(root / "ud2"))
            self.assertNotEqual(bad.returncode, 0)

    def test_redaction_on_failure(self):
        key = ("-----BEGIN PGP PRIVATE KEY BLOCK-----\n"
               "first-secret-line-9f\n"
               "second-secret-line-7d\n"
               "-----END PGP PRIVATE KEY BLOCK-----\n")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = root / "base"
            (base / "refs").mkdir(parents=True)
            candidate = root / "candidate"
            flatpak_build_artifacts(candidate)
            # A gpg that dumps the private key into its log, then fails, so the
            # redacting run_checked wrapper must scrub it.
            bin_dir = root / "bin"
            bin_dir.mkdir()
            gpg = bin_dir / "gpg"
            gpg.write_text(
                '#!/usr/bin/env bash\n'
                'printf "%s\\n" "$FLATPAK_GPG_PRIVATE_KEY"\n'
                'echo "key=$FLATPAK_GPG_PRIVATE_KEY"\n'
                'exit 1\n'
            )
            gpg.chmod(0o755)
            (bin_dir / "flatpak").write_text("#!/usr/bin/env bash\nexit 0\n")
            (bin_dir / "flatpak").chmod(0o755)

            env = {k: v for k, v in os.environ.items() if not k.startswith("FLATPAK_")}
            env["FLATPAK_GPG_PRIVATE_KEY"] = key
            env["PATH"] = f"{bin_dir}:{os.environ['PATH']}"

            output = root / "out"
            result = run("bash", FLATPAK / "prepare-repo.sh", base,
                         candidate, "stable", output, env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("[redacted]", result.stderr)
            for line in key.splitlines():
                if line:
                    self.assertNotIn(line, result.stderr)

    def test_pinned_runtime_refs(self):
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        pin = lock["flatpak-runtime"]["all"]
        self.assertEqual(pin["version"], "25.08")
        self.assertEqual(pin["runtime"], "org.freedesktop.Platform//25.08")
        self.assertEqual(pin["sdk"], "org.freedesktop.Sdk//25.08")
        self.assertEqual(set(pin["arches"]), ARCHES)
        commits = pin["commits"]
        self.assertEqual(set(commits), ARCHES)
        for arch in ARCHES:
            self.assertEqual(set(commits[arch]), {"platform", "sdk"})
            self.assertRegex(commits[arch]["platform"], r"^[0-9a-f]{64}$")
            self.assertRegex(commits[arch]["sdk"], r"^[0-9a-f]{64}$")
        self.assertRegex(lock["flathub-flatpakrepo"]["all"]["sha256"], r"^[0-9a-f]{64}$")
        tooling = lock["flatpak-tooling"]
        self.assertEqual(set(tooling), ARCHES)
        for arch in ARCHES:
            self.assertIn("flatpak", tooling[arch])
            self.assertIn("flatpak-builder", tooling[arch])

    def test_lockfile_pins_have_source(self):
        # Every immutable pin (a sha256 checksum or a container digest) must cite
        # the upstream URL it was taken from, so it is verifiable and not invented.
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())

        def require_source(node, path):
            if isinstance(node, dict):
                if "sha256" in node or "digest" in node:
                    self.assertIn("source", node, path)
                for key, value in node.items():
                    require_source(value, f"{path}.{key}")
            elif isinstance(node, list):
                for index, value in enumerate(node):
                    require_source(value, f"{path}[{index}]")

        require_source(lock, "tools.lock.json")
        # The runtime/SDK commit hashes are bare strings nested under commits/;
        # their upstream source lives at the enclosing all/ level.
        self.assertIn("source", lock["flatpak-runtime"]["all"])

    def test_workflow_flatpak_tooling_pinned(self):
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        workflow = WORKFLOW.read_text()
        # The workflow reads its tooling/runtime pins from tools.lock.json and
        # verifies the .deb checksums before installing, instead of an unpinned
        # `apt-get install flatpak`.
        self.assertIn('json.load(open("packaging/tools.lock.json"', workflow)
        self.assertIn("sha256sum -c -", workflow)
        self.assertIn("./flatpak.deb", workflow)
        self.assertIn("./flatpak-builder.deb", workflow)
        self.assertIn('lock["flatpak-tooling"]', workflow)
        for arch in ARCHES:
            for tool in ("flatpak", "flatpak-builder"):
                pin = lock["flatpak-tooling"][arch][tool]
                self.assertRegex(pin["sha256"], r"^[0-9a-f]{64}$")
                self.assertNotEqual(pin["sha256"], "0" * 64)
                self.assertIn("url", pin)
                self.assertIn("source", pin)

    def test_workflow_pages_deploy(self):
        workflow = WORKFLOW.read_text()
        # Actions Pages deployment: pages:write + id-token:write permissions and
        # upload-pages-artifact + deploy-pages, both pinned by full commit SHA.
        self.assertIn("pages: write", workflow)
        self.assertIn("id-token: write", workflow)
        for action in ("upload-pages-artifact", "deploy-pages"):
            self.assertRegex(workflow, rf"uses: actions/{action}@[0-9a-f]{{40}}")

    def test_workflow_stale_base_reprepare(self):
        workflow = WORKFLOW.read_text()
        self.assertIn("inputs.publish && needs.locate-base.outputs.base != inputs.base", workflow)
        self.assertIn("needs.prepare.result == 'success'", workflow)
        self.assertIn("needs.prepare.result == 'skipped' && needs.locate-base.outputs.base == inputs.base",
                      workflow)
        self.assertEqual(workflow.count("artifact-ids: ${{ needs.prepare.outputs.candidate || inputs.candidate }}"), 2)
        self.assertEqual(workflow.count("merge-multiple: true"), 2)

    def test_workflow_runtime_install_pin(self):
        workflow = WORKFLOW.read_text()
        # The runtime/SDK are installed normally (deployed) and then pinned by
        # updating to the exact commit. `install --no-deploy` leaves the ref
        # undeployed, so a following `update --commit` fails with "not installed";
        # `info` must pass --user so it inspects the per-user install.
        self.assertNotIn("flatpak --user install -y --no-deploy", workflow)
        self.assertIn("flatpak --user install -y flathub", workflow)
        self.assertIn("flatpak --user update -y --commit=", workflow)
        self.assertIn("flatpak --user info --show-commit", workflow)

    def test_fake_flatpak_install_update_info(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = root / "state"
            env = flatpak_toolchain(root)
            env["FAKE_FLATPAK_STATE"] = str(state)
            flatpak = root / "bin" / "flatpak"
            ref = "org.freedesktop.Platform/x86_64/25.08"
            # `install --no-deploy` leaves the ref undeployed: a later `update`
            # fails with "not installed".
            ok = run("bash", flatpak, "install", "--no-deploy", "-y", "flathub", ref,
                     env=env, check=False)
            self.assertEqual(ok.returncode, 0, ok.stderr)
            bad_update = run("bash", flatpak, "update", "-y", "--commit=deadbeef", ref,
                             env=env, check=False)
            self.assertNotEqual(bad_update.returncode, 0)
            # A normal install deploys the ref, so `update --commit` succeeds and
            # `info --show-commit` reports the deployed commit.
            ok2 = run("bash", flatpak, "install", "-y", "flathub", ref, env=env,
                      check=False)
            self.assertEqual(ok2.returncode, 0, ok2.stderr)
            upd = run("bash", flatpak, "update", "-y", "--commit=abcdef", ref,
                      env=env, check=False)
            self.assertEqual(upd.returncode, 0, upd.stderr)
            # `info` must pass --user.
            no_user = run("bash", flatpak, "info", "--show-commit", ref, env=env,
                          check=False)
            self.assertNotEqual(no_user.returncode, 0)
            info = run("bash", flatpak, "--user", "info", "--show-commit", ref,
                       env=env, check=False)
            self.assertEqual(info.returncode, 0, info.stderr)
            self.assertEqual(info.stdout.strip(), "abcdef")

    def test_workflow_unsigned_no_deploy(self):
        workflow = WORKFLOW.read_text()
        # An unsigned candidate (no FLATPAK_GPG_PRIVATE_KEY) must never upload or
        # deploy: Pages upload and deploy are gated on the publish job's
        # `published` output, which publish-repo.sh sets false for unsigned.
        self.assertIn("published: ${{ steps.publish.outputs.published }}", workflow)
        self.assertIn("if: ${{ steps.publish.outputs.published == 'true' }}", workflow)
        self.assertIn("if: ${{ needs.publish.outputs.published == 'true' }}", workflow)


    def test_workflow_publish_result_signed(self):
        # Run the workflow's real sidecar parser against the sidecar that
        # publish-repo.sh writes for a signed publish: GITHUB_OUTPUT must say
        # published=true so the Pages upload and deploy run.
        snippet = workflow_publish_result_snippet()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            signed_candidate_fixture(candidate)
            env = publish_env(root, live_head="c" * 40)
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, "c" * 40,
                         env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            parsed, output = run_publish_result(
                root, snippet, "record_publish_result candidate.result.json")
            self.assertEqual(parsed.returncode, 0, parsed.stderr)
            self.assertEqual(output, "published=true\n")

    def test_workflow_publish_result_unsigned(self):
        # The unsigned sidecar from publish-repo.sh must yield published=false
        # (a clean exit, no upload or deploy).
        snippet = workflow_publish_result_snippet()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            candidate = root / "candidate"
            (candidate / "refs").mkdir(parents=True)
            (candidate / "summary").write_text("summary\n")
            env = publish_env(root)
            result = run("bash", FLATPAK / "publish-repo.sh", candidate, ZERO_SHA,
                         env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            parsed, output = run_publish_result(
                root, snippet, "record_publish_result candidate.result.json")
            self.assertEqual(parsed.returncode, 0, parsed.stderr)
            self.assertEqual(output, "published=false\n")

    def test_workflow_publish_result_rejects_unknown(self):
        # A missing sidecar, a prepare-only sidecar (no published key) or a
        # non-boolean value fails the step, and still records published=false.
        snippet = workflow_publish_result_snippet()
        cases = {
            "missing": None,
            "prepare-only": {"format": "flatpak", "signing": "verified"},
            "string": {"published": "true"},
        }
        for name, sidecar in cases.items():
            with self.subTest(case=name), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                if sidecar is not None:
                    (root / "candidate.result.json").write_text(json.dumps(sidecar))
                parsed, output = run_publish_result(
                    root, snippet, "record_publish_result candidate.result.json")
                self.assertNotEqual(parsed.returncode, 0)
                self.assertEqual(output, "published=false\n")

    def test_workflow_publish_result_failure_path(self):
        # Any early exit (failed publish, failed re-prepare) still writes
        # published=false to GITHUB_OUTPUT exactly once.
        snippet = workflow_publish_result_snippet()
        with tempfile.TemporaryDirectory() as tmp:
            parsed, output = run_publish_result(Path(tmp), snippet, "exit 3")
            self.assertEqual(parsed.returncode, 3)
            self.assertEqual(output, "published=false\n")

    def test_workflow_publish_logs_redacted(self):
        # publish.out/publish.err are printed through the redactor on every
        # attempt (success included), inside the attempt's log group.
        snippet = workflow_publish_result_snippet()
        secret = "TE1ST secret key line"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "publish.out").write_text(f"out {secret}\n")
            (root / "publish.err").write_text(f"err {secret}\n")
            env = dict(os.environ)
            env["FLATPAK_GPG_PRIVATE_KEY"] = f"-----BEGIN-----\n{secret}\n-----END-----\n"
            parsed, _ = run_publish_result(root, snippet, "show_publish_logs", env=env)
            self.assertEqual(parsed.returncode, 0, parsed.stderr)
            self.assertNotIn(secret, parsed.stderr + parsed.stdout)
            self.assertIn("out [redacted]", parsed.stderr)
            self.assertIn("err [redacted]", parsed.stderr)
        workflow = WORKFLOW.read_text()
        step = workflow.split("# END publish-result helpers", 1)[1]
        self.assertRegex(step, r"else\n +rc=\$\?\n +show_publish_logs\n +exit")
        self.assertNotIn("cat publish.", step)

    def test_workflow_include_hidden_files(self):
        workflow = WORKFLOW.read_text()
        # actions/upload-artifact drops hidden files (like .nojekyll) by default;
        # the repo tree uploads must opt in so .nojekyll survives to the publish
        # job and the deployed Pages tree.
        self.assertIn("include-hidden-files: true", workflow)

    def test_workflow_deploy_concurrency(self):
        workflow = WORKFLOW.read_text()
        # The deploy job is serialized with its own concurrency group so
        # overlapping releases cannot deploy to Pages out of order.
        self.assertIn("flatpak-repo-pages-deploy", workflow)

    def test_publish_isolated_flatpak_user_dir(self):
        # The signature-check remote-add/remote-ls must run against an isolated
        # FLATPAK_USER_DIR so two consecutive publishes in the same HOME both
        # succeed (no "Remote airscout-verify already exists" collision).
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            home = root / "home"
            home.mkdir()
            candidate = root / "candidate"
            signed_candidate_fixture(candidate)
            env = publish_env(root, live_head="c" * 40)
            env["HOME"] = str(home)
            for _ in range(2):
                result = run("bash", FLATPAK / "publish-repo.sh", candidate, "c" * 40,
                             env=env, check=False)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_stage_rejects_non_flatpak_targets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage = root / "stage"
            flatpak_stage_fixture(stage, target="armv7-unknown-linux-gnueabihf",
                                  slug="linux-armhf")
            env = flatpak_toolchain(root)
            output = root / "out"
            result = run("bash", FLATPAK / "stage.sh", stage, "stable", output,
                         env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())

    def test_stage_builds_one_arch(self):
        version = re.search(r'(?m)^\[workspace\.package\]\nversion = "([^"]+)"',
                            (ROOT / "Cargo.toml").read_text())[1]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage = root / "stage"
            flatpak_stage_fixture(stage, target="aarch64-unknown-linux-gnu",
                                  slug="linux-arm64", version=version)
            env = flatpak_toolchain(root)
            output = root / "out"
            result = run("bash", FLATPAK / "stage.sh", stage, "beta", output, env=env,
                         check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((output / "build").is_dir())
            sidecar = json.loads((output / "result.json").read_text())
            self.assertEqual(sidecar, {
                "format": "flatpak", "slug": "linux-arm64", "branch": "beta",
                "assets": [], "signing": "not_applicable",
            })


if __name__ == "__main__":
    unittest.main()
