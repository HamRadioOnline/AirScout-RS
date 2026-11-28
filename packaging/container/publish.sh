#!/usr/bin/env bash
# Publish the original OCI manifests by digest and promote their OCI index.
# A beta requires its full beta-vX.Y.Z-<suffix> tag for prerelease precedence.
set -euo pipefail
umask 022
export LC_ALL=C

fail() {
    echo "publish.sh: $*" >&2
    exit 1
}

PACKAGING_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="ghcr.io/hamradioonline/airscout-planeserver"
SOURCE_URL="https://github.com/HamRadioOnline/AirScout-RS"
ORG_IDENTITY="HamRadioOnline"

SECRET_NAMES=(GITHUB_TOKEN)

# Replace every configured secret value on stdin with [redacted], longest first.
# Secret names travel on argv; the values are read from the environment so they
# never reach a process list.
redact() {
    python3 -c '
import os, sys

needles = []
for name in sys.argv[1:]:
    value = os.environ.get(name, "")
    if value and value not in needles:
        needles.append(value)
    for line in value.splitlines():
        if line and line not in needles:
            needles.append(line)
needles.sort(key=len, reverse=True)

text = sys.stdin.read()
for needle in needles:
    text = text.replace(needle, "[redacted]")
sys.stdout.write(text)
' "${SECRET_NAMES[@]}"
}

# Filter both tool streams before writing the log. Print the log on failure.
run_checked() {
    local log=$1
    shift
    if ! "$@" 2>&1 | redact >> "$log"; then
        printf 'publish.sh: command failed: %s\n' "$*" | redact >&2
        echo "--- redacted tool log ---" >&2
        cat "$log" >&2
        exit 1
    fi
}

[[ $# == 3 || $# == 4 ]] ||
    fail "Usage: publish.sh <oci-dir> <stable|beta> <version> [release-tag]"

oci_dir=$(cd -- "$1" && pwd)
channel=$2
version=$3
release_tag=${4:-${TAG:-v$version}}
case "$channel" in
    stable) mutable_tag=latest; annotation=$version ;;
    beta)   mutable_tag=beta; annotation=$release_tag ;;
    *) fail "Unsupported channel '$channel': only stable and beta are eligible" ;;
esac

work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

# Validate the candidate and prepare the descriptors that Buildx will resolve.
python3 - "$oci_dir" "$channel" "$version" "$release_tag" "$work" <<'PY'
import json, re, sys
from pathlib import Path

candidate, channel, version, release_tag, work = sys.argv[1:]
candidate, work = Path(candidate), Path(work)
core = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
if not re.fullmatch(core, version):
    sys.exit("publish.sh: invalid workspace version")
if channel == "stable":
    valid_tag = release_tag == f"v{version}"
else:
    match = re.fullmatch(rf"beta-v{re.escape(version)}-([A-Za-z0-9]+(?:\.[A-Za-z0-9]+)*)",
                         release_tag)
    valid_tag = match and all(not (part.isdigit() and len(part) > 1 and part.startswith("0"))
                              for part in match[1].split("."))
if not valid_tag:
    sys.exit("publish.sh: invalid release tag for candidate version and channel")

result = json.loads((candidate / "result.json").read_text(encoding="utf-8"))
if result["version"] != version:
    sys.exit("publish.sh: requested version does not match the candidate's version")
index = json.loads((candidate / "index.json").read_text(encoding="utf-8"))
if index["schemaVersion"] != 2 or index["mediaType"] != "application/vnd.oci.image.index.v1+json":
    sys.exit("publish.sh: invalid candidate OCI index")
platforms = {"amd64": {"os": "linux", "architecture": "amd64"},
             "arm64": {"os": "linux", "architecture": "arm64"},
             "armv7": {"os": "linux", "architecture": "arm", "variant": "v7"}}
if len(index["manifests"]) != len(platforms):
    sys.exit("publish.sh: candidate must have exactly three platform manifests")
with (work / "images.tsv").open("w", encoding="utf-8") as images:
    for arch, platform in platforms.items():
        descriptors = [m for m in index["manifests"] if m["platform"] == platform]
        if len(descriptors) != 1:
            sys.exit(f"publish.sh: missing or duplicate candidate manifest for {arch}")
        descriptor = descriptors[0]
        digest, media_type, size = (descriptor[k] for k in ("digest", "mediaType", "size"))
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
            sys.exit(f"publish.sh: invalid manifest digest for {arch}")
        # Buildx infers the source repository from --tag. Omitting mediaType
        # makes it resolve the source by digest and fill in the registry type.
        source = {"digest": digest, "size": size, "platform": platform}
        (work / f"{arch}.json").write_text(json.dumps(source), encoding="utf-8")
        print(arch, "/".join(platform.values()), digest, media_type, size,
              sep="\t", file=images)
PY

sources=()
while IFS=$'\t' read -r arch platform digest media_type size; do
    descriptor=$(python3 "$PACKAGING_DIR/container/oci_archive.py" oci \
        "$oci_dir/images/$arch.tar" "$platform")
    IFS=$'\t' read -r archive_digest archive_type archive_size _ <<< "$descriptor"
    [[ "$archive_digest" == "$digest" && "$archive_type" == "$media_type" &&
       "$archive_size" == "$size" ]] || fail "OCI archive differs from the candidate index for $platform"
    sources+=(--file "$work/$arch.json")
done < "$work/images.tsv"

skopeo_image=$(python3 - "$PACKAGING_DIR/tools.lock.json" <<'PY'
import json, sys

with open(sys.argv[1], encoding="utf-8") as lock:
    pin = json.load(lock)["container-tools"]["skopeo"]
print(f'{pin["url"]}@{pin["digest"]}')
PY
)
run_checked "$work/tool.log" docker pull "$skopeo_image"
skopeo_user="$(id -u):$(id -g)"

# The release job uses a fresh runner. Isolate its temporary registry auth
# from both the candidate artifact and the runner's default Docker config.
# Skopeo writes config.json with mode 0600, so Buildx must share its owner.
if [[ -n ${GITHUB_TOKEN:-} ]]; then
    docker_config=${DOCKER_CONFIG:-$HOME/.docker}
    export DOCKER_CONFIG="$work/docker"
    mkdir -m 700 "$DOCKER_CONFIG"
    if [[ -d "$docker_config/cli-plugins" ]]; then
        ln -s "$docker_config/cli-plugins" "$DOCKER_CONFIG/cli-plugins"
    fi
    run_checked "$work/tool.log" docker run --rm -i --pull never --user "$skopeo_user" \
        --volume "$work:/work" --entrypoint /usr/bin/skopeo "$skopeo_image" \
        login --compat-auth-file /work/docker/config.json \
        --username "${GITHUB_ACTOR:-$ORG_IDENTITY}" --password-stdin ghcr.io <<< "$GITHUB_TOKEN"
    export REGISTRY_AUTH_FILE=/work/docker/config.json
fi

skopeo() {
    docker run --rm --pull never --user "$skopeo_user" \
        --env REGISTRY_AUTH_FILE \
        --volume "$oci_dir:/candidate:ro" --volume "$work:/work:ro" \
        --entrypoint /usr/bin/skopeo "$skopeo_image" "$@"
}

inspect_tag() {
    local tag=$1 error
    if error=$(skopeo inspect --raw "docker://$REPO:$tag" 2>&1 > "$work/$tag.raw"); then
        return 0
    fi
    # Skopeo wraps registry errors with the manifest operation. A registry
    # manifest-unknown response (or an explicit 404 for that read) is absence.
    if grep -Eqi 'reading manifest .*: (manifest unknown([:][^"]*)?|statuscode: 404([[:space:]][^"]*)?|error parsing HTTP 404 response body:.*)"?$' \
        <<< "$error"; then
        return 1
    fi
    echo "publish.sh: unable to verify tag $REPO:$tag" >&2
    redact <<< "$error" >&2
    exit 1
}

promotion() {
    python3 - "$work/$1.raw" "$oci_dir/index.json" "$annotation" "$2" <<'PY'
import json, re, sys

remote, candidate, annotation, operation = sys.argv[1:]
version_key = "org.opencontainers.image.version"


def read(path):
    with open(path, encoding="utf-8") as file:
        return json.load(file)


def precedence(value):
    if not isinstance(value, str):
        raise ValueError("missing or invalid version annotation")
    core = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    pattern = (rf"beta-v{core}-([A-Za-z0-9]+(?:\.[A-Za-z0-9]+)*)"
               if annotation.startswith("beta-v") else core)
    match = re.fullmatch(pattern, value)
    if not match:
        raise ValueError("invalid version annotation")
    version = tuple(map(int, match.groups()[:3]))
    if len(match.groups()) == 3:
        return version, ()
    # Numeric identifiers sort before alphanumeric ones, with an equal
    # prefix and fewer identifiers taking lower precedence.
    parts = match[4].split(".")
    if any(part.isdigit() and len(part) > 1 and part.startswith("0") for part in parts):
        raise ValueError("invalid numeric prerelease identifier")
    suffix = tuple((0, int(part)) if part.isdigit() else (1, part) for part in parts)
    return version, suffix


try:
    existing, expected = read(remote), read(candidate)
    if existing["schemaVersion"] != 2 or existing["mediaType"] != expected["mediaType"]:
        raise ValueError("expected a versioned OCI image index")
    old = existing.get("annotations", {}).get(version_key)
    old_version, new_version = precedence(old), precedence(annotation)

    def descriptors(index):
        return sorted(json.dumps({k: m[k] for k in ("digest", "platform", "mediaType", "size")},
                                 sort_keys=True) for m in index["manifests"])

    same = descriptors(existing) == descriptors(expected) and old == annotation
    if operation == "immutable" and not same:
        raise ValueError("immutable tag differs from this candidate")
    print("skip" if same or old_version > new_version else "promote")
except (AttributeError, KeyError, OSError, TypeError, ValueError) as error:
    sys.exit(f"publish.sh: cannot verify {operation} tag: {error}")
PY
}

tags=()
if [[ "$channel" == stable ]]; then
    if inspect_tag "$version"; then
        promotion "$version" immutable > /dev/null
    else
        tags+=("$version")
    fi
fi
if inspect_tag "$mutable_tag"; then
    decision=$(promotion "$mutable_tag" mutable)
    if [[ "$decision" == promote ]]; then
        tags+=("$mutable_tag")
    fi
else
    tags+=("$mutable_tag")
fi

if [[ ${#tags[@]} -gt 0 ]]; then
    while IFS=$'\t' read -r arch platform digest media_type size; do
        run_checked "$work/tool.log" skopeo copy --preserve-digests \
            "oci-archive:/candidate/images/$arch.tar" "docker://$REPO@$digest"
    done < "$work/images.tsv"

    create_args=()
    for tag in "${tags[@]}"; do
        create_args+=(--tag "$REPO:$tag")
    done
    run_checked "$work/tool.log" docker buildx imagetools create "${create_args[@]}" \
        --annotation "index:org.opencontainers.image.version=$annotation" \
        --annotation "index:org.opencontainers.image.source=$SOURCE_URL" \
        --annotation "index:org.opencontainers.image.vendor=$ORG_IDENTITY" "${sources[@]}"
fi

# Keep the status alongside the immutable candidate artifact.
python3 - "$oci_dir" ${tags[@]+"${tags[@]}"} <<'PY'
import json
import sys
from pathlib import Path

oci_dir, *tags = sys.argv[1:]
result = {"format": "container", "published": True, "tags": sorted(tags)}
Path(oci_dir + ".result.json").write_text(
    json.dumps(result, indent=2) + "\n", encoding="utf-8"
)
PY

echo "==> Processed $REPO release $release_tag; tags promoted: ${tags[*]:-none}"
