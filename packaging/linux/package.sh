#!/usr/bin/env bash
set -euo pipefail

fail() {
    echo "package.sh: $*" >&2
    exit 1
}

if [[ $# != 4 ]]; then
    echo 'Usage: package.sh <stage-dir> <tag> <format> <output-dir>' >&2
    exit 2
fi

stage=$(cd "$1" && pwd)
tag=$2
format=$3
linux_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
output=$4
[[ ! -e "$output" && ! -L "$output" ]] || fail "Output already exists: $output"
mkdir -p -- "$(dirname -- "$output")"
output=$(cd "$(dirname -- "$output")" && pwd)/$(basename -- "$output")

case "$format" in
    deb) extension=deb; unit_dir=/lib/systemd/system ;;
    rpm) extension=rpm; unit_dir=/usr/lib/systemd/system ;;
    archlinux) extension=pkg.tar.zst; unit_dir=/usr/lib/systemd/system ;;
    *) fail "Unknown package format: $format" ;;
esac

work=$(mktemp -d "$(dirname -- "$output")/.airscout-linux.XXXXXX")
trap 'rm -rf -- "$work"' EXIT

GITHUB_OUTPUT="$work/tag" "$linux_dir/../release/check-tag.sh" "$tag"
base_version=$(sed -n 's/^version=//p' "$work/tag")
channel=$(sed -n 's/^channel=//p' "$work/tag")
version=$base_version
release=1
if [[ "$format" == deb ]]; then
    release=
fi
if [[ "$channel" != stable ]]; then
    suffix=${tag#"$channel-v$version-"}
    case "$format" in
        deb) version+="~$channel.$suffix" ;;
        rpm) release="0.$channel.$suffix" ;;
        archlinux) version+="$channel.$suffix" ;;
    esac
fi

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
        "glibc_floor": r"[0-9]+\.[0-9]+(?:\.[0-9]+)?",
    }.items():
        if not re.fullmatch(pattern, stage[key]):
            raise ValueError(f"invalid {key}")
    print(*(stage[key] for key in ("target", "slug", "version", "glibc_floor")))
except (KeyError, OSError, TypeError, ValueError) as error:
    sys.exit(f"package.sh: invalid stage.json: {error}")
PY
)
read -r target slug staged_version glibc_floor <<< "$stage_fields"
[[ "$staged_version" == "$base_version" ]] ||
    fail "Stage version $staged_version differs from tag $tag"

case "$target:$slug" in
    x86_64-unknown-linux-gnu:linux-x86_64) arch=amd64 ;;
    aarch64-unknown-linux-gnu:linux-arm64) arch=arm64 ;;
    armv7-unknown-linux-gnueabihf:linux-armhf) arch=arm7 ;;
    i686-unknown-linux-gnu:linux-i686) arch=386 ;;
    *) fail "Unexpected stage target and slug: $target $slug" ;;
esac
if [[ "$format" == rpm && "$arch" != amd64 && "$arch" != arm64 ]]; then
    fail "RPM is unavailable for $slug"
fi

"$linux_dir/install-root.sh" "$stage" "$work/root"
cp -R -- "$linux_dir/scripts" "$work/scripts"
if [[ "$format" == archlinux ]]; then
    # nFPM wraps these bodies in pacman's lifecycle functions. makepkg uses
    # the same source file directly as the complete install script.
    for hook in post_install post_upgrade pre_remove; do
        sed -n "/^${hook}() {$/,/^}$/ { /^${hook}() {$/d; /^}$/d; p; }" \
            "$work/scripts/arch-install" > "$work/scripts/arch-${hook//_/-}"
        [[ -s "$work/scripts/arch-${hook//_/-}" ]] || fail "Missing Arch hook: $hook"
    done
fi

asset="AirScout-RS-$tag-$slug.$extension"
mkdir -p -- "$work/output/assets"
(
    cd "$work"
    export PKG_ARCH="$arch" PKG_VERSION="$version" PKG_RELEASE="$release"
    export PKG_UNIT_DIR="$unit_dir" PKG_GLIBC_FLOOR="$glibc_floor"
    nfpm package --config "$linux_dir/nfpm.yaml" --packager "$format" --target "output/assets/$asset"
)
[[ -s "$work/output/assets/$asset" ]] || fail "nFPM did not produce $asset"

python3 - "$work/output" "$output" "$format" "$slug" "$asset" "$glibc_floor" <<'PY'
import json
import os
from pathlib import Path
import sys

work_output, output, fmt, slug, asset, floor = sys.argv[1:]
result = {
    "format": fmt, "slug": slug, "assets": [asset],
    "signing": "not_applicable", "glibc_floor": floor,
}
(Path(work_output) / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
os.rename(work_output, output)
PY
