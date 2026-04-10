#!/usr/bin/env python3
"""Generate cargo-sources.json for Flathub from Cargo.lock.

Usage: python3 scripts/generate-cargo-sources.py
Output: packaging/flathub/cargo-sources.json
"""

import json
import re
import sys
from pathlib import Path

LOCK_PATH = Path(__file__).parent.parent / "Cargo.lock"
OUT_PATH = Path(__file__).parent.parent / "packaging" / "flathub" / "cargo-sources.json"

CRATE_URL = "https://crates.io/api/v1/crates/{name}/{version}/download"
VENDOR_DIR = "cargo/vendor"

PACKAGE_RE = re.compile(
    r'\[\[package\]\]\s*\n'
    r'name\s*=\s*"(?P<name>[^"]+)"\s*\n'
    r'version\s*=\s*"(?P<version>[^"]+)"\s*\n'
    r'source\s*=\s*"(?P<source>[^"]+)"\s*\n'
    r'(?:checksum\s*=\s*"(?P<checksum>[^"]+)")?'
)

def main():
    content = LOCK_PATH.read_text()
    sources = []

    for m in PACKAGE_RE.finditer(content):
        source = m.group("source")
        checksum = m.group("checksum")
        if "registry" not in source or not checksum:
            continue

        name = m.group("name")
        version = m.group("version")

        sources.append({
            "type": "file",
            "url": CRATE_URL.format(name=name, version=version),
            "sha256": checksum,
            "dest": VENDOR_DIR,
            "dest-filename": f"{name}-{version}.crate",
        })

    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUT_PATH.write_text(json.dumps(sources, indent=2) + "\n")
    print(f"Generated {len(sources)} crate sources -> {OUT_PATH}")

if __name__ == "__main__":
    main()
