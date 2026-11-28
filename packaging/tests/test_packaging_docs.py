import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
README = ROOT / "README.md"
DOC = ROOT / "docs/packaging.md"

APP_ID = "io.github.hamradioonline.AirScoutRS"
WINGET_ID = "HamRadioOnline.AirScout-RS"
GHCR = "ghcr.io/hamradioonline/airscout-planeserver"
REPO = "https://github.com/HamRadioOnline/AirScout-RS"

# Every repository secret named in the release workflows; the operations section
# must list each of these by name (never by value).
SECRETS = (
    "CERTUM_ACCOUNT_EMAIL", "CERTUM_TOTP_SEED", "CERTUM_CERT_FINGERPRINT",
    "MACOS_CERT_P12", "MACOS_CERT_PASSWORD", "MACOS_NOTARY_KEY",
    "MACOS_NOTARY_KEY_ID", "MACOS_NOTARY_ISSUER",
    "FLATPAK_GPG_PRIVATE_KEY", "RELEASE_GPG_PRIVATE_KEY", "RELEASE_GPG_PASSPHRASE",
    "AUR_SSH_PRIVATE_KEY", "HOMEBREW_TAP_TOKEN", "WINGET_TOKEN",
)

# Package-source identifiers that the install matrix must name, each as a whole
# phrase rather than a bare substring that other names could satisfy.
PACKAGE_SOURCES = (
    f"{APP_ID}.flatpakref",
    f"{APP_ID}.beta.flatpakref",
    "airscout-rs.flatpakrepo",
    "brew install --cask HamRadioOnline/homebrew-tap/airscout-rs ",
    "brew install --cask HamRadioOnline/homebrew-tap/airscout-rs@beta",
    "`airscout-rs-bin` — the prebuilt",
    "`airscout-rs` — built from the release source",
    "`airscout-rs-git` — built from the latest",
    "winget install HamRadioOnline.AirScout-RS",
)

# Facts from the "owner setup" checklist: one item per channel, each naming the
# provisioning action and the channel it unlocks.
OWNER_SETUP = (
    "Certum SimplySign",
    "App Store Connect API key",
    "initialized default branch",
    "microsoft/winget-pkgs",
    "aur.archlinux.org",
    "package public",
    'Source = "GitHub Actions"',
    "github-pages",
)


class PackagingDocsTests(unittest.TestCase):
    def read(self, path):
        return path.read_text(encoding="utf-8")

    def test_owner_preflight(self):
        doc = self.read(DOC)
        checklist = doc.partition("### Owner pre-flight checklist")[2].split("\n### ", 1)[0]
        for text in ("`release` and `github-pages`", "Deployment branches and tags",
                     "`v*`, `alpha-v*`, and `beta-v*`", 'Source = "GitHub Actions"',
                     "link the GHCR package to the repository", "make the package public",
                     "after the first push", "one approval per `release` deployment"):
            with self.subTest(text=text):
                self.assertIn(text, checklist)

    def test_linux_package_docs(self):
        doc = self.read(DOC)
        for text in ("| x86_64 | Ubuntu 22.04 | 2.35 |", "| arm64 | Ubuntu 22.04 | 2.35 |",
                     '`bind = "127.0.0.1"`', "0.0.0.0 if the file or bind setting is removed"):
            with self.subTest(text=text):
                self.assertIn(text, doc)

    def test_flatpak_history_docs(self):
        doc = self.read(DOC)
        for text in ("current and previous OSTree commits per ref", "single orphan commit",
                     "`--force-with-lease`", "install and run the signed candidate"):
            with self.subTest(text=text):
                self.assertIn(text, doc)

    def test_docs_cover_release_contract(self):
        readme = self.read(README)
        doc = self.read(DOC)
        combined = readme + "\n" + doc

        # The README links to the install matrix and verification instructions,
        # and carries the release signing key section the fingerprint check
        # compares against.
        self.assertIn("docs/packaging.md", readme)
        self.assertIn("## Release signing key", readme)
        self.assertIn("primary-key fingerprint", readme)

        # Application ID, package identifiers, the container image, the
        # corresponding source URL and the checksum signature are present.
        for term in (APP_ID, WINGET_ID, GHCR, REPO, "SHA256SUMS.asc",
                     "SHA256SUMS.key.asc", "systemctl enable --now airscout-planeserver"):
            self.assertIn(term, combined, term)
        for term in PACKAGE_SOURCES:
            self.assertIn(term, combined, term)

        # Ports are exact: Win-Test tracking output is UDP 9871, the plane
        # server listener is TCP (HTTP) 9872.
        self.assertIn("`9871` | UDP broadcast | Win-Test tracking output", combined)
        self.assertIn("`9872` | TCP (HTTP) | Plane server listener", combined)

        # The runtime libraries each Linux package depends on are named.
        for library in ("libgtk-3", "libayatana-appindicator", "libxdo", "ALSA"):
            self.assertIn(library, combined, library)

        # Source credit: original author, license and corresponding source URL.
        for credit in ("DL2ALF", "GPL-3.0-only"):
            self.assertIn(credit, combined)

        # Verification: check the primary-key fingerprint first, then import the
        # key, verify the detached signature, and check the files against the
        # manifest. The fingerprint must come from a second channel, not from
        # the release itself.
        self.assertIn("gpg --show-keys --with-fingerprint SHA256SUMS.key.asc", combined)
        self.assertIn("gpg --import SHA256SUMS.key.asc", combined)
        self.assertIn("gpg --verify SHA256SUMS.asc SHA256SUMS", combined)
        self.assertIn("sha256sum -c SHA256SUMS", combined)
        self.assertIn("shasum -a 256 -c SHA256SUMS", combined)
        self.assertIn("second channel", combined)

        # Flatpak: the repo descriptor is added with remote-add and the channel
        # reference installed with --from; update path is flatpak update.
        self.assertIn("flatpak remote-add --if-not-exists airscout-rs airscout-rs.flatpakrepo", combined)
        self.assertIn("flatpak install --from io.github.hamradioonline.AirScoutRS.flatpakref", combined)
        self.assertIn("flatpak update", combined)
        # An unsigned Flatpak candidate is never published: the channel stays
        # closed when FLATPAK_GPG_PRIVATE_KEY is absent.
        self.assertNotIn("published unsigned", combined)
        self.assertIn("no Flatpak repository is published or deployed", combined)

        # Per-format invocation details: Flatpak CLI wrappers and serial opt-in,
        # AppImage --tool dispatch and desktop integration, the container's
        # volume mount points, and the Docker pull-by-digest update path.
        self.assertIn("flatpak run --command=", combined)
        self.assertIn("--device=all", combined)
        self.assertIn("--tool airscout-planeserver", combined)
        self.assertIn("~/.local/share/applications", combined)
        self.assertIn("replacing the old AppImage", combined)
        self.assertIn("-v airscout-config:/config", combined)
        self.assertIn("-v airscout-data:/data", combined)
        self.assertIn("docker pull", combined)
        self.assertIn("pull it by digest", combined)

        # Update paths: Homebrew, WinGet and the MSI over-install.
        self.assertIn("brew upgrade --cask", combined)
        self.assertIn("winget upgrade", combined)
        self.assertIn("MajorUpgrade", combined)
        self.assertIn("paru -Syu", combined)

        # Native Linux packages are standalone GitHub Release assets (no distro
        # repository): update by over-installing the downloaded asset.
        self.assertIn("sudo apt install ./", combined)
        self.assertIn("sudo dnf install ./", combined)
        self.assertIn("rpm -U", combined)
        self.assertIn("sudo pacman -U", combined)
        self.assertNotIn("apt upgrade", combined)
        self.assertNotIn("dnf upgrade", combined)
        self.assertNotIn("pacman -Syu", combined)

        # glibc floors: x86_64 is held at glibc 2.35 (Ubuntu 22.04), and the
        # release notes state each asset's floor.
        self.assertIn("glibc 2.35", combined)
        self.assertIn("Ubuntu 22.04", combined)

        # Explicit network exposure and config/state paths.
        self.assertIn("/etc/airscout-planeserver/planeserver.toml", combined)
        self.assertIn("0.0.0.0", combined)
        self.assertIn('"eu", "airscout", "AirScout-RS"', combined)
        self.assertIn("settings.toml", combined)

        # Upgrade preservation: the packaged config is a conffile/noreplace/
        # backup entry; upgrades keep local edits (dpkg prompts or keeps, rpm
        # writes .rpmnew, pacman writes .pacnew) and removal saves modified
        # files (.rpmsave / .pacsave) instead of overwriting them.
        self.assertIn("conffile", combined)
        self.assertIn("noreplace", combined)
        self.assertIn(".rpmnew", combined)
        self.assertIn(".pacnew", combined)
        self.assertIn(".rpmsave", combined)
        self.assertIn(".pacsave", combined)

        # macOS CLI: the cask links the six binaries onto PATH.
        self.assertIn("airscout-rs.rb.in", combined)

        # Windows: the ZIP does not register itself on PATH.
        self.assertIn("add the extracted directory to `PATH`", combined)

        # Signing facts: Certum-signed EXEs inside the MSIs, conditional macOS
        # signing, and the unsigned release-notes marking.
        self.assertIn("EXEs inside the MSIs", combined)
        self.assertIn("unsigned and unnotarized", combined)
        self.assertIn("Unsigned EXEs and MSIs", combined)
        self.assertIn("Unsigned and unnotarized DMGs", combined)

        # Pipeline: fmt is Linux-only; collect is a script and sign-checksums.sh
        # (not collect) writes the checksum sidecars.
        self.assertIn("cargo fmt", combined)
        self.assertIn("Linux only", combined)
        self.assertIn("collect.py", combined)
        self.assertIn("sign-checksums.sh", combined)
        self.assertIn("publish.py", combined)

        # The operations section lists every repository secret by name, the owner
        # approval on the `release` environment, and the owner-setup checklist.
        for secret in SECRETS:
            self.assertIn(secret, doc, secret)
        self.assertIn("required reviewer", doc)
        self.assertIn("HamRadioOnline", doc)
        for term in OWNER_SETUP:
            self.assertIn(term, doc, term)
        # Signed Windows/macOS files expose the public certificate subject; the
        # note must stay generic (no actual name).
        self.assertIn("public certificate subject", doc)


if __name__ == "__main__":
    unittest.main()
