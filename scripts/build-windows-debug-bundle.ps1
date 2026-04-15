$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
Set-Location $RootDir

npx tauri build --debug --bundles msi nsis

Write-Host "Windows debug bundle build completed. Check src-tauri/target/debug/bundle"
