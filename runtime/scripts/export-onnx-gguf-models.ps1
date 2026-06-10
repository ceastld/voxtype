# Export ONNX+GGUF hybrid model packs (requires PyTorch + original ModelScope weights).
param(
    [ValidateSet("fun_asr_nano", "qwen_asr", "all")]
    [string]$Preset = "all",
    [string]$DataRoot = ""
)

$ErrorActionPreference = "Stop"
$runtimeRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $runtimeRoot
if (-not $DataRoot) {
    $DataRoot = Join-Path $env:LOCALAPPDATA "VoxType"
}

function Ensure-ThirdParty {
    $pairs = @(
        @{ Name = "Fun-ASR-GGUF"; Url = "https://github.com/HaujetZhao/Fun-ASR-GGUF.git" },
        @{ Name = "Qwen3-ASR-GGUF"; Url = "https://github.com/HaujetZhao/Qwen3-ASR-GGUF.git" }
    )
    foreach ($p in $pairs) {
        $dest = Join-Path $repoRoot "third_party\$($p.Name)"
        if (-not (Test-Path $dest)) {
            git clone --depth 1 $p.Url $dest
        }
    }
}

function Export-FunAsr {
    $work = Join-Path $repoRoot "third_party\Fun-ASR-GGUF"
    $out = Join-Path $DataRoot "models\fun-asr-nano"
    Push-Location $work
    try {
        uv run --directory $runtimeRoot python -m pip install -q onnx onnxruntime scipy gguf rich pypinyin 2>$null
        if (-not (Test-Path "model\tokens.txt")) {
            Write-Host "Exporting Fun-ASR-Nano ONNX+GGUF (this may take a while)..."
            uv run --directory $runtimeRoot python 01-Export-ONNX-FP32.py
            uv run --directory $runtimeRoot python 02-Optimize-ONNX.py
            uv run --directory $runtimeRoot python 03-Quantize-ONNX.py
            uv run --directory $runtimeRoot python 04-Export-Decoder-GGUF-FP16.py
            uv run --directory $runtimeRoot python 05-Quantize-Decoder-GGUF.py
        }
        New-Item -ItemType Directory -Force -Path $out | Out-Null
        Copy-Item "model\*" $out -Recurse -Force
        Write-Host "Fun-ASR-Nano hybrid model -> $out"
    } finally {
        Pop-Location
    }
}

function Export-Qwen {
    $work = Join-Path $repoRoot "third_party\Qwen3-ASR-GGUF"
    $out = Join-Path $DataRoot "models\qwen-asr"
    Push-Location $work
    try {
        if (-not (Test-Path "model\qwen3_asr_llm.q5_k.gguf")) {
            Write-Host "Exporting Qwen3-ASR ONNX+GGUF (this may take a while)..."
            uv run --directory $runtimeRoot python 01-Export-ASR-Encoder-Frontend.py
            uv run --directory $runtimeRoot python 02-Export_ASR-Encoder-Backend.py
            uv run --directory $runtimeRoot python 03-Optimize-ASR-Encoder.py
            uv run --directory $runtimeRoot python 04-Quantize-ASR-Encoder.py
            uv run --directory $runtimeRoot python 05-Export-ASR-Decoder-HF.py
            uv run --directory $runtimeRoot python 06-Convert-ASR-Decoder-GGUF.py
            uv run --directory $runtimeRoot python 07-Quantize-ASR-Decoder-GGUF.py
        }
        New-Item -ItemType Directory -Force -Path $out | Out-Null
        Copy-Item "model\*" $out -Recurse -Force
        Write-Host "Qwen3-ASR hybrid model -> $out"
    } finally {
        Pop-Location
    }
}

Ensure-ThirdParty
& (Join-Path $runtimeRoot "scripts\install-llama-cpp.ps1")

switch ($Preset) {
    "fun_asr_nano" { Export-FunAsr }
    "qwen_asr" { Export-Qwen }
    default {
        Export-FunAsr
        Export-Qwen
    }
}

Write-Host "ONNX+GGUF export done."
