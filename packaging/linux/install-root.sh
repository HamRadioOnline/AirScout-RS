#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 2 ]]; then
    echo 'Usage: install-root.sh <stage-dir> <root-dir>' >&2
    exit 2
fi

stage=$1
root=$2
linux_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
app_id=io.github.hamradioonline.AirScoutRS
programs=(airscout airscout-planeserver airscout-cli catcheck elevation-tile-gen maptile-gen)
legal=(README.md LICENSE NOTICE)

for program in "${programs[@]}"; do
    if [[ ! -f "$stage/bin/$program" || ! -x "$stage/bin/$program" ]]; then
        echo "install-root.sh: missing executable $stage/bin/$program" >&2
        exit 1
    fi
done
for file in "${legal[@]}"; do
    if [[ ! -f "$stage/$file" ]]; then
        echo "install-root.sh: missing $stage/$file" >&2
        exit 1
    fi
done

install -d -- "$root/usr/bin" "$root/usr/share/applications" "$root/usr/share/metainfo" \
    "$root/usr/share/licenses/airscout-rs" "$root/usr/lib/systemd/system" \
    "$root/usr/lib/systemd/system-preset" "$root/usr/lib/sysusers.d" \
    "$root/etc/airscout-planeserver"
for program in "${programs[@]}"; do
    install -m 0755 -- "$stage/bin/$program" "$root/usr/bin/$program"
done
for file in "${legal[@]}"; do
    install -m 0644 -- "$stage/$file" "$root/usr/share/licenses/airscout-rs/$file"
done

install -m 0644 -- "$linux_dir/$app_id.desktop" "$root/usr/share/applications/"
install -m 0644 -- "$linux_dir/$app_id.metainfo.xml" "$root/usr/share/metainfo/"
for size in 16 32 48 64 128 256; do
    icon_dir="$root/usr/share/icons/hicolor/${size}x${size}/apps"
    install -d -- "$icon_dir"
    install -m 0644 -- "$linux_dir/../icons/hicolor/$size.png" "$icon_dir/$app_id.png"
done

install -m 0644 -- "$linux_dir/airscout-planeserver.service" "$root/usr/lib/systemd/system/"
install -m 0644 -- "$linux_dir/00-airscout-planeserver.preset" "$root/usr/lib/systemd/system-preset/"
install -m 0644 -- "$linux_dir/airscout-planeserver.sysusers" "$root/usr/lib/sysusers.d/airscout-planeserver.conf"
install -m 0644 -- "$linux_dir/planeserver.toml" "$root/etc/airscout-planeserver/"
