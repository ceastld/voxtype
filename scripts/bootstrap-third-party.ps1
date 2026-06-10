#Requires -Version 7.0
<#
.SYNOPSIS
  Shallow-clone ONNX+GGUF engine repos and install llama.cpp inference DLLs for release builds.
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$ThirdParty = Join-Path $Root "third_party"

$pairs = @(
    @{ Name = "Fun-ASR-GGUF"; Url = "https://github.com/HaujetZhao/Fun-ASR-GGUF.git" }
    @{ Name = "Qwen3-ASR-GGUF"; Url = "https://github.com/HaujetZhao/Qwen3-ASR-GGUF.git" }
)

New-Item -ItemType Directory -Force -Path $ThirdParty | Out-Null

foreach ($p in $pairs) {
    $dest = Join-Path $ThirdParty $p.Name
    if (-not (Test-Path $dest)) {
        Write-Host "==> Cloning $($p.Name)" -ForegroundColor Cyan
        git clone --depth 1 $p.Url $dest
    } else {
        Write-Host "==> $($p.Name) already present" -ForegroundColor DarkGray
    }
}

$install = Join-Path $Root "runtime" "scripts" "install-llama-cpp.ps1"
if (-not (Test-Path $install)) {
    throw "Missing $install"
}
Write-Host "==> Installing llama.cpp inference DLLs" -ForegroundColor Cyan
& $install

Write-Host "==> third_party ready" -ForegroundColor Green
