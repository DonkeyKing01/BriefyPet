#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

npm run tauri -- build --debug --bundles dmg

echo "macOS debug DMG build completed. Check src-tauri/target/debug/bundle/dmg"
