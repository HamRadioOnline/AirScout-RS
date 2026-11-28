#!/usr/bin/env bash
# Sign the final checksum manifest and verify the detached signature before export.
set +x
set -euo pipefail
umask 022
export LC_ALL=C

signed=false
fingerprint=
work=
installing=false
cleanup() {
    if [[ -n "$work" ]]; then
        if [[ -d "$work/gnupg" ]] && command -v gpgconf >/dev/null; then
            gpgconf --homedir "$work/gnupg" --kill gpg-agent >/dev/null 2>&1 || true
        fi
        rm -rf -- "$work"
    fi
    if [[ "$installing" == true && "$signed" == false ]]; then
        rm -f -- "$output/SHA256SUMS.key.asc" "$output/SHA256SUMS.asc"
    fi
    if [[ -n ${GITHUB_OUTPUT:-} ]]; then
        printf 'checksums_signed=%s\nfingerprint=%s\n' "$signed" "$fingerprint" >> "$GITHUB_OUTPUT"
    fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
    echo "sign-checksums.sh: $1" >&2
    exit 1
}

[[ $# == 1 ]] || fail 'Usage: sign-checksums.sh <output-dir>'
output=$(cd -- "$1" && pwd)
manifest="$output/SHA256SUMS"
[[ -f "$manifest" && ! -L "$manifest" && -s "$manifest" ]] || fail 'Missing SHA256SUMS'
for name in SHA256SUMS.asc SHA256SUMS.key.asc; do
    [[ ! -e "$output/$name" && ! -L "$output/$name" ]] || fail "Output already exists: $name"
done

if [[ -z ${RELEASE_GPG_PRIVATE_KEY:-} ]]; then
    [[ -z ${RELEASE_GPG_PASSPHRASE:-} ]] || fail 'Passphrase configured without a release GPG key'
    echo missing
    exit 0
fi

work=$(mktemp -d)
export GNUPGHOME="$work/gnupg"
mkdir -m 700 -- "$GNUPGHOME"
# The only multiline secret is sent directly to GPG without printing it or
# passing it via argv. Tool diagnostics can include secret material, so keep
# them off the runner log and emit only fixed operation names on errors.
gpg_cmd=(gpg --no-options --batch --no-tty)
printf '%s\n' "$RELEASE_GPG_PRIVATE_KEY" |
    "${gpg_cmd[@]}" --import >/dev/null 2>&1 || fail 'Could not import release GPG key'
unset RELEASE_GPG_PRIVATE_KEY

primary=$("${gpg_cmd[@]}" --with-colons --list-secret-keys 2>/dev/null |
    awk -F: '$1 == "sec" { primary = 1; next } primary && $1 == "fpr" { print $10; primary = 0 }') ||
    fail 'Could not list the imported release key'
[[ "$primary" =~ ^([0-9A-F]{40}|[0-9A-F]{64})$ ]] ||
    fail 'Expected exactly one release secret key'

[[ ${RELEASE_GPG_PASSPHRASE:-} != *$'\n'* ]] || fail 'The release GPG passphrase must be one line'
(umask 077; printf '%s\n' "${RELEASE_GPG_PASSPHRASE:-}" > "$work/passphrase")
unset RELEASE_GPG_PASSPHRASE
"${gpg_cmd[@]}" --pinentry-mode loopback --passphrase-file "$work/passphrase" \
    --local-user "$primary" --armor --detach-sign --output "$work/signature.asc" "$manifest" \
    </dev/null >/dev/null 2>&1 || fail 'Could not sign SHA256SUMS with the configured key and passphrase'
rm -f -- "$work/passphrase"

"${gpg_cmd[@]}" --verify "$work/signature.asc" "$manifest" </dev/null >/dev/null 2>&1 ||
    fail 'Could not verify the detached checksum signature'
"${gpg_cmd[@]}" --armor --export "$primary" > "$work/public.asc" 2>/dev/null ||
    fail 'Could not export the public release key'
[[ -s "$work/signature.asc" && -s "$work/public.asc" ]] ||
    fail 'Expected an ASCII signature and public key'

# Both files are staged outside the signing home and installed only after verification.
installing=true
install -m 0644 "$work/public.asc" "$output/SHA256SUMS.key.asc"
install -m 0644 "$work/signature.asc" "$output/SHA256SUMS.asc"
fingerprint=$primary
signed=true
echo verified
