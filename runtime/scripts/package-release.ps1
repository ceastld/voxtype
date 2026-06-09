#Requires -Version 7.0
# Build runtime + model release zips for QuickerHub GitHub Release.
param([string]$Version = "")

$ErrorActionPreference = "Stop"
$ScriptDir = $PSScriptRoot
& (Join-Path $ScriptDir "package-runtime.ps1") @PSBoundParameters
if (-not $Version) {
    $Root = Split-Path -Parent (Split-Path -Parent $ScriptDir)
    $VersionFile = Join-Path $Root "dist" "voxtype-runtime" "runtime-version.txt"
    if (Test-Path $VersionFile) {
        $Version = (Get-Content $VersionFile -Raw).Trim()
    }
}
& (Join-Path $ScriptDir "package-model.ps1")
