#!/usr/bin/env bash
#
# Merge both architecture builds into a copy of the existing Flatpak repository
# and, when the signing key is present, sign the result and emit the public
# channel descriptors.
#
#   prepare-repo.sh <base-repo> <candidate-dir> <stable|beta> <output-repo>
#     Copies <base-repo> (the previous flatpak-repo tree, possibly empty on the
#     first release) into <output-repo>, re-exports each architecture build from
#     <candidate-dir>/<arch>/build.tar.gz under the Flatpak ref
#     app/io.github.hamradioonline.AirScoutRS/<arch>/<channel>, retains the refs
#     of the other architecture and channel, then regenerates the summary and prunes to one parent per ref.
#     With FLATPAK_GPG_PRIVATE_KEY set, every export and the summary are signed
#     (--gpg-sign=<fpr> --gpg-homedir=<temp>) and the public key is exported into
#     airscout-rs.flatpakrepo plus the channel's
#     io.github.hamradioonline.AirScoutRS{.beta,}.flatpakref. Without the key the
#     repo is merged unsigned and no channel descriptor is written. Alpha is
#     rejected: it never publishes a channel.
#
#   prepare-repo.sh check
#     Prints "signed" or "missing" (exit 0) for the workflow's gate.
#
# The CI result sidecar is written next to (not inside) <output-repo>, so the
# published tree stays clean. .nojekyll is written at the repository root.
#
# Reads FLATPAK_GPG_PRIVATE_KEY from the environment. Secret values never reach
# a process list (the key is imported from a temporary file) and any command or
# log that may carry it is redacted before printing.
#
set -euo pipefail
# Repository objects are world-readable; only the imported key file is private.
umask 022
export LC_ALL=C

fail() {
    echo "prepare-repo.sh: $*" >&2
    exit 1
}

PACKAGING_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="io.github.hamradioonline.AirScoutRS"
PAGES_URL="https://hamradioonline.github.io/AirScout-RS/"
ARCHES=(x86_64 aarch64)

SECRET_NAMES=(FLATPAK_GPG_PRIVATE_KEY)

# Replace every configured secret value on stdin with [redacted], longest first,
# including each non-empty line of a multiline secret so an armored private key
# cannot survive in the output. Secret names travel on argv; the values are read
# from the environment so they never reach a process list. The program is passed
# via -c (not a heredoc) so stdin stays free for the data being redacted.
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

signing_state() {
    if [[ -n ${FLATPAK_GPG_PRIVATE_KEY:-} ]]; then
        echo signed
    else
        echo missing
    fi
}

run_checked() {
    local log=$1
    shift
    if ! "$@" > "$log" 2>&1; then
        printf 'prepare-repo.sh: command failed: %s\n' "$*" | redact >&2
        echo "--- redacted tool log ---" >&2
        redact < "$log" >&2
        exit 1
    fi
}

case ${1:-} in
    check)
        signing_state
        exit
        ;;
esac

[[ $# == 4 ]] || fail "Usage: prepare-repo.sh <base-repo> <candidate-dir> <stable|beta> <output-repo> (or 'check')"

base_repo=$1
candidate=$2
channel=$3
output=$4

case "$channel" in
    stable|beta) ;;
    *) fail "Channel must be stable or beta, not: $channel" ;;
esac

[[ -d "$base_repo" ]] || fail "Base repository does not exist: $base_repo"
[[ -d "$candidate" ]] || fail "Candidate directory does not exist: $candidate"
for arch in "${ARCHES[@]}"; do
    [[ -f "$candidate/$arch/build.tar.gz" ]] || fail "Missing $arch build archive"
done

[[ ! -e "$output" && ! -L "$output" ]] || fail "Output already exists: $output"
mkdir -p -- "$(dirname -- "$output")"
output=$(cd "$(dirname -- "$output")" && pwd)/$(basename -- "$output")

work=$(mktemp -d "$(dirname -- "$output")/.airscout-flatpak-prepare.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

state=$(signing_state)

# Work on a copy so a failed export leaves the original base repository intact.
repo="$work/repo"
cp -a -- "$base_repo" "$repo"

sign_arg=""
homedir_arg=""
gpg_home=
if [[ "$state" == signed ]]; then
    gpg_home="$work/gnupg"
    mkdir -m 700 -- "$gpg_home"
    key_file="$work/private.key"
    (umask 077; printf '%s' "$FLATPAK_GPG_PRIVATE_KEY" > "$key_file")
    run_checked "$work/tool.log" env GNUPGHOME="$gpg_home" gpg --batch --import "$key_file"
    rm -f -- "$key_file"
    # Drain stdin so gpg never gets a SIGPIPE mid-stream under pipefail.
    key_id=$(env GNUPGHOME="$gpg_home" gpg --list-secret-keys --with-colons | \
        awk -F: '/^fpr/ && !seen {print $10; seen=1}')
    [[ -n "$key_id" ]] || fail "No secret key imported from FLATPAK_GPG_PRIVATE_KEY"
    sign_arg="--gpg-sign=$key_id"
    homedir_arg="--gpg-homedir=$gpg_home"
fi

# flatpak build-export LOCATION DIRECTORY [BRANCH] commits the build on
# app/<id>/<arch>/<branch>; --arch pins the arch and [BRANCH] is the bare
# channel name (no "/"). Every signing command also gets --gpg-homedir so
# OSTree/GPGME finds the imported key instead of the empty default keyring.
for arch in "${ARCHES[@]}"; do
    mkdir -- "$work/$arch"
    run_checked "$work/tool.log" tar -xzf "$candidate/$arch/build.tar.gz" -C "$work/$arch"
    run_checked "$work/tool.log" flatpak build-export \
        --arch="$arch" ${sign_arg:+"$sign_arg"} ${homedir_arg:+"$homedir_arg"} \
        "$repo" "$work/$arch/build" "$channel"
done
run_checked "$work/tool.log" flatpak build-update-repo --prune --prune-depth=1 \
    ${sign_arg:+"$sign_arg"} ${homedir_arg:+"$homedir_arg"} "$repo"

# A channel descriptor is only meaningful for a signed repository; without the
# key nothing is published and Task 10 leaves the channel closed.
assets=()
if [[ "$state" == signed ]]; then
    if ! env GNUPGHOME="$gpg_home" gpg --export "$key_id" > "$work/public.gpg" 2> "$work/tool.log"; then
        redact < "$work/tool.log" >&2
        fail "Failed to export the public GPG key"
    fi
    [[ -s "$work/public.gpg" ]] || fail "Failed to export the public GPG key"
    # Verify OSTree's summary signature through Flatpak before releasing its descriptors.
    flatpak_user_dir="$work/flatpak-user"
    mkdir -p -- "$flatpak_user_dir"
    run_checked "$work/tool.log" env FLATPAK_USER_DIR="$flatpak_user_dir" flatpak remote-add --user \
        --gpg-import="$work/public.gpg" airscout-verify "file://$repo"
    run_checked "$work/tool.log" env FLATPAK_USER_DIR="$flatpak_user_dir" flatpak remote-ls --user airscout-verify
    gpg_key_b64=$(python3 -c 'import base64, sys; print(base64.b64encode(sys.stdin.buffer.read()).decode())' \
        < "$work/public.gpg")

    cat > "$repo/airscout-rs.flatpakrepo" <<EOF
[Flatpak Repo]
Title=AirScout-RS
Url=$PAGES_URL
Homepage=$PAGES_URL
Comment=AirScout-RS by HamRadioOnline
Description=AirScout-RS is a port of AirScout by DL2ALF (GPL-3.0-only).
GPGKey=$gpg_key_b64
EOF

    ref_name="$APP_ID.flatpakref"
    [[ "$channel" == beta ]] && ref_name="$APP_ID.beta.flatpakref"
    title="AirScout-RS ($([ "$channel" = beta ] && echo Beta || echo Release))"
    cat > "$repo/$ref_name" <<EOF
[Flatpak Ref]
Name=$APP_ID
Branch=$channel
Title=$title
Url=$PAGES_URL
RuntimeRepo=https://flathub.org/repo/flathub.flatpakrepo
GPGKey=$gpg_key_b64
IsRuntime=false
EOF

    assets=("airscout-rs.flatpakrepo" "$ref_name")
fi

# GitHub Pages serves the flatpak-repo tree verbatim; .nojekyll keeps Jekyll
# from rewriting it. The CI sidecar lives next to, not inside, the tree.
: > "$repo/.nojekyll"

python3 - "$repo" "$output" "$channel" "$state" "$(printf '%s\n' "${assets[@]+"${assets[@]}"}")" <<'PY'
import json
import os
import sys
from pathlib import Path

repo, output, channel, signing = sys.argv[1:5]
assets = [a for a in sys.argv[5].split("\n") if a]
result = {
    "format": "flatpak",
    "channel": channel,
    "assets": assets,
    "signing": "verified" if signing == "signed" else "missing",
}
Path(output + ".result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
os.rename(repo, output)
PY

echo "==> Prepared $output for channel $channel (signing: $state)"