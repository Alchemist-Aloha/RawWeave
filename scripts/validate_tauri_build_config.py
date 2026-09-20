#!/usr/bin/env python3
"""Validate that Tauri builds the frontend before packaging it."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TAURI_CONFIG = ROOT / "apps/desktop/src-tauri/tauri.conf.json"


def main() -> int:
    config = json.loads(TAURI_CONFIG.read_text(encoding="utf-8"))
    build = config.get("build", {})
    hook = build.get("beforeBuildCommand")

    if not isinstance(hook, dict):
        raise AssertionError("build.beforeBuildCommand must configure a script and cwd")
    if hook.get("script") != "npm run build":
        raise AssertionError("build.beforeBuildCommand must run npm run build")
    if hook.get("cwd") != "../frontend":
        raise AssertionError("build.beforeBuildCommand must run from ../frontend")
    if build.get("frontendDist") != "../frontend/dist":
        raise AssertionError("build.frontendDist must point to the frontend build output")

    frontend_package = TAURI_CONFIG.parent / hook["cwd"] / "package.json"
    package = json.loads(frontend_package.read_text(encoding="utf-8"))
    if package.get("scripts", {}).get("build") != "tsc -b && vite build":
        raise AssertionError("frontend package must define the production build script")

    print("Tauri build config runs the frontend production build before packaging.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
