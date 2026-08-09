param(
    [string]$ModelDirectory = (Join-Path $env:LOCALAPPDATA "KokoroKoe\p3-004\models")
)

$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$tinyModel = Join-Path $ModelDirectory "ggml-tiny.bin"
$baseModel = Join-Path $ModelDirectory "ggml-base.bin"

foreach ($modelPath in @($tinyModel, $baseModel)) {
    if (-not (Test-Path -LiteralPath $modelPath -PathType Leaf)) {
        throw "Required external verified model is unavailable: $modelPath"
    }
}

$env:KOKOROKOE_P3_010_TINY_MODEL = (Resolve-Path -LiteralPath $tinyModel).Path
$env:KOKOROKOE_P3_010_BASE_MODEL = (Resolve-Path -LiteralPath $baseModel).Path

try {
    cargo test `
        --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
        --locked `
        --target x86_64-pc-windows-msvc `
        models::installer::tests::exact_external_catalog_files_match_pinned_hashes `
        -- `
        --ignored `
        --exact `
        --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "P3-010 exact external model gate failed with exit code $LASTEXITCODE."
    }
}
finally {
    Remove-Item Env:KOKOROKOE_P3_010_TINY_MODEL -ErrorAction SilentlyContinue
    Remove-Item Env:KOKOROKOE_P3_010_BASE_MODEL -ErrorAction SilentlyContinue
}
