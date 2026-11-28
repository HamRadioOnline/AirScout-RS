#!/usr/bin/env bash
#
# Build one architecture's Flatpak from a staged release.
#
#   stage.sh <stage-dir> <ostree-branch> <output-dir>
#     Assembles the /app tree (six binaries, desktop entry, hicolor icons,
#     AppStream metadata and license files) next to a copy of the manifest so
#     its `type: dir` source path `stage` resolves to this build, then runs
#     flatpak-builder for the stage's architecture. Writes <output-dir>/build
#     (the flatpak-builder build directory) and <output-dir>/result.json.
#
# The OSTree branch (<stable|beta>) is recorded in the result sidecar and used
# by prepare-repo.sh when the per-arch build is exported into the shared repo.
#
set -euo pipefail
# Shipped build contents are world-readable; there is no secret material here.
umask 022
export LC_ALL=C

fail() {
    echo "stage.sh: $*" >&2
    exit 1
}

PACKAGING_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLATPAK_DIR="$PACKAGING_DIR/flatpak"
MANIFEST="$FLATPAK_DIR/io.github.hamradioonline.AirScoutRS.yml"
APP_ID="io.github.hamradioonline.AirScoutRS"
PROGRAMS=(airscout airscout-planeserver airscout-cli catcheck elevation-tile-gen maptile-gen)
ICON_SIZES=(16 32 48 64 128 256)

run_checked() {
    local log=$1
    shift
    if ! "$@" > "$log" 2>&1; then
        printf 'stage.sh: command failed: %s\n' "$*" >&2
        echo "--- tool log ---" >&2
        cat "$log" >&2
        exit 1
    fi
}

[[ $# == 3 ]] || fail "Usage: stage.sh <stage-dir> <ostree-branch> <output-dir>"

stage=$(cd "$1" && pwd)
branch=$2
output=$3

[[ -f "$stage/stage.json" ]] || fail "Not a stage directory (missing stage.json): $stage"
case "$branch" in
    # Alpha is allowed here so CI can smoke-test an unsigned local Flatpak
    # build; prepare-repo.sh and publish-repo.sh still refuse to publish it.
    stable|beta|alpha) ;;
    *) fail "Unexpected OSTree branch: $branch" ;;
esac

stage_fields=$(python3 - "$stage/stage.json" <<'PY'
import json
from pathlib import Path
import re
import sys

try:
    stage = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    for key, pattern in {
        "target": r"[a-z0-9_-]+",
        "slug": r"linux-[a-z0-9_]+",
        "version": r"[0-9]+\.[0-9]+\.[0-9]+",
    }.items():
        if key not in stage or not re.fullmatch(pattern, stage[key]):
            raise ValueError(f"invalid {key}")
    print(*(stage[key] for key in ("target", "slug", "version")))
except (KeyError, OSError, TypeError, ValueError) as error:
    sys.exit(f"stage.sh: invalid stage.json: {error}")
PY
)
read -r target slug staged_version <<< "$stage_fields"

# Flatpak runtimes have no 32-bit support, so only the two 64-bit Linux targets
# are eligible. The armhf/i686 stages are rejected here, never silently dropped.
case "$target:$slug" in
    x86_64-unknown-linux-gnu:linux-x86_64) arch=x86_64 ;;
    aarch64-unknown-linux-gnu:linux-arm64) arch=aarch64 ;;
    *) fail "Unexpected stage target and slug: $target $slug" ;;
esac

for program in "${PROGRAMS[@]}"; do
    [[ -f "$stage/bin/$program" && -x "$stage/bin/$program" ]] ||
        fail "Missing executable: $stage/bin/$program"
done
for name in README.md LICENSE NOTICE; do
    [[ -f "$stage/$name" ]] || fail "Missing $name: $stage/$name"
done

[[ ! -e "$output" && ! -L "$output" ]] || fail "Output already exists: $output"
mkdir -p -- "$(dirname -- "$output")"
output=$(cd "$(dirname -- "$output")" && pwd)/$(basename -- "$output")

work=$(mktemp -d "$(dirname -- "$output")/.airscout-flatpak.XXXXXX")
trap 'rm -rf -- "$work"' EXIT

echo "==> Building $slug Flatpak for branch $branch (arch $arch)"

# Assemble the /app-relative tree that the manifest's `type: dir` source (path:
# stage) installs. The stage already carries bin/ and the license files; add the
# shared desktop entry, AppStream metadata and hicolor icons.
approot="$work/stage"
cp -a -- "$stage" "$approot"
mkdir -p "$approot/share/applications" "$approot/share/metainfo" "$approot/share/icons/hicolor"
cp -p -- "$PACKAGING_DIR/linux/$APP_ID.desktop" "$approot/share/applications/$APP_ID.desktop"
cp -p -- "$PACKAGING_DIR/linux/$APP_ID.metainfo.xml" "$approot/share/metainfo/$APP_ID.metainfo.xml"
for size in "${ICON_SIZES[@]}"; do
    icon_dir="$approot/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p -- "$icon_dir"
    cp -p -- "$PACKAGING_DIR/icons/hicolor/$size.png" "$icon_dir/$APP_ID.png"
done
# The corresponding-source notice for the bundled third-party libraries is
# installed by the airscout-rs module next to README/LICENSE/NOTICE.
cp -p -- "$FLATPAK_DIR/third-party-notices.txt" "$approot/third-party-notices.txt"
chmod 644 "$approot/share/applications/$APP_ID.desktop" \
    "$approot/share/metainfo/$APP_ID.metainfo.xml"
find "$approot/share/icons" -type f -exec chmod 644 {} +

# flatpak-builder resolves the `type: dir` path relative to the manifest's own
# directory, so keep a copy of the manifest next to the assembled stage.
cp -- "$MANIFEST" "$work/manifest.yml"
# Module `type: patch` sources resolve relative to the manifest as well.
cp -R -- "$FLATPAK_DIR/patches" "$work/patches"
run_checked "$work/tool.log" flatpak-builder --force-clean --user \
    --default-branch="$branch" --arch="$arch" \
    "$work/build" "$work/manifest.yml"
[[ -d "$work/build" ]] || fail "flatpak-builder produced no build directory"

mkdir -p -- "$work/output"
mv -- "$work/build" "$work/output/build"
python3 - "$work/output" "$output" "$slug" "$branch" <<'PY'
import json
import os
import sys
from pathlib import Path

candidate, output, slug, branch = sys.argv[1:]
result = {
    "format": "flatpak",
    "slug": slug,
    "branch": branch,
    "assets": [],
    "signing": "not_applicable",
}
(Path(candidate) / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
os.rename(candidate, output)
PY

echo "==> Built $output/build for $slug ($branch)"
