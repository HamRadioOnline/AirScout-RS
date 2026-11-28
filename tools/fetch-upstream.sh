#!/usr/bin/env bash
# Fetch the upstream AirScout C# source at the pinned commit into target/upstream/AirScout.
set -euo pipefail
SHA=91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/target/upstream/AirScout"
if [ ! -d "$DEST/.git" ]; then
  mkdir -p "$(dirname "$DEST")"
  git init -q "$DEST"
  git -C "$DEST" remote add origin https://github.com/dl2alf/AirScout
fi
if [ "$(git -C "$DEST" rev-parse HEAD 2>/dev/null || true)" != "$SHA" ]; then
  git -C "$DEST" fetch -q --depth 1 origin "$SHA"
  git -C "$DEST" -c advice.detachedHead=false checkout -q "$SHA"
fi
git -C "$DEST" rev-parse HEAD
