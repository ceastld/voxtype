#Requires -Version 7.0
<#
.SYNOPSIS
  Ensure bundled models.json has ModelScope URLs for every supported model.
#>
param(
    [Parameter(Mandatory = $true)]
    [string]$CatalogPath
)

$ErrorActionPreference = "Stop"

if (-not [System.IO.Path]::IsPathRooted($CatalogPath)) {
    $CatalogPath = Join-Path (Get-Location) $CatalogPath
}

if (-not (Test-Path $CatalogPath)) {
    throw "Missing catalog: $CatalogPath"
}

$doc = Get-Content $CatalogPath -Raw | ConvertFrom-Json
if (-not $doc.models) {
    throw "catalog has no models array: $CatalogPath"
}

$errors = [System.Collections.Generic.List[string]]::new()

foreach ($model in $doc.models) {
    $id = [string]$model.id
    $supported = $model.supported -ne $false
    if (-not $supported) {
        continue
    }

    $download = $model.download
    if ($null -eq $download) {
        $errors.Add("[$id] supported=true but download is missing")
        continue
    }

    $source = [string]$download.source
    if ($source -ine "modelscope") {
        $errors.Add("[$id] supported model must use download.source=modelscope (got: $source)")
        continue
    }

    $base = [string]$download.modelscopeResolveBase
    if ([string]::IsNullOrWhiteSpace($base)) {
        $errors.Add("[$id] missing download.modelscopeResolveBase")
    } elseif ($base -notmatch '^https://www\.modelscope\.cn/models/') {
        $errors.Add("[$id] modelscopeResolveBase must be a ModelScope resolve URL")
    }

    $files = @($download.modelscopeFiles)
    if ($files.Count -eq 0) {
        $errors.Add("[$id] missing download.modelscopeFiles")
        continue
    }

    $required = @($files | Where-Object { $_.required -ne $false })
    if ($required.Count -eq 0) {
        $errors.Add("[$id] modelscopeFiles has no required entries")
    }
}

if ($errors.Count -gt 0) {
    throw ("models.json validation failed:`n - " + ($errors -join "`n - "))
}

Write-Host "==> models.json OK ($($doc.models.Count) entries, supported models have ModelScope URLs)" -ForegroundColor Green
