# Stages the native transcription runtimes that the release installer carries.
#
# The CPU runtime and the Microsoft Visual C++ runtime it needs go to
# src-tauri/runtime/cpu (installed beside kokorokoe.exe); the Vulkan runtime
# goes to src-tauri/runtime/vulkan (installed in its vulkan\ folder). That is
# the layout lib.rs resolves from the executable's own directory, so the
# installed application needs no other change. Every file is checked by size
# and SHA-256 after copying. src-tauri/runtime is ignored by Git.
#
#   pwsh scripts/stage-bundled-runtimes.ps1
#   pnpm tauri build --ci --target x86_64-pc-windows-msvc --config src-tauri/tauri.release.conf.json -- --locked
param(
    [string] $RuntimeDirectory = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p5-015\cpu-build\bin\Release"),
    [string] $VulkanRuntimeDirectory = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p5-015\vulkan-build\bin\Release"),
    [string] $WhisperSourceDirectory = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-005\whisper.cpp-v1.9.2"),
    [string] $VcRedistDirectory = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repositoryRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$stagingRoot = Join-Path $repositoryRoot "src-tauri\runtime"
$cpuDestination = Join-Path $stagingRoot "cpu"
$vulkanDestination = Join-Path $stagingRoot "vulkan"

$whisperFiles = @(
    "kokorokoe_whisper_adapter.dll",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "ggml-cpu.dll"
)
# whisper.cpp is built with the dynamic MSVC runtime and OpenMP. Windows 10
# and 11 ship the Universal CRT, but not these, so they are deployed
# app-locally as the Visual Studio redistribution terms allow.
$vcRuntimeFiles = @("msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll")
$openMpFiles = @("vcomp140.dll")

function Find-VcRedist {
    if ($VcRedistDirectory) { return (Resolve-Path -LiteralPath $VcRedistDirectory).Path }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswhere)) { throw "vswhere.exe was not found; pass -VcRedistDirectory." }
    $installation = & $vswhere -latest -products * -property installationPath
    $redist = Get-ChildItem -LiteralPath (Join-Path $installation "VC\Redist\MSVC") -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName "x64\Microsoft.VC143.CRT") } |
        Sort-Object { [version]$_.Name } -Descending -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $redist) { throw "No x64 Visual C++ redistributable folder was found; pass -VcRedistDirectory." }
    return (Join-Path $redist.FullName "x64")
}

function Copy-Verified {
    param(
        [Parameter(Mandatory = $true)] [string] $Directory,
        [Parameter(Mandatory = $true)] [string[]] $Names,
        [Parameter(Mandatory = $true)] [string] $Destination,
        [Parameter(Mandatory = $true)] [string] $Label
    )
    foreach ($name in $Names) {
        $source = Join-Path $Directory $name
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "The $Label runtime is incomplete: $name is missing from $Directory."
        }
        $sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
        $target = Join-Path $Destination $name
        Copy-Item -LiteralPath $source -Destination $target -Force
        $targetHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($targetHash -ne $sourceHash) { throw "The staged $Label file $name failed verification." }
        [pscustomobject]@{
            Folder = Split-Path $Destination -Leaf
            Name = $name
            Bytes = (Get-Item -LiteralPath $target).Length
            Sha256 = $targetHash
        }
    }
}

$cpuRuntime = (Resolve-Path -LiteralPath $RuntimeDirectory).Path
$vulkanRuntime = (Resolve-Path -LiteralPath $VulkanRuntimeDirectory).Path
$whisperLicense = Join-Path (Resolve-Path -LiteralPath $WhisperSourceDirectory).Path "LICENSE"
if (-not (Test-Path -LiteralPath $whisperLicense -PathType Leaf)) { throw "The whisper.cpp LICENSE was not found." }
$redist = Find-VcRedist

if (Test-Path -LiteralPath $stagingRoot) { Remove-Item -LiteralPath $stagingRoot -Recurse -Force }
New-Item -ItemType Directory -Path $cpuDestination, $vulkanDestination -Force | Out-Null

$staged = @(
    Copy-Verified -Directory $cpuRuntime -Names $whisperFiles -Destination $cpuDestination -Label "CPU"
    Copy-Verified -Directory (Join-Path $redist "Microsoft.VC143.CRT") -Names $vcRuntimeFiles -Destination $cpuDestination -Label "Visual C++"
    Copy-Verified -Directory (Join-Path $redist "Microsoft.VC143.OpenMP") -Names $openMpFiles -Destination $cpuDestination -Label "OpenMP"
    Copy-Verified -Directory $vulkanRuntime -Names ($whisperFiles + @("ggml-vulkan.dll")) -Destination $vulkanDestination -Label "Vulkan"
)
Copy-Item -LiteralPath $whisperLicense -Destination (Join-Path $stagingRoot "whisper.cpp-LICENSE.txt") -Force

Write-Output "Staged the bundled transcription runtimes in src-tauri\runtime (Visual C++ runtime from $redist)."
$staged | Sort-Object Folder, Name | Format-Table -AutoSize
