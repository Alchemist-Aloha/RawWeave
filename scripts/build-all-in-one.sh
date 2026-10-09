#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
gpui_manifest="$repo_root/apps/desktop-gpui/Cargo.toml"
release_binary="$repo_root/apps/desktop-gpui/target/release/rawweave-gpui"
output_binary="$repo_root/bin/rawweave-desktop"

cargo build --locked --manifest-path "$gpui_manifest" \
  --target-dir "$repo_root/apps/desktop-gpui/target" \
  --release -p rawweave-gpui
install -Dm755 "$release_binary" "$output_binary"

printf 'Built %s\n' "$output_binary"
