#Requires -Version 7.0
<#
.SYNOPSIS
  Package voxtype-runtime for Windows (PyInstaller onedir).

.EXAMPLE
  pwsh -NoProfile -File ./scripts/build-win.ps1
  pwsh -NoProfile -File ./scripts/build-win.ps1 -InstallToDevPlugin
#>
param(
    [switch]$InstallToDevPlugin
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

Write-Host "==> uv sync (sherpa-onnx CPU runtime)" -ForegroundColor Cyan
uv sync 2>$null
if ($LASTEXITCODE -ne 0) {
    Write-Host "    uv sync skipped (dev runtime may be running — lock on voxtype-runtime.exe)" -ForegroundColor DarkYellow
} else {
    # Avoid accidentally packaging a stale CUDA wheel left in .venv from hybrid dev.
    uv sync --reinstall-package sherpa-onnx --reinstall-package sherpa-onnx-core 2>$null | Out-Null
}

$PyInstaller = Join-Path $Root ".venv" "Scripts" "pyinstaller.exe"
if (-not (Test-Path $PyInstaller)) {
    Write-Host "==> Installing PyInstaller into venv" -ForegroundColor Cyan
    uv pip install pyinstaller | Out-Null
}

Write-Host "==> PyInstaller build" -ForegroundColor Cyan
$env:PYTHONPATH = Join-Path $Root "src"
& $PyInstaller --noconfirm --clean (Join-Path $Root "voxtype-runtime.spec")

$DistDir = Join-Path $Root "dist" "voxtype-runtime"
$Exe = Join-Path $DistDir "voxtype-runtime.exe"
if (-not (Test-Path $Exe)) {
    throw "Build failed: $Exe not found"
}

$Python = Join-Path $Root ".venv" "Scripts" "python.exe"
$SrcPath = Join-Path $Root "src"
$Version = (& $Python -c "import sys; sys.path.insert(0, r'$SrcPath'); from voxtype_runtime import __version__; print(__version__)").Trim()
$VersionFile = Join-Path $DistDir "runtime-version.txt"
Set-Content -Path $VersionFile -Value $Version -Encoding utf8NoBOM

Write-Host "==> Built $Exe (v$Version, $((Get-Item $Exe).Length / 1MB | ForEach-Object { '{0:N1}' -f $_ }) MB exe)" -ForegroundColor Green

if ($InstallToDevPlugin) {
    $PluginRoot = Join-Path $env:USERPROFILE "Documents" "VoxType" "plugins" "voice-asr"
    $RuntimeDir = Join-Path $PluginRoot "runtime"
    New-Item -ItemType Directory -Force -Path $RuntimeDir | Out-Null

    Write-Host "==> Copy runtime to $RuntimeDir" -ForegroundColor Cyan
    Get-ChildItem $DistDir | ForEach-Object {
        Copy-Item -Path $_.FullName -Destination $RuntimeDir -Recurse -Force
    }

    $ModelSrc = Join-Path $Root "models" "sensevoice"
    if (Test-Path (Join-Path $ModelSrc "tokens.txt")) {
        $ModelDst = Join-Path $PluginRoot "models" "sensevoice"
        New-Item -ItemType Directory -Force -Path $ModelDst | Out-Null
        Copy-Item -Path (Join-Path $ModelSrc "*") -Destination $ModelDst -Recurse -Force
        Write-Host "    copied model to $ModelDst" -ForegroundColor DarkGray
    } else {
        Write-Host "    model not found â€?run: uv run download-asr-model" -ForegroundColor DarkYellow
    }

    $ManifestSrc = Join-Path $Root "manifest.example.json"
    $ManifestDst = Join-Path $PluginRoot "manifest.json"
    if (-not (Test-Path $ManifestDst)) {
        Copy-Item $ManifestSrc $ManifestDst
        Write-Host "    wrote $ManifestDst" -ForegroundColor DarkGray
    }

    Write-Host "==> Installed to $PluginRoot (restart VoxType voice service to pick up)" -ForegroundColor Green
}

Write-Host ""
Write-Host "Smoke test (stub, no model):" -ForegroundColor Yellow
Write-Host "  `$env:VOXTYPE_AUTO_DOWNLOAD_MODEL='0'; & '$Exe' --port 6017"
Write-Host "  Invoke-RestMethod http://127.0.0.1:6017/health"
