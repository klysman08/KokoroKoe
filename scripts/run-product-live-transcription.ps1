param(
    [string] $AudioPath = (Join-Path $PSScriptRoot "..\public\audio_test\OpenAI.mp3"),
    [string] $AppLocalDataPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "com.kokorokoe.desktop"),
    [string] $AdapterPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-004\adapter-build\bin\Release\kokorokoe_whisper_adapter.dll"),
    [string] $VulkanAdapterPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-005\adapter-build\bin\Release\kokorokoe_whisper_adapter.dll"),
    [string] $WorkerExecutablePath = "",
    [ValidateRange(1, 30)] [int] $PlaybackDelaySeconds = 8,
    [ValidateRange(20, 120)] [int] $CaptureSeconds = 36,
    [switch] $RequireVulkan
)

$ErrorActionPreference = "Stop"
$probeDirectory = $null
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$audio = (Resolve-Path -LiteralPath $AudioPath).Path
$appLocalData = (Resolve-Path -LiteralPath $AppLocalDataPath).Path
$adapter = (Resolve-Path -LiteralPath $AdapterPath).Path
$vulkanAdapter = (Resolve-Path -LiteralPath $VulkanAdapterPath).Path
$documents = [Environment]::GetFolderPath("MyDocuments")

if (-not (Test-Path -LiteralPath $documents -PathType Container)) {
    throw "The Windows Documents directory is unavailable."
}
if (-not (Test-Path -LiteralPath (Join-Path $appLocalData "kokorokoe.sqlite3") -PathType Leaf)) {
    throw "The product settings database is unavailable. Open KokoroKoe and select an installed default model first."
}

if ([string]::IsNullOrWhiteSpace($WorkerExecutablePath)) {
    cargo build `
        --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
        --locked `
        --target x86_64-pc-windows-msvc `
        --bin kokorokoe
    if ($LASTEXITCODE -ne 0) {
        throw "The P5-013 Vulkan worker executable could not be compiled."
    }
    $worker = Join-Path $repositoryRoot "src-tauri\target\x86_64-pc-windows-msvc\debug\kokorokoe.exe"
}
else {
    $worker = (Resolve-Path -LiteralPath $WorkerExecutablePath).Path
}
if (-not (Test-Path -LiteralPath $worker -PathType Leaf) -or
    -not [IO.Path]::GetFileName($worker).Equals("kokorokoe.exe", [StringComparison]::OrdinalIgnoreCase)) {
    throw "The P5-013 Vulkan worker executable was not produced."
}

if ($RequireVulkan) {
    $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd("\") + "\"
    $probeDirectory = Join-Path $temporaryRoot ("kokorokoe-p5-013-" + [Guid]::NewGuid().ToString("N"))
    $probeDirectory = [IO.Path]::GetFullPath($probeDirectory)
    if (-not $probeDirectory.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "The Vulkan-only probe directory escaped the Windows temporary directory."
    }
    New-Item -ItemType Directory -Path $probeDirectory | Out-Null
    $adapter = Join-Path $probeDirectory "invalid-cpu-adapter.dll"
    New-Item -ItemType File -Path $adapter | Out-Null
}

$env:KOKOROKOE_P3_015_APP_LOCAL_DATA = $appLocalData
$env:KOKOROKOE_P3_015_DOCUMENTS = $documents
$env:KOKOROKOE_P3_015_ADAPTER = $adapter
$env:KOKOROKOE_P3_015_AUDIO = $audio
$env:KOKOROKOE_P5_013_WORKER_EXE = $worker
$env:KOKOROKOE_P5_013_VULKAN_ADAPTER = $vulkanAdapter
$env:KOKOROKOE_P5_013_CAPTURE_SECONDS = $CaptureSeconds.ToString([Globalization.CultureInfo]::InvariantCulture)

cargo test `
    --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
    --locked `
    --target x86_64-pc-windows-msvc `
    --no-run
if ($LASTEXITCODE -ne 0) {
    throw "The P5-013 product live-transcription test could not be compiled."
}

$playback = Start-Job -ScriptBlock {
    param([string] $Path, [int] $DelaySeconds, [int] $DurationSeconds)
    Add-Type -AssemblyName PresentationCore
    $player = [System.Windows.Media.MediaPlayer]::new()
    try {
        $player.Open([Uri]::new($Path))
        Start-Sleep -Seconds $DelaySeconds
        $player.Play()
        Start-Sleep -Seconds $DurationSeconds
    }
    finally {
        $player.Stop()
        $player.Close()
    }
} -ArgumentList $audio, $PlaybackDelaySeconds, $CaptureSeconds

try {
    cargo test `
        --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
        --locked `
        --target x86_64-pc-windows-msvc `
        transcription::product::tests::hardware_probe_product_service_emits_matching_partial_and_final `
        -- `
        --ignored `
        --exact `
        --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "The P5-013 product live-transcription gate failed."
    }
}
finally {
    Stop-Job -Job $playback -ErrorAction SilentlyContinue
    Remove-Job -Job $playback -Force -ErrorAction SilentlyContinue
    if ($probeDirectory -and
        $probeDirectory.StartsWith(
            [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd("\") + "\",
            [StringComparison]::OrdinalIgnoreCase
        ) -and
        (Test-Path -LiteralPath $probeDirectory -PathType Container)) {
        Remove-Item -LiteralPath $probeDirectory -Recurse -Force
    }
}
