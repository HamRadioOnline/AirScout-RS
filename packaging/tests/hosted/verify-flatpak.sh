#!/usr/bin/env bash
set -euo pipefail

hosted=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
lock="$hosted/../../tools.lock.json"
app=io.github.hamradioonline.AirScoutRS
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

run_ref() {
    python3 "$hosted/smoke-stage.py" /app/bin "$1" --flatpak-ref "$2"
}

public_key() {
    python3 - "$1/airscout-rs.flatpakrepo" <<'PY'
import base64
import configparser
import sys

descriptor = configparser.ConfigParser(interpolation=None)
with open(sys.argv[1], encoding="utf-8") as source:
    descriptor.read_file(source)
sys.stdout.buffer.write(base64.b64decode(descriptor["Flatpak Repo"]["GPGKey"], validate=True))
PY
}

install_pinned_runtime() {
    local arch=$1 pins tool_url tool_sha runtime_commit flathub_sha runtime
    pins=$(python3 - "$lock" "$arch" <<'PY'
import json
import sys

lock = json.load(open(sys.argv[1], encoding="utf-8"))
arch = sys.argv[2]
tool = lock["flatpak-tooling"][arch]["flatpak"]
print(tool["url"], tool["sha256"], lock["flatpak-runtime"]["all"]["commits"][arch]["platform"],
      lock["flathub-flatpakrepo"]["all"]["sha256"])
PY
)
    read -r tool_url tool_sha runtime_commit flathub_sha <<< "$pins"
    curl -fsSL --max-time 60 "$tool_url" -o "$work/flatpak.deb"
    printf '%s  %s\n' "$tool_sha" "$work/flatpak.deb" | sha256sum -c -
    sudo apt-get update -qq
    sudo apt-get install -y --no-install-recommends "$work/flatpak.deb" ostree
    export FLATPAK_USER_DIR="$work/user"

    runtime="org.freedesktop.Platform/$arch/25.08"
    curl -fsSL --max-time 60 https://flathub.org/repo/flathub.flatpakrepo -o "$work/flathub.flatpakrepo"
    printf '%s  %s\n' "$flathub_sha" "$work/flathub.flatpakrepo" | sha256sum -c -
    flatpak --user remote-add flathub "$work/flathub.flatpakrepo"
    flatpak --user install -y --noninteractive flathub "$runtime"
    flatpak --user update -y --noninteractive --commit="$runtime_commit" "$runtime"
    [[ $(flatpak --user info --show-commit "$runtime") == "$runtime_commit" ]]
}

if [[ ${1:-} == summary && $# == 2 ]]; then
    repo=$(realpath "$2")
    [[ -s "$repo/summary.sig" ]]
    export FLATPAK_USER_DIR="$work/user"
    public_key "$repo" > "$work/key.gpg"
    flatpak --user remote-add --gpg-import="$work/key.gpg" airscout "file://$repo"
    flatpak --user remote-ls --columns=ref airscout
    exit
fi

if [[ ${1:-} == candidate && $# == 5 ]]; then
    build=$(realpath "$2/build")
    arch=$3
    channel=$4
    version=$5
    ref="app/$app/$arch/$channel"
    flatpak build-export --arch="$arch" "$work/repo" "$build" "$channel"
    flatpak --user remote-add --no-gpg-verify airscout-candidate "file://$work/repo"
    flatpak --user install -y --noninteractive --no-deps airscout-candidate "$ref"
    run_ref "$version" "$ref"
    exit
fi

if [[ ${1:-} == signed-candidate && $# == 5 ]]; then
    repo=$(realpath "$2")
    arch=$3
    channel=$4
    version=$5
    ref="app/$app/$arch/$channel"
    [[ -s "$repo/summary.sig" ]]
    public_key "$repo" > "$work/key.gpg"
    install_pinned_runtime "$arch"
    flatpak --user remote-add --gpg-import="$work/key.gpg" airscout-candidate "file://$repo"
    flatpak --user remote-ls --columns=ref airscout-candidate > "$work/candidate-refs"
    grep -Fx "$ref" "$work/candidate-refs"
    flatpak --user install -y --noninteractive --no-deps airscout-candidate "$ref"
    run_ref "$version" "$ref"
    exit
fi

[[ ${1:-} == published && $# == 7 ]] || {
    echo 'Usage: verify-flatpak.sh summary <repo> | candidate <build> <arch> <channel> <version> | signed-candidate <repo> <arch> <channel> <version> | published <repo> <old-repo> <url> <arch> <channel> <version>' >&2
    exit 2
}
repo=$(realpath "$2")
old_repo=$(realpath "$3")
url=${4%/}
arch=$5
channel=$6
version=$7
ref="app/$app/$arch/$channel"
other=stable
[[ "$channel" == beta ]] || other=beta
other_ref="app/$app/$arch/$other"

install_pinned_runtime "$arch"

# Require Pages to serve the candidate's exact signed summary and descriptors.
# Pages can take a short time to serve the deployment at every edge.
for file in summary summary.sig airscout-rs.flatpakrepo; do
    for attempt in {1..12}; do
        if curl -fsSL --max-time 15 "$url/$file" -o "$work/$file" &&
            cmp -s "$repo/$file" "$work/$file"; then
            break
        fi
        [[ "$attempt" != 12 ]] || { echo "Pages did not serve $file" >&2; exit 1; }
        sleep 5
    done
done


if [[ -s "$old_repo/summary" ]]; then
    public_key "$old_repo" > "$work/old.gpg"
    flatpak --user remote-add --gpg-import="$work/old.gpg" airscout "file://$old_repo"
    flatpak --user remote-ls --columns=ref airscout > "$work/old-refs"
    for previous in "$ref" "$other_ref"; do
        if grep -Fqx "$previous" "$work/old-refs"; then
            flatpak --user install -y --noninteractive --no-deps airscout "$previous"
        fi
    done
else
    touch "$work/old-refs"
fi

public_key "$repo" > "$work/published.gpg"
if [[ -s "$old_repo/summary" ]]; then
    flatpak --user remote-modify --url="$url" --gpg-import="$work/published.gpg" airscout
else
    flatpak --user remote-add --gpg-import="$work/published.gpg" airscout "$url"
fi
flatpak --user remote-ls --columns=ref airscout > "$work/published-refs"
grep -Fx "$ref" "$work/published-refs"
if grep -Fqx "$ref" "$work/old-refs"; then
    flatpak --user update -y --noninteractive --no-deps "$ref"
else
    flatpak --user install -y --noninteractive --no-deps airscout "$ref"
fi
[[ $(flatpak --user info --show-commit "$ref") == "$(ostree --repo="$repo" rev-parse "$ref")" ]]
run_ref "$version" "$ref"

if grep -Fqx "$other_ref" "$work/old-refs"; then
    grep -Fx "$other_ref" "$work/published-refs"
    expected=$(ostree --repo="$old_repo" rev-parse "$other_ref")
    [[ $(ostree --repo="$repo" rev-parse "$other_ref") == "$expected" ]]
    [[ $(flatpak --user info --show-commit "$other_ref") == "$expected" ]]
    flatpak --user install --reinstall -y --noninteractive --no-deps airscout "$other_ref"
    [[ $(flatpak --user info --show-commit "$other_ref") == "$expected" ]]
fi
