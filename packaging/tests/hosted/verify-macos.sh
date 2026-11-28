#!/usr/bin/env bash
set -euo pipefail

[[ $# == 2 ]] || { echo 'Usage: verify-macos.sh <dmg> <version>' >&2; exit 2; }
dmg=$1
version=$2
arch=${dmg%.dmg}
arch=${arch##*-}
[[ "$arch" == "$(uname -m)" ]]
hosted=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
work=$(mktemp -d)
mount="$work/dmg"
mkdir "$mount"
trap 'rm -rf -- "$work"' EXIT
hdiutil attach "$dmg" -readonly -nobrowse -quiet -mountpoint "$mount"
trap 'hdiutil detach "$mount" -quiet || hdiutil detach "$mount" -force -quiet; rm -rf -- "$work"' EXIT
app="$mount/AirScout-RS.app"
bins="$app/Contents/MacOS"
[[ $(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$app/Contents/Info.plist") == io.github.hamradioonline.AirScoutRS ]]
[[ $(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$app/Contents/Info.plist") == "$version" ]]

case "${APPLE_SIGNING:-unsigned}" in
    signed)
        codesign --verify --deep --strict "$app"
        codesign --verify --strict "$dmg"
        spctl --assess --type execute --verbose=4 "$app"
        spctl --assess --type open --context context:primary-signature --verbose=4 "$dmg"
        xcrun stapler validate "$dmg"
        ;;
    unsigned) echo 'Apple signing credentials unavailable; checking unsigned DMG' ;;
    *) exit 2 ;;
esac
for tool in airscout airscout-planeserver airscout-cli catcheck elevation-tile-gen maptile-gen; do
    [[ $(lipo -archs "$bins/$tool") == "$arch" ]]
    if [[ ${APPLE_SIGNING:-} == signed ]]; then
        identity=$(codesign -dv --verbose=4 "$bins/$tool" 2>&1)
        grep -F 'Authority=Developer ID Application:' <<< "$identity"
        grep -Eq 'flags=.*\(.*runtime.*\)' <<< "$identity"
    fi
done
python3 "$hosted/smoke-stage.py" "$bins" "$version"
