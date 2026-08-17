param(
    [string] $RuntimeDirectory = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-004\adapter-build\bin\Release"),
    [string] $VulkanRuntimeDirectory = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-005\adapter-build\bin\Release"),
    [string] $ExecutablePath = (Join-Path $PSScriptRoot "..\src-tauri\target\x86_64-pc-windows-msvc\release\kokorokoe.exe")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$cpuRuntime = (Resolve-Path -LiteralPath $RuntimeDirectory).Path
$vulkanRuntime = (Resolve-Path -LiteralPath $VulkanRuntimeDirectory).Path
$executable = (Resolve-Path -LiteralPath $ExecutablePath).Path
if (-not (Test-Path -LiteralPath $executable -PathType Leaf) -or
    -not [IO.Path]::GetFileName($executable).Equals("kokorokoe.exe", [StringComparison]::OrdinalIgnoreCase)) {
    throw "The POC executable must be an existing kokorokoe.exe file."
}

$destination = Split-Path -Parent $executable
$vulkanDestination = Join-Path $destination "vulkan"
$cpuRequiredFiles = @(
    "kokorokoe_whisper_adapter.dll",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "ggml-cpu.dll"
)
$vulkanRequiredFiles = $cpuRequiredFiles + @("ggml-vulkan.dll")

function Get-ValidatedRuntimeFiles {
    param(
        [Parameter(Mandatory = $true)] [string] $Directory,
        [Parameter(Mandatory = $true)] [string[]] $Names,
        [Parameter(Mandatory = $true)] [string] $Label
    )

    foreach ($name in $Names) {
        $source = Join-Path $Directory $name
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "The verified $Label runtime is incomplete: $name is missing."
        }
        $item = Get-Item -LiteralPath $source
        if ($item.Length -le 0) {
            throw "The verified $Label runtime is invalid: $name is empty."
        }
        [pscustomobject]@{
            Name = $name
            Source = $source
            Length = $item.Length
            Sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
}

function Confirm-StagedRuntimeFiles {
    param(
        [Parameter(Mandatory = $true)] [object[]] $Files,
        [Parameter(Mandatory = $true)] [string] $Directory,
        [Parameter(Mandatory = $true)] [string] $Label
    )

    foreach ($file in $Files) {
        $target = Join-Path $Directory $file.Name
        $targetItem = Get-Item -LiteralPath $target
        $targetHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($targetItem.Length -ne $file.Length -or $targetHash -ne $file.Sha256) {
            throw "The staged $Label runtime failed verification: $($file.Name)."
        }
        [pscustomobject]@{
            Backend = $Label
            Name = $file.Name
            Bytes = $targetItem.Length
            Sha256 = $targetHash
        }
    }
}

$cpuFiles = @(Get-ValidatedRuntimeFiles -Directory $cpuRuntime -Names $cpuRequiredFiles -Label "CPU")
$vulkanFiles = @(Get-ValidatedRuntimeFiles -Directory $vulkanRuntime -Names $vulkanRequiredFiles -Label "Vulkan")

New-Item -ItemType Directory -Path $vulkanDestination -Force | Out-Null
foreach ($file in $cpuFiles) {
    Copy-Item -LiteralPath $file.Source -Destination (Join-Path $destination $file.Name) -Force
}
foreach ($file in $vulkanFiles) {
    Copy-Item -LiteralPath $file.Source -Destination (Join-Path $vulkanDestination $file.Name) -Force
}

$staged = @(
    Confirm-StagedRuntimeFiles -Files $cpuFiles -Directory $destination -Label "CPU"
    Confirm-StagedRuntimeFiles -Files $vulkanFiles -Directory $vulkanDestination -Label "Vulkan"
)

Write-Output "Staged and verified separate unbundled CPU and supervised Vulkan transcription runtimes beside kokorokoe.exe."
$staged | Sort-Object Backend, Name | Format-Table -AutoSize
