#Requires -Version 7.0
<#
.SYNOPSIS
  Stage ASR runtime + sync semver before Tauri bundle (single installer).

.EXAMPLE
  pwsh -NoProfile -File ./scripts/prepare-tauri-build.ps1
  pwsh -NoProfile -File ./scripts/prepare-tauri-build.ps1 -Version 0.1.3
#>
param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$RuntimeDist = Join-Path $Root "runtime" "dist" "voxtype-runtime"
$RuntimeExe = Join-Path $RuntimeDist "voxtype-runtime.exe"
$StageRoot = Join-Path $Root "app" "src-tauri" "bundle-resources"
$StageRuntime = Join-Path $StageRoot "runtime" "voxtype-runtime"
$CatalogSrc = Join-Path $Root "catalog" "models.json"
$StageCatalogDir = Join-Path $StageRoot "catalog"
$StageCatalog = Join-Path $StageCatalogDir "models.json"

if (-not (Test-Path $RuntimeExe)) {
    throw @"
Missing $RuntimeExe
Build runtime first:
  cd runtime
  pwsh -NoProfile -File ./scripts/build-win.ps1
"@
}

Write-Host "==> Staging runtime -> $StageRuntime" -ForegroundColor Cyan
if (Test-Path $StageRuntime) {
    Remove-Item $StageRuntime -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $StageRuntime | Out-Null
Copy-Item -Path (Join-Path $RuntimeDist "*") -Destination $StageRuntime -Recurse -Force

$ExeMb = [math]::Round((Get-Item $RuntimeExe).Length / 1MB, 1)
$StageMb = [math]::Round(
    (Get-ChildItem $StageRuntime -Recurse -File | Measure-Object -Property Length -Sum).Sum / 1MB,
    1
)
Write-Host "    staged $StageMb MB (exe $ExeMb MB)" -ForegroundColor DarkGray

if (-not (Test-Path $CatalogSrc)) {
    throw "Missing bundled catalog source: $CatalogSrc"
}
Write-Host "==> Staging model catalog -> $StageCatalog" -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $StageCatalogDir | Out-Null
Copy-Item -Path $CatalogSrc -Destination $StageCatalog -Force
pwsh -NoProfile -File (Join-Path $Root "scripts" "validate-models-catalog.ps1") -CatalogPath $StageCatalog

if ($Version) {
    Write-Host "==> Sync version -> $Version" -ForegroundColor Cyan
    $Files = @(
        @{ Path = Join-Path $Root "app" "src-tauri" "tauri.conf.json"; Kind = "json"; Key = "version" }
        @{ Path = Join-Path $Root "app" "package.json"; Kind = "json"; Key = "version" }
        @{ Path = Join-Path $Root "app" "src-tauri" "Cargo.toml"; Kind = "toml"; Key = "version" }
        @{ Path = Join-Path $Root "runtime" "pyproject.toml"; Kind = "toml"; Key = "version" }
    )
    foreach ($item in $Files) {
        if (-not (Test-Path $item.Path)) {
            throw "Missing $($item.Path)"
        }
        $raw = Get-Content $item.Path -Raw
        $updated = if ($item.Kind -eq "json") {
            $pattern = '(?m)^(\s*"version"\s*:\s*")[^"]*(")'
            if ($raw -notmatch $pattern) {
                throw "version field not found in $($item.Path)"
            }
            $raw -replace $pattern, "`${1}$Version`${2}"
        } else {
            $pattern = '(?m)^version\s*=\s*".*"$'
            if ($raw -notmatch $pattern) {
                throw "version field not found in $($item.Path)"
            }
            $raw -replace $pattern, "version = `"$Version`""
        }
        Set-Content $item.Path -Value $updated -Encoding utf8NoBOM -NoNewline
        Write-Host "    $($item.Path)" -ForegroundColor DarkGray
    }
}

Write-Host "==> Tauri bundle inputs ready" -ForegroundColor Green
