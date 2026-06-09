#!/usr/bin/env pwsh
# Build (optional), publish GitHub Release, optional Bitiful mirror, optional channel.json sync.
#
# Runtime and model are published to separate GitHub releases:
#   - runtime: tag v{version} (e.g. v0.1.1)
#   - model:   tag model-sensevoice (versionless asset, updated only when model changes)
#
# Examples:
#   pwsh ./publish/Publish-VoiceAsrRelease.ps1
#   pwsh ./publish/Publish-VoiceAsrRelease.ps1 -SkipBuild -UploadBitiful -UpdateChannelJson  # local Bitiful fallback
#   pwsh ./publish/Publish-VoiceAsrRelease.ps1 -PublishModel -SkipBuild
#   pwsh ./publish/Publish-VoiceAsrRelease.ps1 -DryRun

[CmdletBinding()]
param(
    [string]$Repo = 'QuickerHub/voice-asr-runtime',
    [string]$Version = '',
    [string]$RuntimeTag = '',
    [string]$ModelTag = 'model-sensevoice',
    [string]$ReleaseTitle = '',
    [string]$MonorepoRoot = '',
    [switch]$SkipBuild,
    [switch]$PublishModel,
    [switch]$UploadBitiful,
    [switch]$UpdateChannelJson,
    [switch]$UseLocalVoiceRoot,
    [switch]$ForceRetag,
    [switch]$Draft,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

function Get-ProjectVersion {
    param([string]$Root)
    $DistVersionFile = Join-Path $Root 'dist' 'voxtype-runtime' 'runtime-version.txt'
    if (Test-Path -LiteralPath $DistVersionFile) {
        return (Get-Content -Raw -Path $DistVersionFile).Trim()
    }
    $PyProject = Join-Path $Root 'pyproject.toml'
    if (-not (Test-Path -LiteralPath $PyProject)) {
        throw "pyproject.toml not found: $PyProject"
    }
    foreach ($line in Get-Content -Path $PyProject) {
        if ($line -match '^\s*version\s*=\s*"(.+)"\s*$') {
            return $Matches[1]
        }
    }
    throw "Could not read version from pyproject.toml"
}

function Publish-GitHubReleaseAssets {
    param(
        [string]$ReleaseTag,
        [string]$Title,
        [string]$Notes,
        [string[]]$Assets,
        [string]$Repository,
        [switch]$IsDraft,
        [switch]$AllowRetag
    )

    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw 'GitHub CLI (gh) is required.'
    }
    if ([string]::IsNullOrWhiteSpace($ReleaseTag)) {
        throw 'ReleaseTag is required.'
    }

    $null = gh release view $ReleaseTag --repo $Repository --json tagName -q .tagName 2>$null
    $releaseExists = ($LASTEXITCODE -eq 0)

    if ($releaseExists) {
        Write-Host "==> Release $ReleaseTag exists â€?uploading assets (--clobber)" -ForegroundColor Cyan
        $uploadArgs = @('release', 'upload', $ReleaseTag) + $Assets + @('--repo', $Repository, '--clobber')
        & gh @uploadArgs
        if ($LASTEXITCODE -ne 0) {
            throw 'gh release upload failed'
        }
        return
    }

    if ($AllowRetag) {
        gh api -X DELETE "repos/$Repository/git/refs/tags/$ReleaseTag" 2>$null | Out-Null
    }

    $createArgs = @('release', 'create', $ReleaseTag) + $Assets + @(
        '--repo', $Repository,
        '--title', $Title,
        '--notes', $Notes
    )
    if ($IsDraft) {
        $createArgs += '--draft'
    }

    & gh @createArgs
    if ($LASTEXITCODE -ne 0) {
        throw 'gh release create failed'
    }
}

if (-not $Version) {
    $Version = Get-ProjectVersion -Root $RepoRoot
}
if (-not $RuntimeTag) {
    $RuntimeTag = "v$Version"
}
if (-not $ReleaseTitle) {
    $ReleaseTitle = "voxtype-runtime $RuntimeTag"
}
if (-not $MonorepoRoot) {
    $candidate = Join-Path $RepoRoot '..'
    $channelProbe = Join-Path $candidate 'agent-gui/src-tauri/resources/voice-plugin-channel.json'
    if (Test-Path -LiteralPath $channelProbe) {
        $MonorepoRoot = (Resolve-Path -LiteralPath $candidate).Path
    }
}

$PublishDir = Join-Path $RepoRoot 'publish'
$RuntimeZip = Join-Path $PublishDir "voice-asr-runtime-$Version-win-x64.zip"
$ModelZip = Join-Path $PublishDir 'voice-asr-model-sensevoice.zip'
$ManifestPath = Join-Path $PublishDir 'voice-plugin-channel.generated.json'
$BitifulPrefix = 'https://s3.bitiful.net/quicker-pkgs/quicker-rpc/voice-asr'
$manifestScript = Join-Path $PSScriptRoot 'Write-VoicePluginChannelManifest.ps1'

if (-not $SkipBuild) {
    Write-Host '==> Building runtime (PyInstaller)' -ForegroundColor Cyan
    & (Join-Path $RepoRoot 'scripts' 'build-win.ps1')
    Write-Host '==> Packaging runtime zip' -ForegroundColor Cyan
    & (Join-Path $RepoRoot 'scripts' 'package-runtime.ps1') -Version $Version
    if ($PublishModel) {
        Write-Host '==> Packaging model zip' -ForegroundColor Cyan
        & (Join-Path $RepoRoot 'scripts' 'package-model.ps1')
    }
}

if (-not (Test-Path -LiteralPath $RuntimeZip)) {
    throw "Missing runtime zip: $RuntimeZip (run without -SkipBuild or place zip under publish/)"
}

if ($PublishModel -and -not (Test-Path -LiteralPath $ModelZip)) {
    throw "Missing model zip: $ModelZip (run without -SkipBuild or place zip under publish/)"
}

if (-not (Test-Path -LiteralPath $ModelZip)) {
    Write-Host "==> Model zip not local; manifest will use existing model-sensevoice release if present" -ForegroundColor Yellow
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "Missing model zip and gh is not installed."
    }
    New-Item -ItemType Directory -Force -Path $PublishDir | Out-Null
    gh release download $ModelTag --repo $Repo --pattern 'voice-asr-model-sensevoice.zip' -D $PublishDir 2>$null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $ModelZip)) {
        throw "Model zip missing locally and could not download from release $ModelTag. Run with -PublishModel to package and publish model."
    }
}

& pwsh -NoProfile -File $manifestScript -Version $Version -RuntimeTag $RuntimeTag -ModelTag $ModelTag -OutputPath $ManifestPath | Out-Null

$runtimeMb = [math]::Round((Get-Item $RuntimeZip).Length / 1MB, 1)
$modelMb = [math]::Round((Get-Item $ModelZip).Length / 1MB, 1)

$runtimeNotes = @"
Windows voice **runtime** for VoxType (voxtype-voice-v1).

| Asset | Size (approx) |
|-------|---------------|
| ``voice-asr-runtime-$Version-win-x64.zip`` | ~$runtimeMb MB |
| ``voice-plugin-channel.generated.json`` | channel manifest |

Model is published separately: release tag ``$ModelTag`` (``voice-asr-model-sensevoice.zip``, ~$modelMb MB).

Domestic mirror (Bitiful): ``$BitifulPrefix/``
"@

Write-Host "==> Runtime release $RuntimeTag -> $Repo" -ForegroundColor Cyan
Write-Host "    Runtime:  $RuntimeZip"
Write-Host "    Manifest: $ManifestPath"
Write-Host "    Model ref: $ModelTag / voice-asr-model-sensevoice.zip"

if ($DryRun) {
    Write-Host 'DryRun: skipping GitHub / Bitiful / channel sync' -ForegroundColor Yellow
    exit 0
}

Publish-GitHubReleaseAssets -ReleaseTag $RuntimeTag -Title $ReleaseTitle -Notes $runtimeNotes -Assets @($RuntimeZip, $ManifestPath) -Repository $Repo -IsDraft:$Draft -AllowRetag:$ForceRetag

Write-Host "==> Published runtime https://github.com/$Repo/releases/tag/$RuntimeTag" -ForegroundColor Green

if ($PublishModel) {
    $modelNotes = @"
SenseVoice model pack for VoxType voice plugin (versionless; update only when model files change).

| Asset | Size (approx) |
|-------|---------------|
| ``voice-asr-model-sensevoice.zip`` | ~$modelMb MB |

Domestic mirror (Bitiful): ``$BitifulPrefix/voice-asr-model-sensevoice.zip``
"@
    Write-Host "==> Model release $ModelTag -> $Repo" -ForegroundColor Cyan
    Publish-GitHubReleaseAssets -ReleaseTag $ModelTag -Title 'SenseVoice model (voice-asr)' -Notes $modelNotes -Assets @($ModelZip) -Repository $Repo -IsDraft:$Draft -AllowRetag:$ForceRetag
    Write-Host "==> Published model https://github.com/$Repo/releases/tag/$ModelTag" -ForegroundColor Green
}

if ($UpdateChannelJson) {
    if (-not $MonorepoRoot) {
        throw '-UpdateChannelJson requires quicker-rpc monorepo (agent-gui/src-tauri/resources/voice-plugin-channel.json).'
    }
    $syncScript = Join-Path $MonorepoRoot 'publish/Sync-VoicePluginChannel.ps1'
    if (-not (Test-Path -LiteralPath $syncScript)) {
        throw "Missing sync script: $syncScript"
    }
    Write-Host '==> Syncing voice-plugin-channel.json in quicker-rpc' -ForegroundColor Cyan
    & pwsh -NoProfile -File $syncScript -Version $Version -Tag $RuntimeTag -VoiceRoot $RepoRoot
    if ($LASTEXITCODE -ne 0) {
        throw 'Sync-VoicePluginChannel failed'
    }
}
elseif ($MonorepoRoot) {
    Write-Host ""
    Write-Host "Tip: sync channel.json:" -ForegroundColor Yellow
    Write-Host "  pwsh -NoProfile -File `"$(Join-Path $MonorepoRoot 'publish/Sync-VoicePluginChannel.ps1')`" -Version $Version -Tag $RuntimeTag"
}

if ($UploadBitiful) {
    $uploadScript = Join-Path $MonorepoRoot 'publish/Upload-VoiceAsrToBitiful.ps1'
    if (-not (Test-Path -LiteralPath $uploadScript)) {
        $uploadScript = Join-Path $PSScriptRoot 'Upload-VoiceAsrToBitiful.ps1'
    }
    if (-not (Test-Path -LiteralPath $uploadScript)) {
        throw 'Upload-VoiceAsrToBitiful.ps1 not found (voice-asr-runtime/publish or quicker-rpc/publish).'
    }
    Write-Host ""
    Write-Host '==> Uploading to Bitiful (domestic mirror)' -ForegroundColor Cyan
    $uploadArgs = @('-NoProfile', '-File', $uploadScript, '-Version', $Version, '-RuntimeTag', $RuntimeTag)
    if ($MonorepoRoot) { $uploadArgs += @('-RepoRoot', $MonorepoRoot) }
    if ($UseLocalVoiceRoot) { $uploadArgs += '-UseLocalVoiceRoot' }
    if ($PublishModel) { $uploadArgs += '-PublishModel' }
    & pwsh @uploadArgs
    if ($LASTEXITCODE -ne 0) {
        throw 'Bitiful upload failed'
    }
}
