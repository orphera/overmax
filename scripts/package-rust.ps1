# Packaging Script

# Package Rust release (Windows)
# Produces target/release/overmax-rs.exe and zips it up matching the Python layout.

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

Push-Location $repoRoot
try {
    Write-Host "Building overmax-app --release..."
    cargo build -p overmax-app --release
    $exe = Join-Path $repoRoot "target/release/overmax-rs.exe"
    if (-not (Test-Path $exe)) {
        throw "Expected $exe after build"
    }

    $distDir = Join-Path $repoRoot "dist/overmax"
    $zipPath = Join-Path $repoRoot "dist/overmax.zip"
    $manifestPath = Join-Path $repoRoot "dist/release_manifest.json"

    if (Test-Path $distDir) { Remove-Item -Recurse -Force $distDir }
    if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
    if (Test-Path $manifestPath) { Remove-Item -Force $manifestPath }

    New-Item -ItemType Directory -Path $distDir | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $distDir "cache") | Out-Null

    Copy-Item $exe (Join-Path $distDir "overmax.exe")
    if (Test-Path (Join-Path $repoRoot "settings.json")) {
        Copy-Item (Join-Path $repoRoot "settings.json") $distDir -ErrorAction SilentlyContinue
    }
    if (Test-Path (Join-Path $repoRoot "README.md")) {
        Copy-Item (Join-Path $repoRoot "README.md") $distDir -ErrorAction SilentlyContinue
    }

    Write-Host "Creating overmax.zip..."
    Compress-Archive -Path "$distDir\*" -DestinationPath $zipPath -Force

    $zipSha256 = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLower()
    
    # Extract version from Cargo.toml
    $cargoTomlPath = Join-Path $repoRoot "rust/overmax_app/Cargo.toml"
    $appVersion = (Select-String -Path $cargoTomlPath -Pattern '^version\s*=\s*"([^"]+)"').Matches.Groups[1].Value

    if (-not $appVersion) {
        throw "Failed to extract version from Cargo.toml"
    }

    $manifest = @{
        version = "v$appVersion"
        generated_at = (Get-Date).ToUniversalTime().ToString('o')
        assets = @(
            @{
                name = 'overmax.zip'
                sha256 = $zipSha256
            }
        )
    }

    $manifest | ConvertTo-Json -Depth 5 | Set-Content -Path $manifestPath -Encoding UTF8
    Write-Host "release_manifest.json created."

    Write-Host "`n============================================================"
    Write-Host " Build complete!"
    Write-Host " Output: $distDir\overmax.exe"
    Write-Host " Release zip: $zipPath"
    Write-Host " Manifest: $manifestPath"
    Write-Host "============================================================"

} finally {
    Pop-Location
}
