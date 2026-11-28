#!/usr/bin/env bash
# On macOS, run with the repository root to refresh the checked-in derivatives.
set -euo pipefail

if [[ $# -ne 1 || -z $1 ]]; then
    echo "Usage: $0 <output-dir>" >&2
    exit 2
fi

source_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
output_dir=$1
if [[ $output_dir != /* ]]; then
    output_dir="$PWD/$output_dir"
fi

rust_logo_sha256=840941e53f1f3ae8acbdbf41df54f02f8da55ce6bdb08ce68c9ceb11263fe3b3
if ! (cd -- "$source_dir" && shasum -a 256 -c <<<"$rust_logo_sha256  rust-logo.svg"); then
    echo "Rust logo checksum verification failed" >&2
    exit 1
fi

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/airscout-icons.XXXXXXXX")
trap 'rm -rf -- "$work_dir"' EXIT
mkdir "$work_dir/hicolor" "$work_dir/hicolor-planeserver" \
    "$work_dir/AirScout-RS.iconset" "$work_dir/AirScout-RS-PlaneServer.iconset"

swift "$source_dir/render.swift" "$source_dir" "$work_dir"
for name in AirScout-RS AirScout-RS-PlaneServer; do
    iconutil -c icns -o "$work_dir/$name.icns" "$work_dir/$name.iconset"
done

mkdir -p "$output_dir/packaging/icons/hicolor" "$output_dir/packaging/icons/hicolor-planeserver" \
    "$output_dir/crates/airscout/assets" "$output_dir/crates/airscout-planeserver/assets"
for dir in hicolor hicolor-planeserver; do
    install -m 644 "$work_dir/$dir/"*.png "$output_dir/packaging/icons/$dir/"
done
for name in AirScout-RS AirScout-RS-PlaneServer; do
    install -m 644 "$work_dir/$name.ico" "$work_dir/$name.icns" "$output_dir/packaging/icons/"
done
install -m 644 "$work_dir/airscout.rgba" "$output_dir/crates/airscout/assets/app-icon.rgba"
install -m 644 "$work_dir/airscout-planeserver.rgba" \
    "$output_dir/crates/airscout-planeserver/assets/app-icon.rgba"
