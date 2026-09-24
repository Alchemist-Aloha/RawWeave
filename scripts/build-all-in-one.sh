#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
frontend_dir="$repo_root/apps/desktop/frontend"
tauri_manifest="$repo_root/apps/desktop/src-tauri/Cargo.toml"
release_binary="$repo_root/apps/desktop/src-tauri/target/release/rawweave-desktop"
output_binary="$repo_root/bin/rawweave-desktop"

# Keep WebdriverIO's test-only frontend and Tauri settings out of this release.
unset VITE_WDIO_TEST VITE_WDIO_BROWSER TAURI_CONFIG

(
  cd "$frontend_dir"
  pnpm run build
)

cargo build --locked --manifest-path "$tauri_manifest" \
  --target-dir "$repo_root/apps/desktop/src-tauri/target" \
  --release --features custom-protocol
install -Dm755 "$release_binary" "$output_binary"

printf 'Built %s\n' "$output_binary"
