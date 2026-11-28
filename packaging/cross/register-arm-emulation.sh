#!/usr/bin/env bash
set -euo pipefail

packaging=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
emulator=$(python3 - "$packaging/tools.lock.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as lock:
    pin = json.load(lock)["container-tools"]["binfmt"]
print(pin["url"] + "@" + pin["digest"])
PY
)
docker run --rm --privileged "$emulator" --install "$1"
