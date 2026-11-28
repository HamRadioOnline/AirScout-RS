#!/usr/bin/env bash
#
# Build unpublished OCI candidates for the three PlaneServer container platforms.
#
#   build.sh <stage-root> <output-dir>
#     <stage-root> holds one stage directory per architecture slug
#     (linux-x86_64, linux-arm64, linux-armhf), each carrying bin/airscout-planeserver,
#     README.md, LICENSE, NOTICE and stage.json (as written by release/stage.sh and
#     extracted from the stage-* artifacts). <output-dir> receives the per-platform
#     image tarballs, an OCI image index and a result sidecar; nothing is pushed.
#
# Each platform is built with `docker buildx build` against a digest-pinned base
# image from packaging/tools.lock.json ("container-base"), copying only the
# PlaneServer binary and the legal files and installing the GTK/appindicator/xdo/
# ALSA loader libraries the binary links even when headless.
#
set -euo pipefail
umask 022
export LC_ALL=C

fail() {
    echo "build.sh: $*" >&2
    exit 1
}

PACKAGING_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTAINER_DIR="$PACKAGING_DIR/container"
LOCK="$PACKAGING_DIR/tools.lock.json"
skopeo=$(python3 - "$LOCK" <<'PYTHON'
import json, sys

with open(sys.argv[1], encoding="utf-8") as lock:
    pin = json.load(lock)["container-tools"]["skopeo"]
print(f'{pin["url"]}@{pin["digest"]}')
PYTHON
)

[[ $# == 2 ]] || fail "Usage: build.sh <stage-root> <output-dir>"

stage_root=$1
output=$2
[[ -d "$stage_root" ]] || fail "Stage root is not a directory: $stage_root"
[[ ! -e "$output" && ! -L "$output" ]] || fail "Output already exists: $output"

work=$(mktemp -d)
smoke_container=""
smoke_image=""
cleanup() {
    if [[ -n "$smoke_container" ]]; then
        docker rm -fv "$smoke_container" >/dev/null 2>&1 || true
    fi
    if [[ -n "$smoke_image" ]]; then
        docker image rm "$smoke_image" >/dev/null 2>&1 || true
    fi
    rm -rf -- "$work"
}
trap cleanup EXIT
mkdir -p "$work/images"

version=""

# Each of the three platforms. The mapping is fixed: one stage slug per OCI
# platform, its Rust target, the arch token for the tarball, and the runtime
# library packages for that base image.
for slug in linux-x86_64 linux-arm64 linux-armhf; do
    case "$slug" in
        linux-x86_64)
            platform=linux/amd64; arch=amd64; target=x86_64-unknown-linux-gnu
            runtime_packages="libgtk-3-0 libasound2 libayatana-appindicator3-1 libxdo3 libx11-6 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0"
            ;;
        linux-arm64)
            platform=linux/arm64; arch=arm64; target=aarch64-unknown-linux-gnu
            runtime_packages="libgtk-3-0 libasound2 libayatana-appindicator3-1 libxdo3 libx11-6 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0"
            ;;
        linux-armhf)
            platform=linux/arm/v7; arch=armv7; target=armv7-unknown-linux-gnueabihf
            runtime_packages="libgtk-3-0 libasound2 libayatana-appindicator3-1 libxdo3 libx11-6 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0"
            ;;
    esac

    # The digest-pinned base image and its platform come from tools.lock.json.
    base_pin=$(python3 - "$LOCK" "$slug" <<'PY'
import json, sys
pin = json.load(open(sys.argv[1], encoding="utf-8"))["container-base"][sys.argv[2]]
print(pin["url"], pin["digest"], pin["platform"], pin["glibc"])
PY
)
    read -r base_url base_digest locked_platform base_glibc <<< "$base_pin"
    [[ "$locked_platform" == "$platform" ]] ||
        fail "tools.lock.json platform for $slug is $locked_platform, expected $platform"
    base="${base_url}@${base_digest}"

    stage="$stage_root/$slug"
    [[ -d "$stage" ]] || fail "Missing stage directory: $stage"

    stage_metadata=$(python3 - "$stage/stage.json" "$base_glibc" <<'PY'
import json, re, sys
from pathlib import Path

try:
    data = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    for key, pattern in {"target": r"[a-z0-9_-]+", "slug": r"linux-[a-z0-9_]+",
                         "version": r"[0-9]+\.[0-9]+\.[0-9]+"}.items():
        if key not in data or not re.fullmatch(pattern, data[key]):
            raise ValueError("invalid %s" % key)

    def glibc(value):
        if not re.fullmatch(r"[0-9]+\.[0-9]+(?:\.[0-9]+)?", value):
            raise ValueError("invalid glibc version")
        return tuple(map(int, value.split("."))) + (0,) * (2 - value.count("."))

    floor = data["glibc_floor"]
    if glibc(floor) > glibc(sys.argv[2]):
        raise ValueError(f"requires glibc {floor}, base only provides {sys.argv[2]}")
    print(data["target"], data["slug"], data["version"])
except (KeyError, OSError, TypeError, ValueError) as error:
    sys.exit("build.sh: invalid stage.json in %s: %s" % (sys.argv[1], error))
PY
)
    read -r stage_target stage_slug staged_version <<< "$stage_metadata"
    [[ "$stage_slug" == "$slug" ]] || fail "Stage slug $stage_slug does not match directory $slug"
    [[ "$stage_target" == "$target" ]] || fail "Stage target $stage_target is not $target for $slug"
    if [[ -z "$version" ]]; then
        version=$staged_version
    elif [[ "$staged_version" != "$version" ]]; then
        fail "Stage versions disagree: $version vs $staged_version"
    fi
    [[ -f "$stage/bin/airscout-planeserver" && -x "$stage/bin/airscout-planeserver" ]] ||
        fail "Missing executable: $stage/bin/airscout-planeserver"
    for name in README.md LICENSE NOTICE; do
        [[ -f "$stage/$name" ]] || fail "Missing $name: $stage/$name"
    done

    tarfile="$work/images/$arch.tar"

    echo "==> Building $platform image from $stage (base ${base_url}@${base_digest:0:15}...)"
    if ! docker buildx build \
        --platform "$platform" \
        --provenance=false \
        --build-arg "BASE_IMAGE=$base" \
        --build-arg "RUNTIME_PACKAGES=$runtime_packages" \
        --output "type=oci,oci-mediatypes=true,dest=$tarfile" \
        -f "$CONTAINER_DIR/Dockerfile" \
        "$stage" > "$work/tool.log" 2>&1; then
        echo "build.sh: docker buildx build failed for $slug" >&2
        echo "--- tool log ---" >&2
        cat "$work/tool.log" >&2
        exit 1
    fi

    descriptor=$(python3 "$CONTAINER_DIR/oci_archive.py" oci "$tarfile" "$platform")
    IFS=$'\t' read -r digest media_type size config_digest <<< "$descriptor"

    # Convert a copy for Docker load and verify its config digest, which
    # also commits to the uncompressed layer diff IDs. Keep the original OCI
    # archive for distribution.
    if [[ "$arch" == amd64 ]]; then
        docker pull "$skopeo"
    fi
    smoke_image="docker.io/library/airscout-planeserver-smoke:${work##*/}-$arch"
    docker run --rm --pull never --network none --user "$(id -u):$(id -g)" \
        --volume "$work:/work" --entrypoint /usr/bin/skopeo "$skopeo" \
        copy "oci-archive:/work/images/$arch.tar" "docker-archive:/work/smoke.tar:$smoke_image"
    smoke_config=$(python3 "$CONTAINER_DIR/oci_archive.py" docker "$work/smoke.tar" "$smoke_image")
    [[ "$smoke_config" == "$config_digest" ]] ||
        fail "Docker smoke archive does not match the OCI config for $platform"
    docker load --input "$work/smoke.tar"
    rm -- "$work/smoke.tar"

    echo "==> Running offline smoke test for $platform"
    smoke_container=$(docker run --detach --pull never --network none --platform "$platform" \
        --mount type=volume,target=/config --mount type=volume,target=/data \
        "$smoke_image" --run-for 15)
    if ! "$CONTAINER_DIR/../tests/hosted/verify-container.sh" probe "$smoke_container"; then
        docker logs "$smoke_container" >&2
        fail "Offline runtime checks failed for $platform"
    fi
    if [[ "$(docker wait "$smoke_container")" != 0 ]]; then
        docker logs "$smoke_container" >&2
        fail "Headless PlaneServer exited unsuccessfully for $platform"
    fi
    docker rm -v "$smoke_container" >/dev/null
    smoke_container=""
    docker image rm "$smoke_image" >/dev/null
    smoke_image=""

    printf '%s\t%s\t%s\t%s\t%s\n' "$arch" "$platform" "$digest" "$media_type" "$size" \
        >> "$work/descriptors.tsv"
done

# Assemble the OCI image index and the result sidecar from the per-platform
# descriptors collected above.
python3 - "$work/descriptors.tsv" "$work/index.json" "$work/result.json" "$version" <<'PY'
import json, sys

descriptors_path, index_path, result_path, version = sys.argv[1:]
manifests = []
images = {}
platforms = []
for line in open(descriptors_path, encoding="utf-8"):
    arch, platform, digest, media_type, size = line.rstrip("\n").split("\t")
    os_, _, arch_variant = platform.partition("/")
    platform_obj = {"os": os_, "architecture": arch_variant}
    if "/" in arch_variant:
        architecture, _, variant = arch_variant.partition("/")
        platform_obj = {"os": os_, "architecture": architecture, "variant": variant}
    manifests.append({
        "mediaType": media_type,
        "digest": digest,
        "size": int(size),
        "platform": platform_obj,
    })
    images["linux-" + arch] = {"platform": platform, "digest": digest,
                               "tarball": "images/%s.tar" % arch}
    platforms.append(platform)

index = {
    "schemaVersion": 2,
    "mediaType": "application/vnd.oci.image.index.v1+json",
    "manifests": manifests,
}
json.dump(index, open(index_path, "w", encoding="utf-8"), indent=2)
open(index_path, "a", encoding="utf-8").write("\n")

result = {
    "format": "container",
    "version": version,
    "platforms": sorted(platforms),
    "images": images,
    "assets": ["index.json"],
    "signing": "not_applicable",
}
json.dump(result, open(result_path, "w", encoding="utf-8"), indent=2)
open(result_path, "a", encoding="utf-8").write("\n")
PY

# Move the assembled candidates (tarballs + index + sidecar) into place.
mkdir -p -- "$output"
mv -- "$work/images" "$work/index.json" "$work/result.json" "$output"/

echo "==> Built OCI candidates at $output"
