# Install llama.cpp shared libraries for ONNX+GGUF hybrid engines (Windows Vulkan build).
param(
    [string]$Tag = "b7798",
    [switch]$Cuda
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $root

$asset = if ($Cuda) { "llama-$Tag-bin-win-cuda-x64.zip" } else { "llama-$Tag-bin-win-vulkan-x64.zip" }
$url = "https://github.com/ggml-org/llama.cpp/releases/download/$Tag/$asset"
$tmp = Join-Path $env:TEMP "llama-cpp-$Tag.zip"
$extract = Join-Path $env:TEMP "llama-cpp-$Tag"

Write-Host "Downloading $url"
curl.exe -L --retry 5 --retry-delay 2 -o $tmp $url
if (-not (Test-Path $tmp) -or (Get-Item $tmp).Length -lt 1000000) {
    throw "llama.cpp download failed: $url"
}
if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
Expand-Archive -Path $tmp -DestinationPath $extract -Force

$targets = @(
    (Join-Path $repoRoot "third_party\Fun-ASR-GGUF\fun_asr_gguf\inference\bin"),
    (Join-Path $repoRoot "third_party\Qwen3-ASR-GGUF\qwen_asr_gguf\inference\bin")
)

$dlls = Get-ChildItem $extract -Recurse -Include *.dll
foreach ($dir in $targets) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    foreach ($dll in $dlls) {
        Copy-Item $dll.FullName (Join-Path $dir $dll.Name) -Force
        Write-Host "  -> $($dir)\$($dll.Name)"
    }
}

Write-Host "llama.cpp binaries installed."
