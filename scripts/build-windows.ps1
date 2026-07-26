$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RootDir = Split-Path -Parent $PSScriptRoot
$DesktopDir = Join-Path $RootDir "apps/desktop"

if (-not (Test-Path Env:TAURI_SIGNING_PRIVATE_KEY)) {
    throw "Set TAURI_SIGNING_PRIVATE_KEY before building updater artifacts"
}

if (-not (Test-Path Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
    throw "Set TAURI_SIGNING_PRIVATE_KEY_PASSWORD before building updater artifacts"
}

Set-Location $DesktopDir

Write-Host "Installing frontend dependencies..."
npm ci

Write-Host "Building the Windows x64 NSIS installer..."
cargo tauri build --target x86_64-pc-windows-msvc --bundles nsis

Write-Host "Build complete: $RootDir\target\x86_64-pc-windows-msvc\release\bundle\nsis"
