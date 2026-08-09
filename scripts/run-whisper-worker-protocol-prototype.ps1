param(
    [string] $ScratchPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-005")
)

$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$scratch = [System.IO.Path]::GetFullPath($ScratchPath)
$repositoryPrefix = $repositoryRoot.TrimEnd("\") + "\"
if ($scratch.Equals([System.IO.Path]::GetPathRoot($scratch), [StringComparison]::OrdinalIgnoreCase) -or
    $scratch.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "The P3-008 scratch input must be a dedicated path outside the repository."
}

$adapter = Join-Path $scratch "adapter-build\bin\Release\kokorokoe_whisper_adapter.dll"
$model = Join-Path $scratch "models\ggml-tiny.bin"
$fixture = Join-Path $scratch "fixture\generated-vulkan-recovery.f32le"
foreach ($inputPath in @($adapter, $model, $fixture)) {
    if (-not (Test-Path -LiteralPath $inputPath -PathType Leaf)) {
        throw "P3-008 requires the verified external P3-005 adapter, Tiny model, and generated fixture. Run scripts\run-whisper-vulkan-recovery-prototype.ps1 first."
    }
}
$modelHash = (Get-FileHash -LiteralPath $model -Algorithm SHA256).Hash.ToLowerInvariant()
if ($modelHash -ne "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21") {
    throw "The external Tiny model failed its pinned SHA-256 check."
}
$fixtureBytes = (Get-Item -LiteralPath $fixture).Length
if ($fixtureBytes -le 0 -or $fixtureBytes -gt (480000 * 4) -or $fixtureBytes % 4 -ne 0) {
    throw "The generated P3-008 fixture violates the bounded f32 utterance contract."
}

cargo build `
    --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
    --locked `
    --target x86_64-pc-windows-msvc `
    --bin kokorokoe
if ($LASTEXITCODE -ne 0) {
    throw "Unable to build the debug worker executable."
}
$worker = Join-Path $repositoryRoot "src-tauri\target\x86_64-pc-windows-msvc\debug\kokorokoe.exe"
if (-not (Test-Path -LiteralPath $worker -PathType Leaf)) {
    throw "The debug worker executable was not produced."
}

$priorWorker = $env:KOKOROKOE_P3_008_WORKER_EXE
$priorAdapter = $env:KOKOROKOE_WHISPER_ADAPTER
$priorModel = $env:KOKOROKOE_WHISPER_TINY_MODEL
$priorFixture = $env:KOKOROKOE_P3_008_FIXTURE
try {
    $env:KOKOROKOE_P3_008_WORKER_EXE = $worker
    $env:KOKOROKOE_WHISPER_ADAPTER = $adapter
    $env:KOKOROKOE_WHISPER_TINY_MODEL = $model
    $env:KOKOROKOE_P3_008_FIXTURE = $fixture
    cargo test `
        --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
        --locked `
        --target x86_64-pc-windows-msvc `
        transcription::worker::tests::vulkan_worker_protocol_lifecycle_and_cpu_recovery_gate `
        -- `
        --ignored `
        --exact `
        --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "The P3-008 worker protocol/lifecycle gate failed."
    }
}
finally {
    $env:KOKOROKOE_P3_008_WORKER_EXE = $priorWorker
    $env:KOKOROKOE_WHISPER_ADAPTER = $priorAdapter
    $env:KOKOROKOE_WHISPER_TINY_MODEL = $priorModel
    $env:KOKOROKOE_P3_008_FIXTURE = $priorFixture
}
