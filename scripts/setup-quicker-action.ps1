#Requires -Version 7.0
<#
.SYNOPSIS
  Scaffold a Quicker shared action that toggles VoxType dictation via the plugin.

.PREREQUISITES
  - Quicker + QuickerRpc plugin loaded
  - qkrpc on PATH (build.ps1 -t from quicker-rpc)
  - VoxType.Plugin built and installed in Quicker

.EXAMPLE
  pwsh -NoProfile -File ./scripts/setup-quicker-action.ps1 -ActionName "语音输入"
#>
param(
    [string]$ActionName = "VoxType 语音输入"
)

$ErrorActionPreference = "Stop"

Write-Host "==> VoxType Quicker action setup" -ForegroundColor Cyan
Write-Host @"

Manual steps (qkrpc headless authoring):

1. Install VoxType client (NSIS) and VoxType.Plugin to Quicker.
2. Create action "$ActionName" with steps:
   - [子程序] 确保 VoxType 运行 → C# 脚本或 运行程序 启动 VoxType.exe
   - [运行 C# 脚本] 调用 VoxType.Plugin.Launcher.Start()
     或拆成两个动作：StartDictation / StopDictation（按住模式）
3. Publish shared action:
   qkrpc action update --id <shared-guid> --json

Plugin entry points (Quicker C# 模块):
  VoxType.Plugin.Launcher.Start()           # toggle
  VoxType.Plugin.Launcher.StartDictation()  # hold press
  VoxType.Plugin.Launcher.StopDictation()     # hold release

HTTP API (no plugin):
  POST http://127.0.0.1:6020/dictate/toggle

"@ -ForegroundColor DarkGray

if (Get-Command qkrpc -ErrorAction SilentlyContinue) {
    Write-Host "==> qkrpc guide (authoring)" -ForegroundColor Cyan
    qkrpc guide get --topic authoring-workflow --json | Out-Null
    Write-Host "    qkrpc available — use action create / workspace_program to author steps." -ForegroundColor Green
} else {
    Write-Host "    qkrpc not found — install quicker-rpc CLI first." -ForegroundColor Yellow
}
