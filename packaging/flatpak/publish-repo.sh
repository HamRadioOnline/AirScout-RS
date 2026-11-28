#!/usr/bin/env bash
#
# Publish a prepared, signed Flatpak candidate onto the flatpak-repo branch that
# GitHub Pages serves verbatim.
#
#   publish-repo.sh <candidate-repo> <expected-base-sha>
#     <candidate-repo> is the tree written by prepare-repo.sh (OSTree refs/ and a
#     signed summary plus the public channel descriptors). <expected-base-sha> is
#     the flatpak-repo commit the prepare job recorded when it started (the
#     40-zero sentinel on the very first publish).
#
#     The script refuses to publish an unsigned candidate (exit 0, no change),
#     verifies the prepared summary.sig still verifies, re-checks
#     <expected-base-sha> against the live flatpak-repo branch head (not the
#     source checkout), preflights the GitHub Pages size, and pushes the exact
#     tree as a signed orphan commit using an explicit lease on flatpak-repo.
#     The CI sidecar is written outside the tree after a successful push.
#
#   On a stale base the script fails loudly so the release job re-prepares and
#   re-verifies before retrying.
#
# Reads FLATPAK_GPG_PRIVATE_KEY (the Flatpak signing key, required to publish),
# GITHUB_TOKEN/GITHUB_REPOSITORY (only used in CI for the branch commit/push),
# and GITHUB_REF_NAME (the tag, for the commit message). Secret values never
# reach a process list (the key is imported from a temporary file) and any
# command or log that may carry one is redacted before printing.
#
set -euo pipefail
umask 022
export LC_ALL=C

fail() {
    echo "publish-repo.sh: $*" >&2
    exit 1
}

# A stale flatpak-repo base is retryable: the release workflow re-prepares and
# re-verifies against the updated tip and retries, so it needs a distinct exit
# code it can act on (rather than the generic failure exit 1).
stale_base() {
    echo "publish-repo.sh: $*" >&2
    exit "$EXIT_STALE_BASE"
}

FLATPAK_BRANCH="flatpak-repo"
ORG_IDENTITY="HamRadioOnline"
ZERO_SHA="0000000000000000000000000000000000000000"
EXIT_STALE_BASE=42
PAGES_MAX_BYTES=$((1024 * 1024 * 1024)) # GitHub Pages 1 GiB soft limit.

SECRET_NAMES=(FLATPAK_GPG_PRIVATE_KEY GITHUB_TOKEN)

# Replace every configured secret value on stdin with [redacted], longest first,
# including each non-empty line of a multiline secret so an armored private key
# cannot survive in the output. Secret names travel on argv; the values are read
# from the environment so they never reach a process list.
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

run_checked() {
    local log=$1
    shift
    if ! "$@" > "$log" 2>&1; then
        printf 'publish-repo.sh: command failed: %s\n' "$*" | redact >&2
        echo "--- redacted tool log ---" >&2
        redact < "$log" >&2
        exit 1
    fi
}

[[ $# == 2 ]] || fail "Usage: publish-repo.sh <candidate-repo> <expected-base-sha>"

candidate=$1
expected_base=$2

[[ -d "$candidate" ]] || fail "Candidate repository does not exist: $candidate"
[[ -d "$candidate/refs" && -f "$candidate/summary" ]] ||
    fail "Not a prepared OSTree repository (missing refs/ or summary): $candidate"
[[ -n "$expected_base" ]] || fail "Expected base SHA is empty"

candidate=$(cd "$candidate" && pwd)

# --- Unsigned candidates never publish --------------------------------------
# prepare-repo.sh only writes the public channel descriptors when it signed the
# repository, so their absence means the channel must stay closed. Report
# published=false (never push, upload or deploy) and exit cleanly so the
# workflow can gate Pages deployment on actual publication.
if [[ ! -f "$candidate/airscout-rs.flatpakrepo" ]]; then
    echo "publish-repo.sh: unsigned candidate; publication skipped (no channel change)" >&2
    python3 - "$candidate" <<'PY'
import json
import sys
from pathlib import Path

result = {"format": "flatpak", "published": False, "signing": "missing"}
Path(sys.argv[1] + ".result.json").write_text(
    json.dumps(result, indent=2) + "\n", encoding="utf-8"
)
PY
    exit 0
fi

# Use the Flatpak key to verify the candidate and sign the Git commit.
[[ -n ${FLATPAK_GPG_PRIVATE_KEY:-} ]] ||
    fail "Signed candidate but FLATPAK_GPG_PRIVATE_KEY is unset; refusing to publish"

work=$(mktemp -d)
worktree_dir=""
worktree_cleanup() {
    # Remove the registered git worktree (if one was added) before deleting the
    # temp dir, so a failure never leaves a stale .git/worktrees/ entry.
    if [[ -n "$worktree_dir" && -e "$worktree_dir" ]]; then
        git worktree remove --force -- "$worktree_dir" 2>/dev/null || true
    fi
    rm -rf -- "$work"
}
trap 'worktree_cleanup' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

# Import the signing key into a 0700 temp GNUPGHOME (redacted on failure).
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

# --- Base-SHA guard against the live flatpak-repo branch head ---------------
# Compare the recorded base against the branch's current head, not the source
# checkout's HEAD. `git ls-remote` prints nothing when the branch does not
# exist yet (first publish); any fetch/network error fails the pipeline.
live_head=$(git ls-remote --heads origin "refs/heads/$FLATPAK_BRANCH" | awk '{print $1}')
[[ "${live_head:-$ZERO_SHA}" == "$expected_base" ]] ||
    stale_base "Stale base: flatpak-repo is at ${live_head:-<missing>}, expected $expected_base; re-prepare and reverify"

# --- Re-verify the prepared summary signature before touching the tree -------
# OSTree's summary.sig is a GLib GVariant (detached GPG signatures under
# "ostree.gpgsigs"), not a raw OpenPGP signature, so `gpg --verify` cannot check
# it. Verify with flatpak instead: export the public key, add a throwaway remote
# pointed at the candidate and list it — flatpak fetches summary + summary.sig
# and checks the signature against the imported key before returning.
[[ -f "$candidate/summary.sig" ]] ||
    fail "Candidate summary is unsigned (missing summary.sig); refusing to publish"
env GNUPGHOME="$gpg_home" gpg --export "$key_id" > "$work/pub.gpg"
[[ -s "$work/pub.gpg" ]] || fail "Failed to export the public key for signature verification"
# Isolate the throwaway verification remote from the real user Flatpak config:
# point flatpak at a fresh, empty per-user dir (removed with $work on exit) so
# two consecutive publishes in the same HOME never collide on `airscout-verify`.
flatpak_user_dir="$work/flatpak-user"
mkdir -p -- "$flatpak_user_dir"
run_checked "$work/tool.log" env FLATPAK_USER_DIR="$flatpak_user_dir" flatpak remote-add --user \
    --gpg-import="$work/pub.gpg" airscout-verify "file://$candidate"
run_checked "$work/tool.log" env FLATPAK_USER_DIR="$flatpak_user_dir" flatpak remote-ls --user airscout-verify

# --- GitHub Pages size preflight --------------------------------------------
size_kb=$(du -sk -- "$candidate" | cut -f1)
size_bytes=$((size_kb * 1024))
if (( size_bytes > PAGES_MAX_BYTES )); then
    fail "Candidate repository is ${size_kb} KiB, over the $((PAGES_MAX_BYTES / 1024 / 1024)) MiB Pages limit"
fi

# --- Signed orphan commit and compare-and-swap push (CI only) ----------------
# Leave the preverified OSTree tree untouched. A parentless commit keeps older
# binary payloads out of the branch's Git history. The empty lease on a first
# publish requires that the branch still does not exist.
published_head="${live_head:-}"
if [[ -n ${GITHUB_REPOSITORY:-} ]]; then
    run_checked "$work/tool.log" git worktree add --quiet --detach "$work/tree"
    worktree_dir="$work/tree"
    find "$work/tree" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf -- {} +
    cp -a -- "$candidate"/. "$work/tree"/
    run_checked "$work/tool.log" git -C "$work/tree" add -f -A
    tree=$(git -C "$work/tree" write-tree)
    if ! published_head=$(env GNUPGHOME="$gpg_home" git -C "$work/tree" \
        -c user.name="$ORG_IDENTITY" \
        -c user.email="${ORG_IDENTITY}@users.noreply.github.com" \
        -c user.signingkey="$key_id" \
        -c gpg.format=openpgp \
        -c gpg.program=gpg \
        commit-tree -S "$tree" -m "Flatpak repo: ${GITHUB_REF_NAME:-release}" 2>"$work/tool.log"); then
        redact < "$work/tool.log" >&2
        fail "Failed to create the signed Flatpak branch commit"
    fi
    [[ "$published_head" =~ ^[a-f0-9]{40}$ ]] || fail "Invalid signed commit ID"
    run_checked "$work/tool.log" git -C "$work/tree" push origin \
        --force-with-lease="refs/heads/$FLATPAK_BRANCH:$live_head" \
        "$published_head:refs/heads/$FLATPAK_BRANCH"
fi

# --- CI sidecar outside the published tree ----------------------------------
# Written only after a successful push (or the local validation path), so it
# never claims success for a publish that failed. It is a sibling of the
# candidate so the exact OSTree tree pushed to flatpak-repo carries no internal
# CI metadata.
python3 - "$candidate" "$published_head" "$size_kb" <<'PY'
import json
import sys
from pathlib import Path

candidate, head, size_kb = sys.argv[1:]
result = {
    "format": "flatpak",
    "published": True,
    "base_sha": head,
    "size_kb": int(size_kb),
    "signing": "verified",
}
Path(candidate + ".result.json").write_text(
    json.dumps(result, indent=2) + "\n", encoding="utf-8"
)
PY

echo "==> Published candidate ready at $candidate (base ${published_head:-<none>}, ${size_kb} KiB)"
