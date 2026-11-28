#!/usr/bin/env python3
"""Download a tool from tools.lock.json and verify it before publishing the file."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile
from urllib.request import urlopen


LOCK = Path(__file__).resolve().parents[1] / "tools.lock.json"


def fetch(name, platform, output):
    pin = json.loads(LOCK.read_text(encoding="utf-8"))[name][platform]
    expected = pin["sha256"]
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError(f"{name}/{platform} has an invalid SHA-256 pin")

    digest = hashlib.sha256()
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=output.parent, delete=False) as download:
            temporary = Path(download.name)
            with urlopen(pin["url"], timeout=60) as response:
                for chunk in iter(lambda: response.read(1024 * 1024), b""):
                    digest.update(chunk)
                    download.write(chunk)
        actual = digest.hexdigest()
        if actual != expected:
            raise ValueError(f"{name}/{platform} SHA-256 mismatch: expected {expected}, got {actual}")
        os.replace(temporary, output)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name")
    parser.add_argument("platform")
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    try:
        fetch(args.name, args.platform, args.output)
    except (KeyError, OSError, ValueError) as error:
        parser.exit(1, f"fetch-tool.py: {error}\n")


if __name__ == "__main__":
    main()
