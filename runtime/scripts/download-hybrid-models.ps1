# Download pre-converted ONNX+GGUF hybrid ASR models (HaujetZhao releases).
param(
    [ValidateSet("fun", "qwen", "all")]
    [string]$Model = "all"
)

$ErrorActionPreference = "Stop"
$modelsRoot = Join-Path $env:LOCALAPPDATA "VoxType\models"

$specs = @(
    @{
        Name = "fun-asr-nano"
        ZipName = "Fun-ASR-Nano-GGUF.zip"
        MinBytes = 700000000
        Urls = @(
            "https://ghfast.top/https://github.com/HaujetZhao/CapsWriter-Offline/releases/download/models/Fun-ASR-Nano-GGUF.zip",
            "https://github.com/HaujetZhao/CapsWriter-Offline/releases/download/models/Fun-ASR-Nano-GGUF.zip"
        )
    },
    @{
        Name = "qwen-asr"
        ZipName = "Qwen3-ASR-0.6B-gguf.zip"
        MinBytes = 500000000
        Urls = @(
            "https://ghfast.top/https://github.com/HaujetZhao/Qwen3-ASR-GGUF/releases/download/models/Qwen3-ASR-0.6B-gguf.zip",
            "https://github.com/HaujetZhao/Qwen3-ASR-GGUF/releases/download/models/Qwen3-ASR-0.6B-gguf.zip"
        )
    }
)

function Install-HybridModel {
    param($Spec)
    $dest = Join-Path $modelsRoot $Spec.Name
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    $zip = Join-Path $env:TEMP $Spec.ZipName
    $ok = $false
    foreach ($u in $Spec.Urls) {
        Write-Host "Trying $u"
        curl.exe -L --retry 5 --retry-delay 3 -C - -o $zip $u
        if ((Get-Item $zip -ErrorAction SilentlyContinue).Length -ge $Spec.MinBytes) {
            $ok = $true
            break
        }
    }
    if (-not $ok) {
        throw "Download failed for $($Spec.Name)"
    }
    $extract = Join-Path $env:TEMP ("voxtype-hybrid-" + $Spec.Name)
    if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
    Expand-Archive -Path $zip -DestinationPath $extract -Force
    $children = Get-ChildItem $extract | Where-Object { $_.PSIsContainer }
    $source = if ($children.Count -eq 1) { $children[0].FullName } else { $extract }
    Get-ChildItem $source -File -Recurse | ForEach-Object {
        Copy-Item $_.FullName (Join-Path $dest $_.Name) -Force
    }
    Write-Host "Installed $($Spec.Name) -> $dest"
    Get-ChildItem $dest | Format-Table Name, Length
}

foreach ($spec in $specs) {
    if ($Model -eq "all" -or ($Model -eq "fun" -and $spec.Name -eq "fun-asr-nano") -or ($Model -eq "qwen" -and $spec.Name -eq "qwen-asr")) {
        Install-HybridModel $spec
    }
}

Write-Host "Done. Run: pwsh -NoProfile -File ./scripts/install-llama-cpp.ps1"
