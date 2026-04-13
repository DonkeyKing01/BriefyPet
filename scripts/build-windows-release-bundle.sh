#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "MINGW"* && "$(uname -s)" != "MSYS"* && "$(uname -s)" != "CYGWIN"* ]]; then
  echo "Windows bundling should be executed on a Windows build host (MSVC toolchain required)."
  echo "Attempting command anyway..."
fi

npm run tauri -- build --target x86_64-pc-windows-msvc --bundles msi,nsis

echo "Windows bundle build completed. Check src-tauri/target/x86_64-pc-windows-msvc/release/bundle"
