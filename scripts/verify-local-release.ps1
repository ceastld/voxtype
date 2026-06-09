#Requires -Version 7.0
<#
.SYNOPSIS
  Local release gate: runtime health + NSIS installer layout.

.EXAMPLE
  pwsh -NoProfile -File ./scripts/verify-local-release.ps1
  pwsh -NoProfile -File ./scripts/verify-local-release.ps1 -SkipTauriBuild
#>
param(
    [switch]$SkipTauriBuild,
    [string]$Version = "0.0.0-local",
    [int]$HealthPort = 6033
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)

function Test-RuntimeHealth {
    param(
        [string]$Exe,
        [string]$WorkingDir,
        [int]$Port
    )
    $env:VOXTYPE_AUTO_DOWNLOAD_MODEL = "0"
    $env:VOXTYPE_ALLOW_STUB = "1"
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = "--port $Port"
    $psi.WorkingDirectory = $WorkingDir
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $proc = [System.Diagnostics.Process]::Start($psi)
    try {
        foreach ($i in 1..30) {
            Start-Sleep -Milliseconds 500
            try {
                $health = Invoke-RestMethod "http://127.0.0.1:$Port/health" -TimeoutSec 2
                return $health
            } catch {
                if ($proc.HasExited) {
                    throw "runtime exited early (code $($proc.ExitCode))"
                }
            }
        }
        throw "runtime health timeout on port $Port"
    } finally {
        if (-not $proc.HasExited) {
            $proc.Kill()
            $proc.WaitForExit(5000)
        }
    }
}

Write-Host "==> Build runtime" -ForegroundColor Cyan
Push-Location (Join-Path $Root "runtime")
pwsh -NoProfile -File ./scripts/build-win.ps1 | Out-Host
Pop-Location

$distExe = Join-Path $Root "runtime" "dist" "voxtype-runtime" "voxtype-runtime.exe"
$distDir = Split-Path $distExe -Parent
Write-Host "==> Runtime smoke (dist)" -ForegroundColor Cyan
$distHealth = Test-RuntimeHealth -Exe $distExe -WorkingDir $distDir -Port $HealthPort
Write-Host "    dist health OK: $($distHealth | ConvertTo-Json -Compress)" -ForegroundColor Green

Write-Host "==> Stage bundle inputs" -ForegroundColor Cyan
pwsh -NoProfile -File (Join-Path $Root "scripts" "prepare-tauri-build.ps1") -Version $Version | Out-Host

if (-not $SkipTauriBuild) {
    Write-Host "==> Tauri build" -ForegroundColor Cyan
    Push-Location (Join-Path $Root "app")
    if (-not (Test-Path "node_modules")) {
        pnpm install --frozen-lockfile | Out-Host
    }
    pnpm tauri build | Out-Host
    Pop-Location
}

$setup = Get-ChildItem (Join-Path $Root "app" "src-tauri" "target" "release" "bundle" "nsis" "*.exe") |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $setup) {
    throw "NSIS installer not found under app/src-tauri/target/release/bundle/nsis"
}
Write-Host "==> Installer: $($setup.FullName) ($([math]::Round($setup.Length/1MB,1)) MB)" -ForegroundColor Cyan

$verifyRoot = Join-Path $env:LOCALAPPDATA "VoxType-verify-local"
if (Test-Path $verifyRoot) {
    Remove-Item $verifyRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $verifyRoot | Out-Null
$install = Start-Process -FilePath $setup.FullName -ArgumentList "/S", "/D=$verifyRoot" -Wait -PassThru
if ($install.ExitCode -ne 0) {
    throw "NSIS silent install failed with exit $($install.ExitCode)"
}

$required = @(
    "voxtype.exe",
    "catalog\models.json",
    "runtime\voxtype-runtime\voxtype-runtime.exe"
)
foreach ($rel in $required) {
    $path = Join-Path $verifyRoot $rel
    if (-not (Test-Path $path)) {
        throw "installed layout missing: $rel"
    }
}

$utf8 = New-Object System.Text.UTF8Encoding $false
$catalogText = $utf8.GetString([System.IO.File]::ReadAllBytes((Join-Path $verifyRoot "catalog\models.json")))
$catalog = $catalogText | ConvertFrom-Json
$supported = @($catalog.models | Where-Object { $_.supported -ne $false })
if ($supported.Count -lt 1) {
    throw "bundled catalog has no supported models"
}

$installedExe = Join-Path $verifyRoot "runtime\voxtype-runtime\voxtype-runtime.exe"
$installedDir = Split-Path $installedExe -Parent
Write-Host "==> Runtime smoke (installed)" -ForegroundColor Cyan
$installedHealth = Test-RuntimeHealth -Exe $installedExe -WorkingDir $installedDir -Port ($HealthPort + 1)
Write-Host "    installed health OK: $($installedHealth | ConvertTo-Json -Compress)" -ForegroundColor Green

Write-Host "==> LOCAL RELEASE GATE PASSED" -ForegroundColor Green
Write-Host "installer=$($setup.FullName)"
