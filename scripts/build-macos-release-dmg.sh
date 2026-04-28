#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

npm run tauri -- build --bundles app

BUNDLE_DIR="src-tauri/target/release/bundle"
MACOS_DIR="$BUNDLE_DIR/macos"
DMG_DIR="$BUNDLE_DIR/dmg"

APP_PATH="$(find "$MACOS_DIR" -maxdepth 1 -name '*.app' | head -n 1)"
if [[ -z "$APP_PATH" ]]; then
  echo "No .app artifact found in $MACOS_DIR"
  exit 1
fi

APP_NAME="$(basename "$APP_PATH")"
VOL_NAME="${APP_NAME%.app}"
VERSION="$(node -p "require('./package.json').version")"

ARCH_RAW="$(uname -m)"
case "$ARCH_RAW" in
  arm64)
    ARCH="aarch64"
    ;;
  x86_64)
    ARCH="x86_64"
    ;;
  *)
    ARCH="$ARCH_RAW"
    ;;
esac

DMG_NAME="${VOL_NAME}_${VERSION}_${ARCH}.dmg"
rm -f "$MACOS_DIR/$DMG_NAME" "$MACOS_DIR/rw.$DMG_NAME"

(
  cd "$MACOS_DIR"
  ../dmg/bundle_dmg.sh \
    --sandbox-safe \
    --volname "$VOL_NAME" \
    --icon "$APP_NAME" 180 170 \
    --app-drop-link 480 170 \
    --window-size 660 400 \
    --hide-extension "$APP_NAME" \
    --volicon ../dmg/icon.icns \
    "$DMG_NAME" \
    "$APP_NAME"
)

DMG_PATH="$MACOS_DIR/$DMG_NAME"
if [[ ! -f "$DMG_PATH" ]]; then
  echo "No release DMG artifact found at $DMG_PATH"
  exit 1
fi

echo "Release DMG generated: $DMG_PATH"
