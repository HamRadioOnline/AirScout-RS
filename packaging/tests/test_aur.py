import hashlib
import importlib.util
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
from test_linux import APP_ID, PROGRAMS

AUR = ROOT / "packaging/aur"
RENDER = AUR / "render.py"
PUBLISH = AUR / "publish.sh"
LINUX = ROOT / "packaging/linux"
REPOSITORY = "https://github.com/HamRadioOnline/AirScout-RS"
MAINTAINER = "HamRadioOnline"
PACKAGES = ("airscout-rs-bin", "airscout-rs", "airscout-rs-git")
# AUR architecture name -> release slug (asset basename uses the slug).
ARCH_SLUGS = {
    "x86_64": "linux-x86_64",
    "aarch64": "linux-arm64",
    "armv7h": "linux-armhf",
    "i686": "linux-i686",
}
# The AUR host keys are published on the AUR front page; pin the ed25519 one.
AUR_ED25519 = "SHA256:RFzBCUItH9LZS0cKB5UE6ceAYhBD5C8GeOBip8Z11+4"
ARCH_BUILDER_DIGEST = "sha256:a1d4b509c51fc68dcc7ee1f88342fe26b1f65e6775c3aff26f196141e6c76a6f"
DEPENDS = [
    "glibc", "libgcc", "gtk3", "alsa-lib", "libayatana-appindicator", "xdotool",
    "libx11", "libxkbcommon", "libxkbcommon-x11", "wayland", "libglvnd", "mesa",
    "vulkan-icd-loader", "vulkan-swrast", "adwaita-icon-theme", "hicolor-icon-theme",
    "systemd",
]
MAKEDEPENDS = ["cargo", "gcc", "pkgconf", "gtk3", "alsa-lib", "libayatana-appindicator", "xdotool"]


def pkgbuild_fields(path):
    """Source a PKGBUILD and return its metadata as a key -> list-of-values dict."""
    script = (
        'source "$0"\n'
        'emit() { local name=$1; shift; local v; for v in "$@"; do '
        'printf "%s\\t%s\\n" "$name" "$v"; done; }\n'
        'emit pkgname "$pkgname"\n'
        'emit pkgver "$pkgver"\n'
        'emit pkgrel "$pkgrel"\n'
        'emit arch "${arch[@]}"\n'
        'emit depends "${depends[@]}"\n'
        'emit makedepends "${makedepends[@]}"\n'
        'emit provides "${provides[@]}"\n'
        'emit conflicts "${conflicts[@]}"\n'
        'emit install "${install}"\n'
        'emit backup "${backup[@]}"\n'
        'emit source "${source[@]}"\n'
        'emit sha256sums "${sha256sums[@]}"\n'
        'emit source_x86_64 "${source_x86_64[@]}"\n'
        'emit source_aarch64 "${source_aarch64[@]}"\n'
        'emit source_armv7h "${source_armv7h[@]}"\n'
        'emit source_i686 "${source_i686[@]}"\n'
        'emit sha256sums_x86_64 "${sha256sums_x86_64[@]}"\n'
        'emit sha256sums_aarch64 "${sha256sums_aarch64[@]}"\n'
        'emit sha256sums_armv7h "${sha256sums_armv7h[@]}"\n'
        'emit sha256sums_i686 "${sha256sums_i686[@]}"\n'
    )
    result = run("bash", "-c", script, str(path))
    assert result.returncode == 0, result.stderr
    fields = {}
    for line in result.stdout.splitlines():
        key, _, value = line.partition("\t")
        fields.setdefault(key, []).append(value)
    return fields


def srcinfo_fields(path):
    """Parse a .SRCINFO file into a key -> list-of-values dict."""
    fields = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if " = " not in line:
            continue
        key, _, value = line.strip().partition(" = ")
        fields.setdefault(key, []).append(value)
    return fields


def scalar(fields, key):
    values = fields[key]
    assert len(values) == 1, (key, values)
    return values[0]


def build_stable_input(directory, version=VERSION, *, signed=False, fingerprint=None,
                       source_bytes=b"source snapshot\n", corrupt_source=False):
    """Write a complete, verifiable stable release payload into `directory`."""
    assets = {}
    for arch, slug in ARCH_SLUGS.items():
        name = f"AirScout-RS-v{version}-{slug}.pkg.tar.zst"
        data = f"arch asset {arch}\n".encode()
        (directory / name).write_bytes(data)
        assets[name] = hashlib.sha256(data).hexdigest()
    source_name = f"airscout-rs-{version}-source.tar.gz"
    (directory / source_name).write_bytes(source_bytes)
    assets[source_name] = hashlib.sha256(source_bytes).hexdigest()
    if corrupt_source:
        assets[source_name] = "0" * 64
    (directory / "SHA256SUMS").write_text(
        "".join(f"{digest}  {name}\n" for name, digest in sorted(assets.items())))
    inventory = {
        "tag": f"v{version}", "version": version, "channel": "stable",
        "checksums_signed": signed,
        "assets": [
            {"name": name, "sha256": assets[name], "format": "archlinux",
             "slug": slug, "signing": "not_applicable"}
            for arch, slug in ARCH_SLUGS.items()
            for name in [f"AirScout-RS-v{version}-{slug}.pkg.tar.zst"]
        ] + [
            {"name": source_name, "sha256": assets[source_name],
             "format": "source", "signing": "not_applicable"},
        ],
    }
    if fingerprint:
        inventory["checksum_fingerprint"] = fingerprint
    (directory / "inventory.json").write_text(json.dumps(inventory))
    return assets, source_name


class AurTests(unittest.TestCase):
    def _load_render(self):
        spec = importlib.util.spec_from_file_location("aur_render", RENDER)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_aur_recipes(self):
        for name in PACKAGES:
            with self.subTest(package=name):
                source = (AUR / name / "PKGBUILD").read_text(encoding="utf-8")
                fields = pkgbuild_fields(AUR / name / "PKGBUILD")
                self.assertEqual(scalar(fields, "pkgname"), name)
                self.assertEqual(scalar(fields, "install"), "arch-install")
                self.assertEqual(fields["backup"], ["etc/airscout-planeserver/planeserver.toml"])
                self.assertEqual(fields["provides"], ["airscout-rs"])
                self.assertEqual(set(fields["arch"]), set(ARCH_SLUGS))
                self.assertEqual(set(fields["depends"]), set(DEPENDS))
                # Organization maintainer, no personal identity anywhere.
                self.assertRegex(source, r"(?m)^# Maintainer: HamRadioOnline$")
                self.assertNotRegex(source, r"<[^>]+@[^>]+>")
                for marker in ("Philipp", "Wagner", "DO5AMF", "DN9APW", "DL2ALF"):
                    self.assertNotIn(marker, source)

                conflicts = set(fields["conflicts"])
                self.assertEqual(conflicts, set(PACKAGES) - {name})

        # source-versus-bin strategy. ${pkgver} expands when the PKGBUILD is
        # sourced, so the field values are concrete, but the raw text stays a
        # version-agnostic template.
        bin_source = (AUR / "airscout-rs-bin" / "PKGBUILD").read_text(encoding="utf-8")
        bin_fields = pkgbuild_fields(AUR / "airscout-rs-bin" / "PKGBUILD")
        self.assertEqual(set(bin_fields.get("source", [])), set())
        for arch, slug in ARCH_SLUGS.items():
            entries = bin_fields[f"source_{arch}"]
            self.assertEqual(len(entries), 1)
            self.assertEqual(entries[0], f"AirScout-RS-v{VERSION}-{slug}.pkg.tar.zst::"
                                         f"{REPOSITORY}/releases/download/v{VERSION}/"
                                         f"AirScout-RS-v{VERSION}-{slug}.pkg.tar.zst")
            self.assertEqual(bin_fields[f"sha256sums_{arch}"], ["SKIP"])
            self.assertIn(f"v${{pkgver}}-{slug}", bin_source)
        self.assertIn('cp -a "${srcdir}/usr" "${pkgdir}/"', bin_source)
        self.assertIn('cp -a "${srcdir}/etc" "${pkgdir}/"', bin_source)
        self.assertNotIn("cargo build", bin_source)
        self.assertNotIn("install-root.sh", bin_source)

        src_source = (AUR / "airscout-rs" / "PKGBUILD").read_text(encoding="utf-8")
        src_fields = pkgbuild_fields(AUR / "airscout-rs" / "PKGBUILD")
        self.assertEqual(len(src_fields["source"]), 1)
        self.assertEqual(src_fields["source"][0],
                         f"airscout-rs-{VERSION}-source.tar.gz::{REPOSITORY}"
                         f"/releases/download/v{VERSION}/airscout-rs-{VERSION}-source.tar.gz")
        self.assertIn("releases/download/v${pkgver}/airscout-rs-${pkgver}-source.tar.gz", src_source)
        self.assertEqual(src_fields["sha256sums"], ["SKIP"])
        self.assertEqual(set(src_fields["makedepends"]), set(MAKEDEPENDS))
        self.assertIn("cargo build --workspace --bins --release --locked", src_source)
        self.assertIn("packaging/linux/install-root.sh", src_source)

        git_source = (AUR / "airscout-rs-git" / "PKGBUILD").read_text(encoding="utf-8")
        git_fields = pkgbuild_fields(AUR / "airscout-rs-git" / "PKGBUILD")
        self.assertEqual(len(git_fields["source"]), 1)
        self.assertIn("git+https://github.com/HamRadioOnline/AirScout-RS.git", git_fields["source"][0])
        self.assertEqual(git_fields["sha256sums"], ["SKIP"])
        self.assertRegex(scalar(git_fields, "pkgver"), r"^" + re.escape(VERSION) + r"\.r\d+\.g[0-9a-f]+$")
        self.assertIn("pkgver()", git_source)
        self.assertIn("git rev-list --count HEAD", git_source)
        self.assertIn("git rev-parse --short HEAD", git_source)
        self.assertIn("cargo build --workspace --bins --release --locked", git_source)
        self.assertIn("packaging/linux/install-root.sh", git_source)

    def test_srcinfo_synced(self):
        for name in PACKAGES:
            with self.subTest(package=name):
                pkgs = pkgbuild_fields(AUR / name / "PKGBUILD")
                src = srcinfo_fields(AUR / name / ".SRCINFO")
                self.assertEqual(set(src["pkgbase"]), {name})
                self.assertEqual(set(src["pkgname"]), {name})
                self.assertEqual(set(src["pkgver"]), set(pkgs["pkgver"]))
                self.assertEqual(set(src["pkgrel"]), set(pkgs["pkgrel"]))
                self.assertEqual(set(src["arch"]), set(pkgs["arch"]))
                self.assertEqual(set(src["depends"]), set(pkgs["depends"]))
                self.assertEqual(set(src.get("makedepends", [])), set(pkgs.get("makedepends", [])))
                self.assertEqual(set(src["provides"]), set(pkgs["provides"]))
                self.assertEqual(set(src["conflicts"]), set(pkgs["conflicts"]))
                self.assertEqual(set(src["install"]), set(pkgs["install"]))
                self.assertEqual(set(src["backup"]), set(pkgs["backup"]))
                sources = set(pkgs.get("source", [])) | set(
                    sum((pkgs.get(f"source_{arch}", []) for arch in ARCH_SLUGS), []))
                checksums = set(pkgs.get("sha256sums", [])) | set(
                    sum((pkgs.get(f"sha256sums_{arch}", []) for arch in ARCH_SLUGS), []))
                self.assertEqual(
                    set(src.get("source", [])) | set(src.get("source_x86_64", []))
                    | set(src.get("source_aarch64", [])) | set(src.get("source_armv7h", []))
                    | set(src.get("source_i686", [])), sources)
                self.assertEqual(
                    set(src.get("sha256sums", [])) | set(src.get("sha256sums_x86_64", []))
                    | set(src.get("sha256sums_aarch64", [])) | set(src.get("sha256sums_armv7h", []))
                    | set(src.get("sha256sums_i686", [])), checksums)

    def test_repo_rewrite_preserves_state(self):
        with tempfile.TemporaryDirectory() as tmp:
            srcdir = Path(tmp) / "srcdir"
            pkgdir = Path(tmp) / "pkgdir"
            (srcdir / "usr/bin").mkdir(parents=True)
            for program in PROGRAMS:
                (srcdir / "usr/bin" / program).write_text("#!/bin/sh\nexit 0\n")
            (srcdir / "usr/share/applications").mkdir(parents=True)
            (srcdir / "usr/share/applications" / f"{APP_ID}.desktop").write_text("desktop\n")
            (srcdir / "etc/airscout-planeserver").mkdir(parents=True)
            (srcdir / "etc/airscout-planeserver/planeserver.toml").write_text('bind = "127.0.0.1"\n')
            for meta in (".PKGINFO", ".BUILDINFO", ".MTREE", ".INSTALL"):
                (srcdir / meta).write_text(f"{meta} metadata\n")
            pkgdir.mkdir()

            script = (
                f'source "{AUR / "airscout-rs-bin" / "PKGBUILD"}"\n'
                f'srcdir="{srcdir}"\n'
                f'pkgdir="{pkgdir}"\n'
                'package\n'
            )
            result = run("bash", "-c", script)
            self.assertEqual(result.returncode, 0, result.stderr)
            installed = {path.name for path in (pkgdir / "usr/bin").iterdir()}
            self.assertEqual(installed, PROGRAMS)
            self.assertTrue((pkgdir / "etc/airscout-planeserver/planeserver.toml").is_file())
            self.assertTrue((pkgdir / "usr/share/applications" / f"{APP_ID}.desktop").is_file())
            for meta in (".PKGINFO", ".BUILDINFO", ".MTREE", ".INSTALL"):
                self.assertFalse((pkgdir / meta).exists(), meta)
                self.assertFalse(list(pkgdir.glob(f"**/{meta}")), meta)

    def test_render_stable_and_rejects_prereleases(self):
        for tag in (f"beta-v{VERSION}-rc.1", f"alpha-v{VERSION}-test"):
            with self.subTest(tag=tag), tempfile.TemporaryDirectory() as tmp:
                directory = Path(tmp)
                (directory / "SHA256SUMS").write_text("")
                (directory / "inventory.json").write_text(json.dumps(
                    {"tag": tag, "version": VERSION, "channel": tag.split("-")[0],
                     "checksums_signed": True, "assets": []}))
                result = run(sys.executable, RENDER, "stable", tag, directory, directory / "out")
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((directory / "out").exists())

        # Unsigned checksums are rejected before any signature verification.
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            build_stable_input(directory, signed=False)
            result = run(sys.executable, RENDER, "stable", f"v{VERSION}", directory, directory / "out")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not signed", result.stderr)
            self.assertFalse((directory / "out").exists())

    def _gpg_home(self):
        tmp = tempfile.TemporaryDirectory(dir="/tmp")
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        home = root / "home"
        home.mkdir(mode=0o700)
        gpg = ["gpg", "--no-options", "--homedir", str(home), "--batch", "--no-tty",
               "--pinentry-mode", "loopback", "--passphrase", ""]
        self.addCleanup(run, "gpgconf", "--homedir", home, "--kill", "gpg-agent")
        return root, home, gpg

    def _gpg_generate_key(self, gpg, uid):
        result = run(*gpg, "--quick-generate-key", uid, "ed25519", "sign", "0")
        self.assertEqual(result.returncode, 0, result.stderr)
        listed = run(*gpg, "--with-colons", "--list-keys", "--with-fingerprint")
        self.assertEqual(listed.returncode, 0, listed.stderr)
        fprs = [line.split(":")[9] for line in listed.stdout.splitlines()
                if line.startswith("fpr:")]
        self.assertTrue(fprs, listed.stdout)
        # The freshly generated key is the last primary-key fingerprint listed.
        fingerprint = fprs[-1].upper()
        self.assertRegex(fingerprint, r"^[0-9A-F]{40}$|^[0-9A-F]{64}$")
        return fingerprint

    def _gpg_export(self, gpg, fingerprint=None):
        args = [*gpg, "--armor", "--export"]
        if fingerprint:
            args.append(fingerprint)
        result = run(*args)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def _gpg_sign(self, gpg, directory, local_user=None):
        args = [*gpg, "--armor", "--detach-sign"]
        if local_user:
            args += ["--local-user", local_user]
        args += ["--output", str(directory / "SHA256SUMS.asc"),
                 str(directory / "SHA256SUMS")]
        result = run(*args)
        self.assertEqual(result.returncode, 0, result.stderr)

    @unittest.skipUnless(shutil.which("gpg"), "GnuPG unavailable")
    def test_render_stable_verifies_signed_checksums(self):
        render = self._load_render()
        root, _home, gpg = self._gpg_home()
        fingerprint = self._gpg_generate_key(gpg, "AUR Fixture <aur@example.invalid>")
        public_key = self._gpg_export(gpg, fingerprint)

        def sign(directory):
            self._gpg_sign(gpg, directory)
            (directory / "SHA256SUMS.key.asc").write_text(public_key)

        directory = root / "input"
        directory.mkdir()
        assets, source_name = build_stable_input(directory, signed=True, fingerprint=fingerprint)
        sign(directory)
        output = directory / "out"
        render.render_stable(f"v{VERSION}", directory, output)

        bin_pkg = pkgbuild_fields(output / "airscout-rs-bin" / "PKGBUILD")
        self.assertEqual(scalar(bin_pkg, "pkgver"), VERSION)
        for arch, slug in ARCH_SLUGS.items():
            name = f"AirScout-RS-v{VERSION}-{slug}.pkg.tar.zst"
            self.assertEqual(bin_pkg[f"sha256sums_{arch}"], [assets[name]])
        src_pkg = pkgbuild_fields(output / "airscout-rs" / "PKGBUILD")
        self.assertEqual(scalar(src_pkg, "pkgver"), VERSION)
        self.assertEqual(src_pkg["sha256sums"], [assets[source_name]])
        self.assertEqual(src_pkg["source"], [
            f"airscout-rs-{VERSION}-source.tar.gz::{REPOSITORY}"
            f"/releases/download/v{VERSION}/airscout-rs-{VERSION}-source.tar.gz"])

        # A tampered manifest fails signature verification.
        tampered = root / "tampered"
        tampered.mkdir()
        assets_t, _ = build_stable_input(tampered, signed=True, fingerprint=fingerprint)
        sign(tampered)
        manifest = (tampered / "SHA256SUMS").read_text()
        (tampered / "SHA256SUMS").write_text(manifest.replace(assets_t[source_name], "0" * 64, 1))
        with self.assertRaises(subprocess.CalledProcessError):
            render.render_stable(f"v{VERSION}", tampered, tampered / "out")

        # A wrong key fingerprint is rejected even though the signature verifies.
        wrong = root / "wrong"
        wrong.mkdir()
        build_stable_input(wrong, signed=True, fingerprint="0" * 64)
        sign(wrong)
        with self.assertRaises(ValueError):
            render.render_stable(f"v{VERSION}", wrong, wrong / "out")

        # A validly signed manifest that mislists a hash is rejected.
        mismatch = root / "mismatch"
        mismatch.mkdir()
        build_stable_input(mismatch, signed=True, fingerprint=fingerprint, corrupt_source=True)
        sign(mismatch)
        with self.assertRaises(ValueError):
            render.render_stable(f"v{VERSION}", mismatch, mismatch / "out")

    @unittest.skipUnless(shutil.which("gpg"), "GnuPG unavailable")
    def test_render_stable_rejects_missing_arch_asset(self):
        render = self._load_render()
        root, _home, gpg = self._gpg_home()
        fingerprint = self._gpg_generate_key(gpg, "AUR Fixture <aur@example.invalid>")
        public_key = self._gpg_export(gpg, fingerprint)

        directory = root / "input"
        directory.mkdir()
        build_stable_input(directory, signed=True, fingerprint=fingerprint)
        # Drop one Arch asset hash from the manifest, then sign the reduced manifest.
        dropped = f"AirScout-RS-v{VERSION}-linux-x86_64.pkg.tar.zst"
        manifest = (directory / "SHA256SUMS").read_text()
        (directory / "SHA256SUMS").write_text(
            "".join(line for line in manifest.splitlines(keepends=True) if dropped not in line))
        self._gpg_sign(gpg, directory)
        (directory / "SHA256SUMS.key.asc").write_text(public_key)
        with self.assertRaisesRegex(ValueError, "missing a required AUR hash"):
            render.render_stable(f"v{VERSION}", directory, directory / "out")

    @unittest.skipUnless(shutil.which("gpg"), "GnuPG unavailable")
    def test_render_stable_rejects_tampered_arch_asset(self):
        render = self._load_render()
        root, _home, gpg = self._gpg_home()
        fingerprint = self._gpg_generate_key(gpg, "AUR Fixture <aur@example.invalid>")
        public_key = self._gpg_export(gpg, fingerprint)

        directory = root / "input"
        directory.mkdir()
        build_stable_input(directory, signed=True, fingerprint=fingerprint)
        self._gpg_sign(gpg, directory)
        (directory / "SHA256SUMS.key.asc").write_text(public_key)
        # Tamper an Arch asset's bytes after signing: the signature still verifies
        # but the file no longer matches its pinned hash.
        asset = directory / f"AirScout-RS-v{VERSION}-linux-x86_64.pkg.tar.zst"
        asset.write_bytes(b"tampered arch asset bytes\n")
        with self.assertRaisesRegex(ValueError, "Arch asset hash differs"):
            render.render_stable(f"v{VERSION}", directory, directory / "out")

    @unittest.skipUnless(shutil.which("gpg"), "GnuPG unavailable")
    def test_render_stable_rejects_foreign_signature_key(self):
        render = self._load_render()
        root, _home, gpg = self._gpg_home()
        real_fingerprint = self._gpg_generate_key(gpg, "Release Key <release@example.invalid>")
        real_key = self._gpg_export(gpg, real_fingerprint)
        foreign_fingerprint = self._gpg_generate_key(gpg, "Foreign Key <foreign@example.invalid>")
        foreign_key = self._gpg_export(gpg, foreign_fingerprint)

        directory = root / "input"
        directory.mkdir()
        build_stable_input(directory, signed=True, fingerprint=real_fingerprint)
        # The key file carries the release key and a second key, but the signature
        # is made by the second key: verification must reject it.
        self._gpg_sign(gpg, directory, local_user=foreign_fingerprint)
        (directory / "SHA256SUMS.key.asc").write_text(real_key + "\n" + foreign_key)
        with self.assertRaisesRegex(ValueError, "signature key does not match"):
            render.render_stable(f"v{VERSION}", directory, directory / "out")

    def test_render_git_derives_pkgver(self):
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()
            (repo / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\nversion = "9.8.7"\n')

            def git(*args):
                return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)

            git("init", "-q")
            git("config", "user.name", "test")
            git("config", "user.email", "test@example.invalid")
            git("config", "commit.gpgsign", "false")
            git("add", "Cargo.toml")
            git("commit", "-q", "-m", "cargo")
            for i in range(3):
                (repo / "file").write_text(f"commit {i}\n")
                git("add", "file")
                git("commit", "-q", "-m", f"c{i}")
            count = git("rev-list", "--count", "HEAD").stdout.strip()
            short = git("rev-parse", "--short", "HEAD").stdout.strip()
            output = Path(tmp) / "out"
            result = run(sys.executable, RENDER, "git", "HEAD", repo, output)
            self.assertEqual(result.returncode, 0, result.stderr)
            fields = pkgbuild_fields(output / "airscout-rs-git" / "PKGBUILD")
            self.assertEqual(scalar(fields, "pkgver"), f"9.8.7.r{count}.g{short}")
            self.assertEqual((output / "airscout-rs-git" / "arch-install").read_text(),
                             (LINUX / "scripts/arch-install").read_text())

    def test_render_git_handles_workspace_section_layout(self):
        # A comment between [workspace.package] and the version key must not
        # break parsing, and a missing key must raise a clear error (no TypeError).
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp) / "repo"
            repo.mkdir()

            def git(*args):
                return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)

            git("init", "-q")
            git("config", "user.name", "test")
            git("config", "user.email", "test@example.invalid")
            git("config", "commit.gpgsign", "false")
            (repo / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\n'
                '# a comment before the key\nversion = "9.8.7"\n')
            git("add", "Cargo.toml")
            git("commit", "-q", "-m", "c0")
            output = Path(tmp) / "out"
            result = run(sys.executable, RENDER, "git", "HEAD", repo, output)
            self.assertEqual(result.returncode, 0, result.stderr)
            fields = pkgbuild_fields(output / "airscout-rs-git" / "PKGBUILD")
            self.assertRegex(scalar(fields, "pkgver"), r"^9\.8\.7\.r1\.g[0-9a-f]+$")

            (repo / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\n# no version here\n')
            git("add", "Cargo.toml")
            git("commit", "-q", "-m", "c1")
            result = run(sys.executable, RENDER, "git", "HEAD", repo, output / "broken")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("no version key", result.stderr)
            self.assertNotIn("TypeError", result.stderr)
            self.assertNotIn("Traceback", result.stderr)

    def test_rendered_recipes_match_committed_bytes(self):
        render = self._load_render()
        git_pkgver = scalar(pkgbuild_fields(AUR / "airscout-rs-git" / "PKGBUILD"), "pkgver")
        recipes = {
            "airscout-rs-bin": render.render_bin(VERSION, {slug: "SKIP" for slug in ARCH_SLUGS.values()}),
            "airscout-rs": render.render_source(VERSION, "SKIP"),
            "airscout-rs-git": render.render_git_pkgbuild(git_pkgver),
        }
        for name, rendered in recipes.items():
            with self.subTest(package=name):
                committed = (AUR / name / "PKGBUILD").read_text(encoding="utf-8")
                self.assertEqual(rendered, committed)
                self.assertTrue(rendered.endswith("\n"))
                result = run("bash", "-n", str(AUR / name / "PKGBUILD"))
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_arch_install_copies_match(self):
        source = (LINUX / "scripts" / "arch-install").read_bytes()
        for name in PACKAGES:
            with self.subTest(package=name):
                self.assertEqual((AUR / name / "arch-install").read_bytes(), source)

    def test_publish_fast_forwards_and_validates_host_key(self):
        fake_bin, log = self.fake_tools()
        env = dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}",
                   TOOL_LOG=str(log), AUR_SSH_PRIVATE_KEY="synthetic-ed25519-private-key\n")
        recipe = Path(tempfile.mkdtemp())
        (recipe / "PKGBUILD").write_text("pkgname=airscout-rs-bin\n")
        (recipe / ".SRCINFO").write_text("pkgbase = airscout-rs-bin\n")
        (recipe / "arch-install").write_text("post_install() { :; }\n")
        self.addCleanup(shutil.rmtree, recipe, ignore_errors=True)

        env["EXPECTED_FINGERPRINT"] = AUR_ED25519
        result = run("bash", PUBLISH, recipe, "airscout-rs-bin", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        log_text = log.read_text()
        self.assertIn("clone ssh://aur@aur.archlinux.org/airscout-rs-bin.git", log_text)
        self.assertIn("commit -S -m Update airscout-rs-bin", log_text)
        self.assertIn("push origin HEAD:master", log_text)
        self.assertIn("config user.name github-actions[bot]", log_text)
        self.assertIn("ssh-keyscan -T 10 -t ed25519 aur.archlinux.org", log_text)

        # A mismatched host key aborts before any clone.
        env["EXPECTED_FINGERPRINT"] = "SHA256:different"
        log.write_text("")
        result = run("bash", PUBLISH, recipe, "airscout-rs-bin", env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fingerprint", (result.stderr or "").lower())
        self.assertNotIn("clone", log.read_text())

    def test_publish_rejects_empty_key(self):
        recipe = Path(tempfile.mkdtemp())
        (recipe / "PKGBUILD").write_text("pkgname=airscout-rs-bin\n")
        (recipe / ".SRCINFO").write_text("pkgbase = airscout-rs-bin\n")
        self.addCleanup(shutil.rmtree, recipe, ignore_errors=True)
        env = dict(os.environ, AUR_SSH_PRIVATE_KEY="")
        result = run("bash", PUBLISH, recipe, "airscout-rs-bin", env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("AUR_SSH_PRIVATE_KEY", result.stderr)

    def test_publish_skips_unchanged_tree(self):
        fake_bin, log = self.fake_tools()
        env = dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}",
                   TOOL_LOG=str(log), AUR_SSH_PRIVATE_KEY="synthetic-ed25519-private-key\n",
                   EXPECTED_FINGERPRINT=AUR_ED25519, FAKE_GIT_DIFF="0")
        recipe = Path(tempfile.mkdtemp())
        (recipe / "PKGBUILD").write_text("pkgname=airscout-rs-bin\n")
        (recipe / ".SRCINFO").write_text("pkgbase = airscout-rs-bin\n")
        self.addCleanup(shutil.rmtree, recipe, ignore_errors=True)
        result = run("bash", PUBLISH, recipe, "airscout-rs-bin", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("already current", result.stdout)
        self.assertNotIn("commit -S", log.read_text())

    def test_publish_refuses_downgrade(self):
        fake_bin, log = self.fake_tools()
        recipe = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, recipe, ignore_errors=True)
        (recipe / "PKGBUILD").write_text("pkgname=airscout-rs-git\n")
        (recipe / ".SRCINFO").write_text(
            "pkgbase = airscout-rs-git\n\tpkgver = 0.1.0.r67.gabcdef0\n")
        env = dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}",
                   TOOL_LOG=str(log), AUR_SSH_PRIVATE_KEY="synthetic-ed25519-private-key\n",
                   EXPECTED_FINGERPRINT=AUR_ED25519,
                   FAKE_REMOTE_PKGVER="0.1.0.r68.ghigher", FAKE_VERCMP="-1")
        result = run("bash", PUBLISH, recipe, "airscout-rs-git", env=env)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("refusing to downgrade", result.stderr)
        self.assertNotIn("commit -S", log.read_text())
        self.assertIn("docker", log.read_text())

    def test_publish_equal_pkgver_content_change_publishes(self):
        fake_bin, log = self.fake_tools()
        github_output = Path(tempfile.mkdtemp()) / "github-output"
        self.addCleanup(shutil.rmtree, github_output.parent, ignore_errors=True)
        recipe = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, recipe, ignore_errors=True)
        (recipe / "PKGBUILD").write_text("pkgname=airscout-rs-bin\n")
        (recipe / ".SRCINFO").write_text(
            "pkgbase = airscout-rs-bin\n\tpkgver = 1.2.3\n")
        env = dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}",
                   TOOL_LOG=str(log), AUR_SSH_PRIVATE_KEY="synthetic-ed25519-private-key\n",
                   EXPECTED_FINGERPRINT=AUR_ED25519,
                   FAKE_REMOTE_PKGVER="1.2.3", FAKE_VERCMP="0",
                   GITHUB_OUTPUT=str(github_output))
        result = run("bash", PUBLISH, recipe, "airscout-rs-bin", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("commit -S", log.read_text())
        self.assertIn("push origin HEAD:master", log.read_text())
        self.assertIn("published=true", github_output.read_text())

    def test_publish_equal_pkgver_no_change_reports_not_published(self):
        fake_bin, log = self.fake_tools()
        github_output = Path(tempfile.mkdtemp()) / "github-output"
        self.addCleanup(shutil.rmtree, github_output.parent, ignore_errors=True)
        github_output.write_text("")
        recipe = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, recipe, ignore_errors=True)
        (recipe / "PKGBUILD").write_text("pkgname=airscout-rs-bin\n")
        (recipe / ".SRCINFO").write_text(
            "pkgbase = airscout-rs-bin\n\tpkgver = 1.2.3\n")
        env = dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}",
                   TOOL_LOG=str(log), AUR_SSH_PRIVATE_KEY="synthetic-ed25519-private-key\n",
                   EXPECTED_FINGERPRINT=AUR_ED25519,
                   FAKE_REMOTE_PKGVER="1.2.3", FAKE_VERCMP="0",
                   FAKE_GIT_DIFF="0", GITHUB_OUTPUT=str(github_output))
        result = run("bash", PUBLISH, recipe, "airscout-rs-bin", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("already current", result.stdout)
        self.assertNotIn("commit -S", log.read_text())
        self.assertEqual(github_output.read_text(), "")

    def test_publish_fails_loudly_on_readonly_clone_error(self):
        fake_bin, log = self.fake_tools()
        recipe = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, recipe, ignore_errors=True)
        (recipe / "PKGBUILD").write_text("pkgname=airscout-rs-bin\n")
        (recipe / ".SRCINFO").write_text(
            "pkgbase = airscout-rs-bin\n\tpkgver = 1.2.3\n")
        env = dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}",
                   TOOL_LOG=str(log), AUR_SSH_PRIVATE_KEY="synthetic-ed25519-private-key\n",
                   EXPECTED_FINGERPRINT=AUR_ED25519,
                   FAKE_REMOTE_PKGVER="1.2.4")
        for failure in ("network", "notfound"):
            with self.subTest(failure=failure):
                log.write_text("")
                env["FAKE_CLONE_FAIL"] = failure
                result = run("bash", PUBLISH, recipe, "airscout-rs-bin", env=env)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("failed to read the live AUR package", result.stderr)
                self.assertNotIn("commit -S", log.read_text())

    def test_publish_treats_missing_repo_as_first_publish(self):
        fake_bin, log = self.fake_tools()
        recipe = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, recipe, ignore_errors=True)
        (recipe / "PKGBUILD").write_text("pkgname=airscout-rs-bin\n")
        (recipe / ".SRCINFO").write_text(
            "pkgbase = airscout-rs-bin\n\tpkgver = 1.2.3\n")
        env = dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}",
                   TOOL_LOG=str(log), AUR_SSH_PRIVATE_KEY="synthetic-ed25519-private-key\n",
                   EXPECTED_FINGERPRINT=AUR_ED25519)
        result = run("bash", PUBLISH, recipe, "airscout-rs-bin", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("commit -S", log.read_text())

    def test_publish_aborts_on_lookup_error(self):
        fake_bin, log = self.fake_tools()
        env = dict(os.environ, PATH=f"{fake_bin}:{os.environ['PATH']}",
                   TOOL_LOG=str(log), AUR_SSH_PRIVATE_KEY="synthetic-ed25519-private-key\n",
                   EXPECTED_FINGERPRINT=AUR_ED25519, FAKE_LS_REMOTE_STATUS="128")
        result = run("bash", PUBLISH, fake_bin, "airscout-rs-bin", env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("failed to look up the live AUR package", result.stderr)
        self.assertNotIn("clone", log.read_text())

    def test_aur_pins(self):
        pin = json.loads((ROOT / "packaging/tools.lock.json").read_text(encoding="utf-8"))
        host = pin["aur-ssh-host"]["aur.archlinux.org"]
        self.assertEqual(host["ed25519"], AUR_ED25519)
        self.assertEqual(host["source"], "https://aur.archlinux.org/")
        builder = pin["aur-builder"]["linux-x86_64"]
        self.assertEqual(builder["digest"], ARCH_BUILDER_DIGEST)
        self.assertEqual(builder["url"], "docker.io/library/archlinux:base-devel")
        self.assertEqual(builder["source"],
                         "https://registry-1.docker.io/v2/library/archlinux/manifests/base-devel")

    def test_workflow_structure(self):
        caller = workflow_jobs("release.yml")["publish-aur"]
        self.assertIn("release", caller["needs"])
        self.assertEqual(caller["if"],
                         "${{ success() && needs.release.outputs.aur_eligible == 'true' }}")
        source = (WORKFLOWS / "aur-publish.yml").read_text()
        callee = workflow_jobs("aur-publish.yml")
        job = callee["aur"]
        self.assertEqual(job["environment"], "release")
        self.assertEqual(job["runs-on"], "ubuntu-latest")
        if yaml:
            call = yaml.load(source, Loader=yaml.BaseLoader)["on"]["workflow_call"]
            self.assertEqual(call["secrets"]["AUR_SSH_PRIVATE_KEY"], {"required": "false"})
            self.assertEqual(call["inputs"]["tag"], {"required": "true", "type": "string"})
            self.assertIn("|| 'false'", call["outputs"]["published"]["value"])
        else:
            declared = source.split("\n    secrets:\n", 1)[1].split("\n    outputs:", 1)[0]
            self.assertRegex(declared, r"(?m)^      AUR_SSH_PRIVATE_KEY:\n        required: false$")
            self.assertIn("jobs.aur.outputs.published || 'false'", source)
        self.assertIn("name: release-assets-verified", source)

        nightly = (WORKFLOWS / "aur-nightly-git.yml").read_text()
        if yaml:
            data = yaml.load(nightly, Loader=yaml.BaseLoader)
            triggers = data["on"]
            self.assertIn("workflow_dispatch", triggers)
            schedule = triggers["schedule"]
            self.assertEqual(len(schedule), 1)
            self.assertEqual(schedule[0]["cron"], "0 3 * * *")
            self.assertEqual(data["concurrency"], {"group": "aur-git", "cancel-in-progress": "false"})
            jobs = data["jobs"]
            self.assertIn("render", jobs)
            self.assertNotIn("environment", jobs["render"])
            self.assertIn("publish", jobs)
            self.assertEqual(jobs["publish"]["environment"], "release")
            self.assertIn("render", jobs["publish"]["needs"])
        else:
            self.assertIn("workflow_dispatch:", nightly)
            self.assertIn("schedule:", nightly)
            self.assertIn("- cron:", nightly)
            self.assertRegex(nightly, r"(?m)^concurrency:\n  group: aur-git\n  cancel-in-progress: false$")
            self.assertIn("environment: release", nightly)
        self.assertIn("AUR_SSH_PRIVATE_KEY", nightly)
        # Checkout is pinned to main without persisted credentials.
        self.assertIn("ref: main", nightly)
        self.assertIn("persist-credentials: false", nightly)

    def fake_tools(self):
        fake = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, fake, ignore_errors=True)
        log = fake / "tool.log"
        (fake / "ssh-keygen").write_text(
            '#!/bin/sh\n'
            'if [ "$1" = "-y" ]; then echo "ssh-ed25519 AAAAkey"; exit 0; fi\n'
            'if [ "$1" = "-lf" ]; then printf "256 %s %s (ED25519)\\n" "$EXPECTED_FINGERPRINT" "$2"; exit 0; fi\n'
            'echo "ssh-keygen $*" >> "$TOOL_LOG"\n'
        )
        (fake / "ssh-keyscan").write_text(
            '#!/bin/sh\n'
            'echo "ssh-keyscan $*" >> "$TOOL_LOG"\n'
            'printf "aur.archlinux.org ssh-ed25519 AAAAkey\\n"\n'
        )
        (fake / "git").write_text(
            '#!/bin/sh\n'
            'echo "git $*" >> "$TOOL_LOG"\n'
            'if [ "$1" = ls-remote ]; then\n'
            '  [ -z "${FAKE_LS_REMOTE_STATUS:-}" ] || exit "$FAKE_LS_REMOTE_STATUS"\n'
            '  [ -n "${FAKE_REMOTE_PKGVER:-}" ] || exit 2\n'
            '  echo "abcdef refs/heads/master"; exit 0\n'
            'fi\n'
            'if [ "${3:-}" = symbolic-ref ]; then echo refs/heads/master; exit 0; fi\n'
            'if [ "${3:-}" = show-ref ]; then [ -n "${FAKE_REMOTE_PKGVER:-}" ]; exit $?; fi\n'
            'if [ "${3:-}" = clone ]; then\n'
            '  if [ -n "${FAKE_CLONE_FAIL:-}" ]; then\n'
            '    if [ "$FAKE_CLONE_FAIL" = notfound ]; then\n'
            '      echo "fatal: remote repository not found" >&2\n'
            '    else\n'
            '      echo "fatal: could not read from remote repository" >&2\n'
            '    fi\n'
            '    exit 128\n'
            '  fi\n'
            '  last=""; for a in "$@"; do last=$a; done; mkdir -p "$last"\n'
            '  if [ -n "${FAKE_REMOTE_PKGVER:-}" ]; then\n'
            '    printf "pkgbase = x\\n\\tpkgver = %s\\n" "$FAKE_REMOTE_PKGVER" > "$last/.SRCINFO"\n'
            '  fi\n'
            'fi\n'
            'for a in "$@"; do [ "$a" = diff ] && exit "${FAKE_GIT_DIFF:-1}"; done\n'
            'exit 0\n'
        )
        (fake / "docker").write_text(
            '#!/bin/sh\n'
            'echo "docker $*" >> "$TOOL_LOG"\n'
            'printf "%s\\n" "${FAKE_VERCMP:-1}"\n'
        )
        for name in ("ssh-keygen", "ssh-keyscan", "git", "docker"):
            (fake / name).chmod(0o755)
        return fake, log


if __name__ == "__main__":
    unittest.main()
