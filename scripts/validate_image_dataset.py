#!/usr/bin/env python3
"""Validate the checked-in image dataset against its manifest."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "test-data" / "images"
MANIFEST = ROOT / "manifest.json"


def main() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    expected_paths: set[Path] = set()
    failures: list[str] = []

    for entry in manifest["files"]:
        relative = Path(entry["path"])
        expected_paths.add(relative)
        path = ROOT / relative
        if not path.is_file():
            failures.append(f"missing: {relative}")
            continue
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if len(data) != entry["byte_size"]:
            failures.append(
                f"size mismatch: {relative}: {len(data)} != {entry['byte_size']}"
            )
        if digest != entry["sha256"]:
            failures.append(
                f"sha256 mismatch: {relative}: {digest} != {entry['sha256']}"
            )

    actual_paths = {
        path.relative_to(ROOT)
        for directory in (ROOT / "raw", ROOT / "common")
        for path in directory.iterdir()
        if path.is_file()
    }
    for unexpected in sorted(actual_paths - expected_paths):
        failures.append(f"unmanifested file: {unexpected}")

    if failures:
        raise SystemExit("dataset validation failed:\n- " + "\n- ".join(failures))
    print(f"validated {len(expected_paths)} image fixtures")


if __name__ == "__main__":
    main()
