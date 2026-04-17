# One-time (or after clone): use MSVC only inside this repo, keep global default on GNU if you prefer.
# Requires: Visual Studio Build Tools (MSVC + Windows SDK) and:
#   rustup toolchain install stable-x86_64-pc-windows-msvc

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup override set stable-x86_64-pc-windows-msvc
Write-Host "OK: directory override for $repoRoot -> MSVC"
rustc -vV | Select-String "host:"
