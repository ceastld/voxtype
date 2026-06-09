#Requires -Version 7.0
<#
.SYNOPSIS
  Zip SenseVoice model files for voice plugin install (step 2 of release).

.EXAMPLE
  pwsh -NoProfile -File ./scripts/package-model.ps1
#>
param(
    [switch]$SkipModelDownload
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

$ModelDir = Join-Path $Root "models" "sensevoice"
if (-not (Test-Path (Join-Path $ModelDir "tokens.txt"))) {
    if ($SkipModelDownload) {
        throw "Missing model in $ModelDir — run: uv run download-asr-model"
    }
    Write-Host "==> Downloading SenseVoice model (~160 MB)" -ForegroundColor Cyan
    $env:PYTHONIOENCODING = 'utf-8'
    uv run download-asr-model
}

$PublishDir = Join-Path $Root "publish"
New-Item -ItemType Directory -Force -Path $PublishDir | Out-Null
$ZipName = 'voxtype-model-sensevoice.zip'
$ZipPath = Join-Path $PublishDir $ZipName
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath -Force
}

Write-Host "==> Packaging $ZipPath" -ForegroundColor Cyan
Compress-Archive -Path (Join-Path $ModelDir "*") -DestinationPath $ZipPath -CompressionLevel Optimal

$SizeMb = [math]::Round((Get-Item $ZipPath).Length / 1MB, 1)
Write-Host "==> Model zip ready ($SizeMb MB): $ZipPath" -ForegroundColor Green
