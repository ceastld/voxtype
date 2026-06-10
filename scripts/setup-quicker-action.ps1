#Requires -Version 7.0
<#
.SYNOPSIS
  Scaffold a Quicker shared action for VoxType voice dictation.

.EXAMPLE
  pwsh -NoProfile -File ./scripts/setup-quicker-action.ps1
#>
param(
    [string]$ActionName = "VoxType 语音输入"
)

$ErrorActionPreference = "Stop"

Write-Host "==> VoxType Quicker integration" -ForegroundColor Cyan
Write-Host @"

Architecture: Quicker -> VoxType.Plugin (HTTP) -> VoxType.exe :6020

1. Build plugin:
     cd plugin && dotnet build -c Release
   Copy bin/Release/net472/VoxType.Plugin.dll + voxtype-plugin-channel.json to action package.

2. Register subprogram:
     load {packagePath}/VoxType.Plugin.*.dll
     type VoxType.Plugin.Launcher, VoxType.Plugin

3. First-run action "$ActionName":
   - If not installed: Launcher.DownloadInstaller() -> open voxtype_installer_path (user runs NSIS)
   - Else: Launcher.Start() or hold StartDictation/StopDictation

quicker_in_param modes:
  ?start    POST /dictate/start
  ?stop     POST /dictate/stop     -> voxtype_text
  ?toggle   POST /dictate/toggle
  ?download download NSIS          -> voxtype_installer_path
  ?ensure   launch VoxType.exe only

HTTP (no plugin):
  GET  http://127.0.0.1:6020/health
  POST http://127.0.0.1:6020/dictate/start|stop|toggle

Design doc (HTTP vs pipe): quicker-rpc/docs/voxtype-quicker-integration.md

"@ -ForegroundColor DarkGray

if (Get-Command qkrpc -ErrorAction SilentlyContinue) {
    Write-Host "qkrpc available — use workspace_program to author steps." -ForegroundColor Green
}
