param(
    [string] $RuntimeDirectory = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-004\adapter-build\bin\Release"),
    [string] $ExecutablePath = (Join-Path $PSScriptRoot "..\src-tauri\target\x86_64-pc-windows-msvc\release\kokorokoe.exe")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$runtime = (Resolve-Path -LiteralPath $RuntimeDirectory).Path
$executable = (Resolve-Path -LiteralPath $ExecutablePath).Path
if (-not (Test-Path -LiteralPath $executable -PathType Leaf) -or
    -not [IO.Path]::GetFileName($executable).Equals("kokorokoe.exe", [StringComparison]::OrdinalIgnoreCase)) {
    throw "The POC executable must be an existing kokorokoe.exe file."
}

$destination = Split-Path -Parent $executable
$requiredFiles = @(
    "kokorokoe_whisper_adapter.dll",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "ggml-cpu.dll"
)

$validated = foreach ($name in $requiredFiles) {
    $source = Join-Path $runtime $name
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "The verified CPU runtime is incomplete: $name is missing."
    }
    $item = Get-Item -LiteralPath $source
    if ($item.Length -le 0) {
        throw "The verified CPU runtime is invalid: $name is empty."
    }
    [pscustomobject]@{
        Name = $name
        Source = $source
        Length = $item.Length
        Sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

foreach ($file in $validated) {
    Copy-Item -LiteralPath $file.Source -Destination (Join-Path $destination $file.Name) -Force
}

$staged = foreach ($file in $validated) {
    $target = Join-Path $destination $file.Name
    $targetItem = Get-Item -LiteralPath $target
    $targetHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($targetItem.Length -ne $file.Length -or $targetHash -ne $file.Sha256) {
        throw "The staged CPU runtime failed verification: $($file.Name)."
    }
    [pscustomobject]@{
        Name = $file.Name
        Bytes = $targetItem.Length
        Sha256 = $targetHash
    }
}

Write-Output "Staged and verified the unbundled CPU transcription runtime beside kokorokoe.exe."
$staged | Sort-Object Name | Format-Table -AutoSize
