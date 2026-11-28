#!/usr/bin/env bash
# Publish one rendered AUR recipe by fast-forwarding the package branch.
set -euo pipefail

if [[ $# != 2 ]]; then
    echo "Usage: publish.sh <recipe-root> <package-name>" >&2
    exit 2
fi

recipe=$1
package=$2
aur_host=aur.archlinux.org
# The AUR host key is published on the AUR front page (https://aur.archlinux.org/)
# and pinned in packaging/tools.lock.json alongside the other release tools.
fingerprint=$(python3 - <<'PY'
import json
with open("packaging/tools.lock.json", encoding="utf-8") as handle:
    pin = json.load(handle)["aur-ssh-host"]["aur.archlinux.org"]
print(pin["ed25519"])
PY
)
[[ -n "$fingerprint" ]] || {
    echo "error: missing AUR host-key pin" >&2
    exit 1
}

[[ -n "${AUR_SSH_PRIVATE_KEY:-}" ]] || {
    echo "error: AUR_SSH_PRIVATE_KEY is empty" >&2
    exit 1
}

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

key=$workdir/aur_key
known_hosts=$workdir/known_hosts
config=$workdir/config

printf '%s\n' "$AUR_SSH_PRIVATE_KEY" > "$key"
sed -i.bak 's/\r$//' "$key" && rm -f "$key.bak"
chmod 600 "$key"
ssh-keygen -y -f "$key" > "$key.pub"

ssh-keyscan -T 10 -t ed25519 "$aur_host" > "$known_hosts"
[[ -s "$known_hosts" ]] || {
    echo "error: no SSH host key received from $aur_host" >&2
    exit 1
}

actual_fingerprint=$(ssh-keygen -lf "$known_hosts" | awk '{print $2}')
if [[ "$actual_fingerprint" != "$fingerprint" ]]; then
    echo "error: $aur_host host-key fingerprint $actual_fingerprint does not match the pinned $fingerprint" >&2
    exit 1
fi

printf '%s\n' \
    "Host $aur_host" \
    "  HostName $aur_host" \
    "  User aur" \
    "  IdentityFile $key" \
    "  IdentitiesOnly yes" \
    "  StrictHostKeyChecking yes" \
    "  UserKnownHostsFile $known_hosts" \
    > "$config"
chmod 600 "$config"

export GIT_SSH_COMMAND="ssh -F $config"

remote="ssh://aur@$aur_host/$package.git"
remote_has_master=true
if git ls-remote --exit-code --refs "$remote" refs/heads/master > /dev/null; then
    :
else
    status=$?
    if [[ "$status" != 2 ]]; then
        echo "error: failed to look up the live AUR package $package" >&2
        exit 1
    fi
    remote_has_master=false
fi

aur_dir=$workdir/$package
if ! git -c init.defaultBranch=master clone "$remote" "$aur_dir"; then
    echo "error: failed to read the live AUR package $package" >&2
    exit 1
fi
if [[ $(git -C "$aur_dir" symbolic-ref --quiet HEAD) != refs/heads/master ]]; then
    echo "error: unexpected checkout branch for AUR package $package" >&2
    exit 1
fi

# Compare against the clone that will be pushed. If a concurrent first
# publication came in after ls-remote, the clone still gets the downgrade guard.
pkgver_of() {
    awk '$1 == "pkgver" { print $3; exit }' "$1"
}
if git -C "$aur_dir" show-ref --verify --quiet refs/heads/master; then
    current_pkgver=$(pkgver_of "$aur_dir/.SRCINFO")
    candidate_pkgver=$(pkgver_of "$recipe/.SRCINFO")
    if [[ -z "$current_pkgver" || -z "$candidate_pkgver" ]]; then
        echo "error: missing pkgver for AUR package $package" >&2
        exit 1
    fi
    builder=$(python3 - <<'PY'
import json
with open("packaging/tools.lock.json", encoding="utf-8") as handle:
    pin = json.load(handle)["aur-builder"]["linux-x86_64"]
print(pin["url"] + "@" + pin["digest"])
PY
)
    comparison=$(docker run --rm "$builder" vercmp "$candidate_pkgver" "$current_pkgver")
    if [[ "$comparison" == -1 ]]; then
        echo "error: refusing to downgrade $package from $current_pkgver to $candidate_pkgver" >&2
        exit 1
    fi
else
    status=$?
    if [[ "$status" != 1 || "$remote_has_master" == true ]]; then
        echo "error: unable to verify the live AUR master branch for $package" >&2
        exit 1
    fi
fi

cp "$recipe/PKGBUILD" "$aur_dir/PKGBUILD"
cp "$recipe/.SRCINFO" "$aur_dir/.SRCINFO"
[[ ! -f "$recipe/arch-install" ]] || cp "$recipe/arch-install" "$aur_dir/arch-install"

git -C "$aur_dir" config user.name "github-actions[bot]"
git -C "$aur_dir" config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git -C "$aur_dir" config gpg.format ssh
git -C "$aur_dir" config user.signingkey "$key.pub"

git -C "$aur_dir" add PKGBUILD .SRCINFO
[[ ! -f "$aur_dir/arch-install" ]] || git -C "$aur_dir" add arch-install
if git -C "$aur_dir" diff --cached --quiet; then
    echo "$package: package metadata is already current"
    exit 0
fi

git -C "$aur_dir" commit -S -m "Update $package"
git -C "$aur_dir" push origin HEAD:master
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    printf 'published=true\n' >> "$GITHUB_OUTPUT"
fi
echo "$package: fast-forwarded the AUR package branch"
