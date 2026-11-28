#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

if [[ $# != 1 ]]; then
    echo 'Usage: check-tag.sh <tag>' >&2
    exit 2
fi

tag=$1
if [[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
    version=${BASH_REMATCH[1]}
    channel=stable
    prerelease=false
elif [[ "$tag" =~ ^(alpha|beta)-v([0-9]+\.[0-9]+\.[0-9]+)-[A-Za-z0-9.]+$ ]]; then
    version=${BASH_REMATCH[2]}
    channel=${BASH_REMATCH[1]}
    prerelease=true
else
    echo "::error::Tag '$tag' is neither vX.Y.Z nor alpha-/beta-vX.Y.Z-<suffix>" >&2
    exit 1
fi

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
workspace_version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$root/Cargo.toml")
if [[ "$version" != "$workspace_version" ]]; then
    echo "::error::Tag version $version differs from the Cargo.toml version $workspace_version" >&2
    exit 1
fi

# GHCR uses the stable version as an immutable tag and the full beta release tag
# as a version annotation for precedence. Check these forms before building.
canonical_version='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
if [[ "$channel" != alpha && ( ! "$version" =~ $canonical_version || ${#version} -gt 128 ) ]]; then
    echo "::error::Version '$version' must be a canonical X.Y.Z of at most 128 characters for GHCR" >&2
    exit 1
fi
if [[ "$channel" == beta ]]; then
    suffix=${tag#beta-v"$version"-}
    identifiers='^[A-Za-z0-9]+(\.[A-Za-z0-9]+)*$'
    leading_zero='(^|\.)0[0-9]+(\.|$)'
    if [[ ${#tag} -gt 128 || ! "$suffix" =~ $identifiers || "$suffix" =~ $leading_zero ]]; then
        echo "::error::Beta tags must be at most 128 characters with dot-separated alphanumeric identifiers and no leading zeros in numeric identifiers" >&2
        exit 1
    fi
fi

printf 'tag=%s\nversion=%s\nchannel=%s\nprerelease=%s\n' \
    "$tag" "$version" "$channel" "$prerelease" >> "${GITHUB_OUTPUT:?Set GITHUB_OUTPUT to the job output file}"
printf 'Tag %s, version %s, channel %s\n' "$tag" "$version" "$channel"
