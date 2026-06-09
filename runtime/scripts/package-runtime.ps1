#Requires -Version 7.0
<#
.SYNOPSIS
  Zip PyInstaller runtime for voice plugin install (step 1 of release).

.EXAMPLE
  pwsh -NoProfile -File ./scripts/package-runtime.ps1
#>
param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$DistDir = Join-Path $Root "dist" "voxtype-runtime"
$Exe = Join-Path $DistDir "voxtype-runtime.exe"
if (-not (Test-Path $Exe)) {
    throw "Missing $Exe â€?run scripts/build-win.ps1 first"
}

if (-not $Version) {
    $VersionFile = Join-Path $DistDir "runtime-version.txt"
    if (Test-Path $VersionFile) {
        $Version = (Get-Content $VersionFile -Raw).Trim()
    } else {
        $Python = Join-Path $Root ".venv" "Scripts" "python.exe"
        $SrcPath = Join-Path $Root "src"
        $Version = (& $Python -c "import sys; sys.path.insert(0, r'$SrcPath'); from voxtype_runtime import __version__; print(__version__)").Trim()
    }
}

$PublishDir = Join-Path $Root "publish"
New-Item -ItemType Directory -Force -Path $PublishDir | Out-Null
$ZipName = "voice-asr-runtime-$Version-win-x64.zip"
$ZipPath = Join-Path $PublishDir $ZipName
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath -Force
}

Write-Host "==> Packaging $ZipPath" -ForegroundColor Cyan
Compress-Archive -Path (Join-Path $DistDir "*") -DestinationPath $ZipPath -CompressionLevel Optimal

$SizeMb = [math]::Round((Get-Item $ZipPath).Length / 1MB, 1)
Write-Host "==> Runtime zip ready ($SizeMb MB): $ZipPath" -ForegroundColor Green
