#!/usr/bin/env bash
#
# Build a native macOS .app bundle and DMG from a staged release, signing and
# notarizing it when the Apple credentials are present.
#
#   build-signed-dmg.sh <stage-dir> <tag> <output-dir>
#     Assembles Contents/MacOS, Contents/Resources and Info.plist from the
#     stage, then signs (Developer ID, hardened runtime, timestamp) and
#     notarizes from the inside out when all five secrets are set, or leaves
#     the DMG unsigned otherwise. Writes <output-dir>/assets/<name>.dmg and
#     <output-dir>/result.json.
#
#   build-signed-dmg.sh check
#     Prints "signed" (all five secrets set) or "unsigned" and exits 0.
#
# Reads from the environment:
#   SIGNING_IDENTITY        Developer ID Application identity name (without the
#                           "Developer ID Application: " prefix). Set by
#                           packaging/macos/ci-import-cert.sh, or by hand.
#   MACOS_CERT_P12          base64 of the exported .p12 (used by ci-import-cert.sh)
#   MACOS_CERT_PASSWORD     the .p12 password (used by ci-import-cert.sh)
#   MACOS_NOTARY_KEY        App Store Connect .p8 private key contents
#   MACOS_NOTARY_KEY_ID     the key's ID
#   MACOS_NOTARY_ISSUER     the issuer UUID
#
# A complete set signs and notarizes; a missing secret selects the unsigned
# build, and the result sidecar is marked accordingly. Secret *values* are kept
# off tool command lines where possible (the notary key is written to a file),
# and any command line or log that may carry a secret is redacted before
# printing.
#
set -euo pipefail
# Normal permissions for shipped bundle contents (dirs 0755, files 0644,
# executables 0755); sensitive material is protected by its own umask instead.
umask 022
export LC_ALL=C

fail() {
    echo "build-signed-dmg.sh: $*" >&2
    exit 1
}

PACKAGING_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_DIR="$PACKAGING_DIR/release"
ICON="$PACKAGING_DIR/icons/AirScout-RS.icns"
ENTITLEMENTS="$PACKAGING_DIR/macos/AirScout-RS.entitlements"
INFO_PLIST="$PACKAGING_DIR/macos/Info.plist"
PROGRAMS=(airscout airscout-planeserver airscout-cli catcheck elevation-tile-gen maptile-gen)

SECRET_NAMES=(MACOS_CERT_P12 MACOS_CERT_PASSWORD MACOS_NOTARY_KEY MACOS_NOTARY_KEY_ID MACOS_NOTARY_ISSUER)

# Replace every configured secret value on stdin with [redacted]. A multiline
# secret (e.g. the notary .p8 key) can never match a single input line, so the
# work is delegated to Python: it replaces each whole secret value and each of
# its non-empty lines, longest first. Python's str.replace is a literal (non-
# regex) replacement, so arbitrary secret bytes are safe. Secret *names* travel
# on argv; the secret *values* are read from the environment so they never reach
# a process list. The program is passed via -c (not a heredoc) so stdin stays
# free for the data being redacted.
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
    local count=0 name
    for name in "${SECRET_NAMES[@]}"; do
        if [[ -n ${!name:-} ]]; then
            count=$((count + 1))
        fi
    done
    case $count in
        0) echo unsigned ;;
        5) echo signed ;;
        # Any missing Apple secret selects the unsigned build: a signed but
        # un-notarized DMG would be rejected by Gatekeeper, so partial sets
        # degrade to unsigned rather than shipping a misleading artifact.
        *) echo unsigned ;;
    esac
}

# Run a command, capturing its output; on failure print a redacted copy of the
# argv and the log so a hosted run can be diagnosed without leaking secrets.
run_checked() {
    local log=$1
    shift
    if ! "$@" > "$log" 2>&1; then
        printf 'build-signed-dmg.sh: command failed: %s\n' "$*" | redact >&2
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

[[ $# == 3 ]] || fail "Usage: build-signed-dmg.sh <stage-dir> <tag> <output-dir> (or 'check')"

stage=$(cd "$1" && pwd)
tag=$2
output=$3
[[ -f "$stage/stage.json" ]] || fail "Not a stage directory (missing stage.json): $stage"
[[ ! -e "$output" && ! -L "$output" ]] || fail "Output already exists: $output"
mkdir -p -- "$(dirname -- "$output")"
output=$(cd "$(dirname -- "$output")" && pwd)/$(basename -- "$output")

work=$(mktemp -d "$(dirname -- "$output")/.airscout-macos.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

state=$(signing_state)

# The stage carries the workspace version; check-tag.sh pins the tag to it.
GITHUB_OUTPUT="$work/tag" "$RELEASE_DIR/check-tag.sh" "$tag"
base_version=$(sed -n 's/^version=//p' "$work/tag")

stage_fields=$(python3 - "$stage/stage.json" <<'PY'
import json
import re
import sys
from pathlib import Path

try:
    stage = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    for key, pattern in {
        "target": r"[a-z0-9_-]+",
        "slug": r"macos-(arm64|x86_64)",
        "version": r"[0-9]+\.[0-9]+\.[0-9]+",
    }.items():
        if not re.fullmatch(pattern, stage[key]):
            raise ValueError(f"invalid {key}")
    print(*(stage[key] for key in ("target", "slug", "version")))
except (KeyError, OSError, TypeError, ValueError) as error:
    sys.exit(f"build-signed-dmg.sh: invalid stage.json: {error}")
PY
)
read -r target slug staged_version <<< "$stage_fields"

case "$target:$slug" in
    aarch64-apple-darwin:macos-arm64|x86_64-apple-darwin:macos-x86_64) ;;
    *) fail "Unexpected stage target and slug: $target $slug" ;;
esac
[[ "$staged_version" == "$base_version" ]] ||
    fail "Stage version $staged_version differs from tag $tag"

echo "==> Building $slug bundle for $tag (version $base_version, $state)"

# --- Assemble the .app bundle ------------------------------------------------
# World-readable modes are set explicitly so the bundle installs cleanly into
# /Applications for every user; only temporary key/keychain material is kept
# restrictive.
app="$work/AirScout-RS.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
chmod 755 "$app" "$app/Contents" "$app/Contents/MacOS" "$app/Contents/Resources"
for program in "${PROGRAMS[@]}"; do
    binary="$stage/bin/$program"
    [[ -f "$binary" && -x "$binary" ]] || fail "Missing executable: $binary"
    cp -p -- "$binary" "$app/Contents/MacOS/$program"
    chmod 755 "$app/Contents/MacOS/$program"
done
for name in README.md LICENSE NOTICE; do
    cp -- "$stage/$name" "$app/Contents/Resources/$name"
    chmod 644 "$app/Contents/Resources/$name"
done
cp -- "$ICON" "$app/Contents/Resources/AirScout-RS.icns"
chmod 644 "$app/Contents/Resources/AirScout-RS.icns"
cp -- "$INFO_PLIST" "$app/Contents/Info.plist"
chmod 644 "$app/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$base_version" "$app/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$base_version" "$app/Contents/Info.plist"

if [[ "$state" == signed ]]; then
    : "${SIGNING_IDENTITY:?SIGNING_IDENTITY is not set; run packaging/macos/ci-import-cert.sh first}"
    FULL_IDENTITY="Developer ID Application: $SIGNING_IDENTITY"

    echo "==> Signing bundle contents (inside out)"

    # Every Mach-O file carries its own signature before the enclosing bundle
    # is sealed. The release binaries are unsigned, so collect them first.
    macho_list="$work/macho"
    : > "$macho_list"
    for binary in "$app/Contents/MacOS/"*; do
        if [[ "$(file --mime-type -b "$binary")" == application/x-mach-binary ]]; then
            printf '%s\n' "$binary" >> "$macho_list"
        fi
    done
    [[ -s "$macho_list" ]] || fail "No Mach-O binaries found in the app bundle"

    while IFS= read -r binary; do
        run_checked "$work/tool.log" codesign --force --timestamp --options runtime \
            --sign "$FULL_IDENTITY" "$binary"
    done < "$macho_list"

    run_checked "$work/tool.log" codesign --force --timestamp --options runtime \
        --entitlements "$ENTITLEMENTS" --sign "$FULL_IDENTITY" "$app"

    # Notarization rejects a bundle with an unsigned binary or a missing secure
    # timestamp, and a round trip to Apple costs minutes. Check locally first.
    while IFS= read -r binary; do
        info=$(codesign -dv --verbose=2 "$binary" 2>&1 || true)
        printf '%s' "$info" | grep -q "Authority=Developer ID Application" ||
            fail "${binary#$app/}: not signed with a Developer ID certificate"
        printf '%s' "$info" | grep -q "Timestamp=" ||
            fail "${binary#$app/}: signature has no secure timestamp"
    done < "$macho_list"
fi

# --- Build the DMG -----------------------------------------------------------
dmg_stage="$work/dmg-stage"
mkdir -p "$dmg_stage"
ditto "$app" "$dmg_stage/AirScout-RS.app"
ln -s /Applications "$dmg_stage/Applications"

asset="AirScout-RS-$tag-$slug.dmg"
mkdir -p -- "$work/output/assets"
dmg="$work/output/assets/$asset"

echo "==> Building DMG"
run_checked "$work/tool.log" hdiutil create -volname "AirScout-RS" \
    -srcfolder "$dmg_stage" -ov -format UDZO -quiet "$dmg"
[[ -s "$dmg" ]] || fail "hdiutil produced no DMG"

if [[ "$state" == signed ]]; then
    echo "==> Signing DMG"
    run_checked "$work/tool.log" codesign --force --timestamp --sign "$FULL_IDENTITY" "$dmg"

    echo "==> Notarizing"
    notary_key="$work/notary-key.p8"
    (umask 077; printf '%s' "$MACOS_NOTARY_KEY" > "$notary_key")
    run_checked "$work/tool.log" xcrun notarytool submit "$dmg" \
        --key "$notary_key" --key-id "$MACOS_NOTARY_KEY_ID" \
        --issuer "$MACOS_NOTARY_ISSUER" --wait --timeout 30m
    rm -f "$notary_key"

    echo "==> Stapling"
    run_checked "$work/tool.log" xcrun stapler staple "$dmg"
fi

# --- Verify what actually ships ----------------------------------------------
echo "==> Verifying"
mount_point="$work/mount"
mkdir -p "$mount_point"
hdiutil attach "$dmg" -nobrowse -quiet -mountpoint "$mount_point"
trap 'hdiutil detach "$mount_point" -quiet 2>/dev/null || hdiutil detach "$mount_point" -force -quiet 2>/dev/null || true; rm -rf -- "$work"' EXIT

shipped_app="$mount_point/AirScout-RS.app"
[[ -d "$shipped_app" ]] || fail "The DMG does not contain AirScout-RS.app"

if [[ "$state" == signed ]]; then
    run_checked "$work/tool.log" codesign --verify --deep --strict --verbose=2 "$shipped_app"
    if codesign -dv "$shipped_app" 2>&1 | grep -q "adhoc"; then
        fail "the app inside the DMG is ad-hoc signed, not Developer ID signed"
    fi
    run_checked "$work/tool.log" xcrun stapler validate "$dmg"

    # Gatekeeper assessment: a signed build must pass it or the release gate
    # fails. An unsigned build is expected to be "rejected", so it skips this
    # step entirely rather than swallowing the result.
    run_checked "$work/tool.log" spctl --assess --type execute --verbose=4 "$shipped_app"
    run_checked "$work/tool.log" spctl --assess --type open \
        --context context:primary-signature --verbose=4 "$dmg"
fi

hdiutil detach "$mount_point" -quiet 2>/dev/null || hdiutil detach "$mount_point" -force -quiet 2>/dev/null || true
trap 'rm -rf -- "$work"' EXIT

# --- Result sidecar ----------------------------------------------------------
python3 - "$work/output" "$output" "$asset" "$slug" "$state" <<'PY'
import json
import os
import sys
from pathlib import Path

work_output, output, asset, slug, signing = sys.argv[1:]
result = {
    "format": "dmg",
    "slug": slug,
    "assets": [asset],
    "signing": signing,
}
(Path(work_output) / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
os.rename(work_output, output)
PY

echo "==> Built $output/assets/$asset (signing: $state)"
