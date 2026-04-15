#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

npm run tauri -- build --bundles app,dmg

BUNDLE_DIR="src-tauri/target/release/bundle"
MACOS_DIR="$BUNDLE_DIR/dmg"

DMG_PATH="$(find "$MACOS_DIR" -maxdepth 1 -name '*.dmg' | head -n 1)"
if [[ -z "$DMG_PATH" ]]; then
  echo "No release DMG artifact found in $MACOS_DIR"
  exit 1
fi

echo "Release DMG generated: $DMG_PATH"
