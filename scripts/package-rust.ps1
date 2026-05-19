# Package Rust release (Windows)
# Produces target/release/overmax-rs.exe and a minimal zip layout under dist/overmax-rust/

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

Push-Location $repoRoot
try {
    cargo build -p overmax-app --release
    $exe = Join-Path $repoRoot "target/release/overmax-rs.exe"
    if (-not (Test-Path $exe)) {
        throw "Expected $exe after build"
    }
    $dist = Join-Path $repoRoot "dist/overmax-rust"
    if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
    New-Item -ItemType Directory -Path $dist | Out-Null
    Copy-Item $exe (Join-Path $dist "overmax.exe")
    Copy-Item (Join-Path $repoRoot "settings.json") $dist -ErrorAction SilentlyContinue
    Write-Host "Output: $dist (overmax.exe + settings.json if present)"
} finally {
    Pop-Location
}
