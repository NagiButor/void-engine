param([switch]$Debug)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $profile = if ($Debug) { 'debug' } else { 'release' }
    $arguments = @('build', '--locked', '-p', 'void-demo')
    if (-not $Debug) { $arguments += '--release' }
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed ($LASTEXITCODE)" }
    $configuration = if ($Debug) { 'Debug' } else { 'Release' }
    $destination = Join-Path $root "build/bin/$configuration"
    New-Item -ItemType Directory -Force $destination | Out-Null
    Copy-Item "target/$profile/void_demo.exe" $destination -Force
    Copy-Item 'assets' $destination -Recurse -Force
    Write-Host "Rust runtime: $destination/void_demo.exe"
} finally {
    Pop-Location
}
