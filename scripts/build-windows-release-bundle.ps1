$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
Set-Location $RootDir

npx tauri build --bundles msi nsis

Write-Host "Windows bundle build completed. Check src-tauri/target/release/bundle"
