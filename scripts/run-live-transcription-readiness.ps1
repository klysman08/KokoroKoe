param(
    [string] $AudioPath = (Join-Path $PSScriptRoot "..\public\audio_test\OpenAI.mp3"),
    [string] $AppLocalDataPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "com.kokorokoe.desktop"),
    [string] $AdapterPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-004\adapter-build\bin\Release\kokorokoe_whisper_adapter.dll")
)

$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$audio = (Resolve-Path -LiteralPath $AudioPath).Path
$appLocalData = (Resolve-Path -LiteralPath $AppLocalDataPath).Path
$adapter = (Resolve-Path -LiteralPath $AdapterPath).Path
$documents = [Environment]::GetFolderPath("MyDocuments")

if (-not (Test-Path -LiteralPath $documents -PathType Container)) {
    throw "The Windows Documents directory is unavailable."
}
if (-not (Test-Path -LiteralPath (Join-Path $appLocalData "kokorokoe.sqlite3") -PathType Leaf)) {
    throw "The product settings database is unavailable. Open KokoroKoe and select an installed default model first."
}

$env:KOKOROKOE_P3_014_APP_LOCAL_DATA = $appLocalData
$env:KOKOROKOE_P3_014_DOCUMENTS = $documents
$env:KOKOROKOE_P3_014_ADAPTER = $adapter
$env:KOKOROKOE_P3_014_AUDIO = $audio

cargo test `
    --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
    --locked `
    --target x86_64-pc-windows-msvc `
    --no-run
if ($LASTEXITCODE -ne 0) {
    throw "The P3-014 readiness test could not be compiled."
}

$playback = Start-Job -ScriptBlock {
    param([string] $Path)
    Add-Type -AssemblyName PresentationCore
    $player = [System.Windows.Media.MediaPlayer]::new()
    try {
        $player.Open([Uri]::new($Path))
        Start-Sleep -Seconds 3
        $player.Play()
        Start-Sleep -Seconds 24
    }
    finally {
        $player.Stop()
        $player.Close()
    }
} -ArgumentList $audio

try {
    cargo test `
        --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
        --locked `
        --target x86_64-pc-windows-msvc `
        transcription::live::tests::hardware_probe_live_dual_capture_to_verified_local_partial_and_final `
        -- `
        --ignored `
        --exact `
        --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "The P3-014 partial-transcription readiness gate failed."
    }
}
finally {
    Stop-Job -Job $playback -ErrorAction SilentlyContinue
    Remove-Job -Job $playback -Force -ErrorAction SilentlyContinue
}
