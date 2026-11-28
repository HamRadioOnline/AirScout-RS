#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

fail() {
    echo "build.sh: $*" >&2
    exit 1
}

if [[ $# != 3 ]]; then
    echo 'Usage: build.sh <stage-dir> <tag> <output-dir>' >&2
    exit 2
fi

stage=$(cd "$1" && pwd)
tag=$2
output=$3
packaging=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
[[ ! -e "$output" && ! -L "$output" ]] || fail "Output already exists: $output"
mkdir -p -- "$(dirname -- "$output")"
output=$(cd "$(dirname -- "$output")" && pwd)/$(basename -- "$output")
work=$(mktemp -d "$(dirname -- "$output")/.airscout-appimage.XXXXXX")
trap 'rm -rf -- "$work"' EXIT

GITHUB_OUTPUT="$work/tag" "$packaging/release/check-tag.sh" "$tag"
version=$(sed -n 's/^version=//p' "$work/tag")

stage_fields=$(python3 - "$stage/stage.json" "$version" <<'PY'
import json
from pathlib import Path
import re
import sys

path, version = sys.argv[1:]
stage = json.loads(Path(path).read_text())
if stage["version"] != version:
    sys.exit("Stage version differs from the tag")
for field, pattern in {
    "target": r"[a-z0-9_-]+",
    "slug": r"linux-[a-z0-9_]+",
    "glibc_floor": r"[0-9]+\.[0-9]+(?:\.[0-9]+)?",
}.items():
    if not re.fullmatch(pattern, stage[field]):
        sys.exit(f"Invalid stage field: {field}")
print(stage["target"], stage["slug"], stage["glibc_floor"])
PY
)
read -r target slug staged_floor <<< "$stage_fields"

case "$target:$slug" in
    x86_64-unknown-linux-gnu:linux-x86_64)
        arch=x86_64 triplet=x86_64-linux-gnu machine='Advanced Micro Devices X86-64' distro=ubuntu:22.04 ;;
    aarch64-unknown-linux-gnu:linux-arm64)
        arch=aarch64 triplet=aarch64-linux-gnu machine=AArch64 distro=ubuntu:22.04 ;;
    armv7-unknown-linux-gnueabihf:linux-armhf)
        arch=armhf triplet=arm-linux-gnueabihf machine=ARM distro=debian:12 ;;
    i686-unknown-linux-gnu:linux-i686)
        arch=i686 triplet=i386-linux-gnu machine='Intel 80386' distro=debian:12 ;;
    *) fail "Unexpected stage target and slug: $target $slug" ;;
esac
# Resolve dependencies from the same distro release that produced the stage.
actual_distro=$(. /etc/os-release && printf '%s:%s' "$ID" "$VERSION_ID")
[[ "$actual_distro" == "$distro" ]] || fail "$slug needs target libraries from $distro"

case "$(uname -m)" in
    x86_64) tool_platform=linux-x86_64 ;;
    aarch64) tool_platform=linux-arm64 ;;
    *) fail "Unsupported appimagetool build host" ;;
esac

app_id=io.github.hamradioonline.AirScoutRS
appdir="$work/AirScout-RS.AppDir"
"$packaging/linux/install-root.sh" "$stage" "$work/shared"
mkdir -p -- "$appdir/usr"
mv -- "$work/shared/usr/bin" "$work/shared/usr/share" "$appdir/usr/"
install -m 0755 -- "$packaging/appimage/AppRun" "$appdir/AppRun"
# appimagetool can update the root desktop entry with the release version.
cp -- "$appdir/usr/share/applications/$app_id.desktop" "$appdir/"
ln -s -- "usr/share/icons/hicolor/256x256/apps/$app_id.png" "$appdir/$app_id.png"
ln -s -- "$app_id.png" "$appdir/.DirIcon"

glibc_floor=$(python3 - "$appdir" "$triplet" "$machine" "$slug" "$staged_floor" <<'PY'
from collections import deque
import fnmatch
from pathlib import Path
import re
import shutil
import subprocess
import sys
from urllib.parse import quote

appdir, triplet, machine, slug, staged_floor = sys.argv[1:]
appdir = Path(appdir)
libdir = Path("/usr/lib") / triplet
search = (libdir, Path("/lib") / triplet)
bundle_lib = appdir / "usr/lib"
# Point GIO at an empty bundled module directory rather than the host default.
(bundle_lib / "gio/modules").mkdir(parents=True)
notices = appdir / "usr/share/licenses/airscout-rs/runtime"
notices.mkdir()

# Leave the C runtime and graphics dispatch stack with the host. Loading a
# host GPU driver against bundled copies of these libraries can break it.
host_libraries = (
    "ld-linux*.so*", "libc.so.*", "libm.so.*", "libmvec.so.*",
    "libpthread.so.*", "libdl.so.*", "librt.so.*", "libresolv.so.*",
    "libutil.so.*", "libanl.so.*", "libBrokenLocale.so.*",
    "libnss_*.so*", "libthread_db.so.*", "libgcc_s.so.*", "libstdc++.so.*",
    "libcidn.so.*", "libmemusage.so", "libpcprofile.so", "libSegFault.so",
    "libGL*.so.*", "libEGL*.so.*", "libOpenGL.so.*", "libGLES*.so.*",
    "libgbm.so.*", "libglapi.so.*", "libdrm*.so.*", "libvulkan*.so.*",
    "libwayland-*.so.*", "libX11*.so.*", "libxcb*.so.*", "libxshmfence.so.*",
)

def library(soname):
    for directory in search:
        candidate = directory / soname
        if candidate.is_file():
            return candidate
    sys.exit(f"Target library missing ({triplet}): {soname}")


def bundle(source, destination):
    real = source.resolve(strict=True)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(real, destination)
    # On a merged-/usr system the dpkg file list may use the /lib spelling.
    alternate = str(real).removeprefix("/usr")
    owners = subprocess.run(
        ["dpkg-query", "--search", str(real), alternate],
        capture_output=True, text=True,
    ).stdout.splitlines()
    if not owners:
        sys.exit(f"No distro package owns {real}")
    packages.add(owners[0].split(": ", 1)[0])
    pending.append(destination)


pending = deque(sorted((appdir / "usr/bin").iterdir()))
packages = set()
seen = set()
floor = tuple(map(int, staged_floor.split(".")))

# The tray opens Ayatana through libloading. winit also opens xkbcommon
# dynamically. Include these entry points even when DT_NEEDED omits them.
for soname in ("libayatana-appindicator3.so.1", "libxkbcommon.so.0", "libxkbcommon-x11.so.0"):
    bundle(library(soname), bundle_lib / soname)
    seen.add(soname)

# GTK's input methods and pixbuf loaders are plugins, not ELF link inputs.
for pattern in (
    "gtk-3.0/3.0.0/immodules/*.so",
    "gdk-pixbuf-2.0/2.10.0/loaders/*.so",
    "libgtk-3-0*/gtk-query-immodules-3.0",
    "gdk-pixbuf-2.0/gdk-pixbuf-query-loaders",
):
    sources = sorted(libdir.glob(pattern))
    if not sources:
        sys.exit(f"Missing GTK runtime files: {libdir / pattern}")
    for source in sources:
        destination = (bundle_lib / source.relative_to(libdir) if source.suffix == ".so"
                       else appdir / "usr/libexec" / source.name)
        bundle(source, destination)
if not (bundle_lib / "gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.so").is_file():
    sys.exit("Missing GTK SVG pixbuf loader")

while pending:
    binary = pending.popleft()
    info = subprocess.check_output(["readelf", "-h", "-d", "-V", "-W", binary], text=True)
    if not re.search(rf"^\s*Machine:\s+{re.escape(machine)}$", info, re.M):
        sys.exit(f"Wrong ELF architecture for {slug}: {binary}")
    # Only version requirements contribute to the floor, not symbol exports.
    needs = info.partition("Version needs section")[2]
    for requirement in re.findall(r"Name: GLIBC_([0-9]+(?:\.[0-9]+)+)\s", needs):
        floor = max(floor, tuple(map(int, requirement.split("."))))
    if "Name: GLIBC_ABI_DT_RELR " in needs:
        floor = max(floor, (2, 36))
    for soname in re.findall(r"\(NEEDED\).*?\[([^\]]+)\]", info):
        if soname in seen or any(fnmatch.fnmatchcase(soname, name) for name in host_libraries):
            continue
        if not re.fullmatch(r"[A-Za-z0-9_.+\-]+\.so(?:\.[A-Za-z0-9_.+\-]+)?", soname):
            sys.exit(f"Unexpected ELF dependency name: {soname}")
        bundle(library(soname), bundle_lib / soname)
        seen.add(soname)

if slug in {"linux-x86_64", "linux-arm64"} and floor > (2, 35, 0):
    sys.exit(f"Bundled libraries exceed the {slug} glibc 2.35 limit: {floor}")

source_site = ("https://launchpad.net/ubuntu/+source/" if slug in {"linux-x86_64", "linux-arm64"}
               else "https://sources.debian.org/src/")
source_lines = []
for package in sorted(packages):
    fields = "${binary:Package}\t${Version}\t${source:Package}\t${source:Version}"
    name, version, source, source_version = subprocess.check_output(
        ["dpkg-query", "-W", "-f=" + fields, package], text=True,
    ).split("\t")
    copyright_file = Path("/usr/share/doc") / name.split(":")[0] / "copyright"
    shutil.copyfile(copyright_file, notices / (name + ".copyright"))
    for license_name in set(re.findall(r"/usr/share/common-licenses/([\w+-]+(?:\.[\w+-]+)*)",
                                      copyright_file.read_text(errors="replace"))):
        common_license = Path("/usr/share/common-licenses") / license_name
        if common_license.is_file():
            shutil.copyfile(common_license, notices / license_name)
    url = f"{source_site}{source}/{quote(source_version, safe='.+-')}"
    source_lines.append(f"{name} {version} ({source} {source_version})\n  {url}\n")
(notices / "SOURCES.txt").write_text("\n".join(source_lines), encoding="utf-8")
print(".".join(map(str, floor)))
PY
)
printf '%s AppImage requires glibc %s\n' "$slug" "$glibc_floor"

license_dir="$appdir/usr/share/licenses/airscout-rs"
cat > "$license_dir/AppImage.md" <<'DOC'
# Running AirScout-RS as an AppImage

Make the downloaded AppImage executable with `chmod +x` and run it to open
AirScout. Pass `--tool airscout-planeserver`, `--tool airscout-cli`,
`--tool catcheck`, `--tool elevation-tile-gen` or `--tool maptile-gen`
before the tool's own arguments. On hosts without FUSE support, run it with
`--appimage-extract-and-run` before `--tool`.

For desktop integration, run `./<file>.AppImage --appimage-extract` and move
`squashfs-root` to a permanent location. Copy its root desktop file to
`~/.local/share/applications/`. Set `Exec=` to the absolute path to the
extracted `AppRun` and `Icon=` to the absolute path to the root PNG, quoting
the executable path in the desktop file if it contains spaces.

For shell access, save a wrapper named after the command in a directory on
your PATH. Point it to your permanent copy of the AppImage or to its
extracted `AppRun`. For example, an `airscout-cli` wrapper can contain:

```sh
#!/bin/sh
exec "$HOME/Applications/AirScout-RS.AppDir/AppRun" --tool airscout-cli "$@"
```

Make the wrapper executable with `chmod +x`. Bundled libraries can be
replaced under `usr/lib` in the extracted AppDir. See `runtime/SOURCES.txt`
and the associated copyright files for distro library sources and licenses.
DOC

fetch="$packaging/release/fetch-tool.py"
python3 "$fetch" appimagetool "$tool_platform" "$work/appimagetool"
python3 "$fetch" appimage-runtime "$arch" "$work/runtime"
python3 "$fetch" appimage-runtime-license all "$license_dir/runtime/type2-runtime.LICENSE"
printf '\nAppImage Type 2 runtime 20251108\n  https://github.com/AppImage/type2-runtime/tree/20251108\n' \
    >> "$license_dir/runtime/SOURCES.txt"
chmod +x -- "$work/appimagetool"

asset="AirScout-RS-$tag-$slug.AppImage"
mkdir -p -- "$work/output/assets"
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" VERSION="$tag" "$work/appimagetool" \
    --runtime-file "$work/runtime" "$appdir" "$work/output/assets/$asset"
[[ -s "$work/output/assets/$asset" && -x "$work/output/assets/$asset" ]] ||
    fail "appimagetool did not produce $asset"

python3 - "$work/output" "$output" "$slug" "$asset" "$glibc_floor" <<'PY'
import json
import os
from pathlib import Path
import sys

candidate, output, slug, asset, floor = sys.argv[1:]
result = {
    "format": "appimage", "slug": slug, "assets": [asset],
    "signing": "not_applicable", "glibc_floor": floor,
}
(Path(candidate) / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
os.rename(candidate, output)
PY
