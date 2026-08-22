param(
    [string] $ScratchPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p5-015"),
    [string] $WhisperSourcePath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-005\whisper.cpp-v1.9.2"),
    [string] $VulkanSdkPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-005\vulkan-sdk-1.4.350.0")
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$repositoryPrefix = $repositoryRoot.TrimEnd("\") + "\"
$scratch = [IO.Path]::GetFullPath($ScratchPath)
if ($scratch.Equals([IO.Path]::GetPathRoot($scratch), [StringComparison]::OrdinalIgnoreCase) -or
    $scratch.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "The P5-015 scratch directory must be a dedicated path outside the repository."
}

$source = (Resolve-Path -LiteralPath $WhisperSourcePath).Path
$vulkanSdk = (Resolve-Path -LiteralPath $VulkanSdkPath).Path
$pinnedCommit = "306c88f4d1286aec1bf96e544632897886af5501"
$modelSha256 = "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"
$modelBytes = 574041195
$modelUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin"
$modelDirectory = Join-Path $scratch "models"
$model = Join-Path $modelDirectory "ggml-large-v3-turbo-q5_0.bin"
$fixtureDirectory = Join-Path $scratch "fixture"
$fixtureWav = Join-Path $fixtureDirectory "generated-model-quality.wav"
$fixtureRaw = Join-Path $fixtureDirectory "generated-model-quality.f32le"
$cpuBuild = Join-Path $scratch "cpu-build"
$vulkanBuild = Join-Path $scratch "vulkan-build"

New-Item -ItemType Directory -Force -Path $scratch, $modelDirectory, $fixtureDirectory | Out-Null

$actualCommit = (git -C $source rev-parse HEAD).Trim()
$origin = (git -C $source remote get-url origin).Trim().TrimEnd("/")
$sourceChanges = git -C $source status --porcelain
if ($actualCommit -ne $pinnedCommit -or
    $origin -ne "https://github.com/ggml-org/whisper.cpp.git" -or
    $sourceChanges) {
    throw "The P5-015 native build requires the clean official pinned whisper.cpp v1.9.2 source."
}
if (-not (Test-Path -LiteralPath (Join-Path $vulkanSdk "Bin\glslc.exe") -PathType Leaf)) {
    throw "The verified Vulkan SDK is unavailable. Run scripts\run-whisper-vulkan-recovery-prototype.ps1 first."
}

function Test-ModelArtifact {
    if (-not (Test-Path -LiteralPath $model -PathType Leaf)) {
        return $false
    }
    $item = Get-Item -LiteralPath $model
    if ($item.Length -ne $modelBytes) {
        return $false
    }
    return (Get-FileHash -LiteralPath $model -Algorithm SHA256).Hash.ToLowerInvariant() -eq $modelSha256
}

if (-not (Test-ModelArtifact)) {
    $temporary = "$model.$([Guid]::NewGuid().ToString('N')).download"
    Invoke-WebRequest -UseBasicParsing -Uri $modelUrl -OutFile $temporary
    $temporaryItem = Get-Item -LiteralPath $temporary
    $temporaryHash = (Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($temporaryItem.Length -ne $modelBytes -or $temporaryHash -ne $modelSha256) {
        throw "The downloaded Large-v3 Turbo Q5_0 artifact failed its immutable byte/hash check; the temporary file was retained for inspection."
    }
    Move-Item -LiteralPath $temporary -Destination $model -Force
}
if (-not (Test-ModelArtifact)) {
    throw "The installed external P5-015 model failed its final immutable byte/hash check."
}

Add-Type -AssemblyName System.Speech
$synthesizer = [System.Speech.Synthesis.SpeechSynthesizer]::new()
try {
    $synthesizer.Rate = 0
    $synthesizer.SelectVoiceByHints(
        [System.Speech.Synthesis.VoiceGender]::NotSet,
        [System.Speech.Synthesis.VoiceAge]::NotSet,
        0,
        [Globalization.CultureInfo]::GetCultureInfo("en-US")
    )
    $synthesizer.SetOutputToWaveFile($fixtureWav)
    $synthesizer.Speak("KokoroKoe verifies local transcription recovery on Windows.")
}
finally {
    $synthesizer.Dispose()
}
ffmpeg -nostdin -v error -y -i $fixtureWav -ac 1 -ar 16000 -f f32le $fixtureRaw
if ($LASTEXITCODE -ne 0) {
    throw "Unable to decode the generated non-sensitive P5-015 speech fixture."
}
$fixtureBytes = (Get-Item -LiteralPath $fixtureRaw).Length
if ($fixtureBytes -le 0 -or $fixtureBytes -gt (480000 * 4) -or $fixtureBytes % 4 -ne 0) {
    throw "The generated P5-015 fixture violates the bounded f32 utterance contract."
}

function Test-NativeOutputCurrent {
    param(
        [Parameter(Mandatory = $true)] [string] $BuildPath,
        [switch] $RequireVulkan
    )
    $adapterOutput = Join-Path $BuildPath "bin\Release\kokorokoe_whisper_adapter.dll"
    if (-not (Test-Path -LiteralPath $adapterOutput -PathType Leaf)) {
        return $false
    }
    if ($RequireVulkan -and
        -not (Test-Path -LiteralPath (Join-Path $BuildPath "bin\Release\ggml-vulkan.dll") -PathType Leaf)) {
        return $false
    }
    $nativeRoot = Join-Path $repositoryRoot "src-tauri\native\whisper_adapter"
    $newestInput = Get-Item -LiteralPath `
        (Join-Path $nativeRoot "CMakeLists.txt"), `
        (Join-Path $nativeRoot "kokorokoe_whisper_adapter.cpp"), `
        (Join-Path $nativeRoot "kokorokoe_whisper_adapter.h") |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    return (Get-Item -LiteralPath $adapterOutput).LastWriteTimeUtc -ge $newestInput.LastWriteTimeUtc
}

if (-not (Test-NativeOutputCurrent -BuildPath $cpuBuild)) {
    cmake `
        -S (Join-Path $repositoryRoot "src-tauri\native\whisper_adapter") `
        -B $cpuBuild `
        -A x64 `
        "-DWHISPER_CPP_SOURCE_DIR=$source"
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to configure the P5-015 CPU adapter."
    }
    cmake --build $cpuBuild --config Release --target kokorokoe_whisper_adapter
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to build the P5-015 CPU adapter."
    }
}

if (-not (Test-NativeOutputCurrent -BuildPath $vulkanBuild -RequireVulkan)) {
    $priorVulkanSdk = $env:VULKAN_SDK
    $priorPath = $env:PATH
    try {
        $env:VULKAN_SDK = $vulkanSdk
        $env:PATH = (Join-Path $vulkanSdk "Bin") + ";" + $priorPath
        cmake `
            -S (Join-Path $repositoryRoot "src-tauri\native\whisper_adapter") `
            -B $vulkanBuild `
            -A x64 `
            "-DWHISPER_CPP_SOURCE_DIR=$source" `
            -DKK_WHISPER_ENABLE_VULKAN=ON
        if ($LASTEXITCODE -ne 0) {
            throw "Unable to configure the P5-015 Vulkan adapter."
        }
        cmake --build $vulkanBuild --config Release --target kokorokoe_whisper_adapter
        if ($LASTEXITCODE -ne 0) {
            throw "Unable to build the P5-015 Vulkan adapter."
        }
    }
    finally {
        $env:VULKAN_SDK = $priorVulkanSdk
        $env:PATH = $priorPath
    }
}

$cpuAdapter = Join-Path $cpuBuild "bin\Release\kokorokoe_whisper_adapter.dll"
$vulkanAdapter = Join-Path $vulkanBuild "bin\Release\kokorokoe_whisper_adapter.dll"
if (-not (Test-Path -LiteralPath $cpuAdapter -PathType Leaf) -or
    -not (Test-Path -LiteralPath $vulkanAdapter -PathType Leaf) -or
    -not (Test-Path -LiteralPath (Join-Path $vulkanBuild "bin\Release\ggml-vulkan.dll") -PathType Leaf)) {
    throw "The P5-015 CPU/Vulkan runtime outputs are incomplete."
}

cargo build `
    --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
    --locked `
    --target x86_64-pc-windows-msvc `
    --bin kokorokoe
if ($LASTEXITCODE -ne 0) {
    throw "The P5-015 worker executable could not be compiled."
}
$worker = Join-Path $repositoryRoot "src-tauri\target\x86_64-pc-windows-msvc\debug\kokorokoe.exe"

$env:KOKOROKOE_P3_008_FIXTURE = $fixtureRaw
$env:KOKOROKOE_P5_015_TURBO_MODEL = $model
$env:KOKOROKOE_P5_015_WORKER_EXE = $worker
$env:KOKOROKOE_P5_015_CPU_ADAPTER = $cpuAdapter
$env:KOKOROKOE_P5_015_VULKAN_ADAPTER = $vulkanAdapter

cargo test `
    --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
    --locked `
    --target x86_64-pc-windows-msvc `
    models::installer::tests::exact_external_large_v3_turbo_file_matches_pinned_hash `
    -- `
    --ignored `
    --exact `
    --nocapture
if ($LASTEXITCODE -ne 0) {
    throw "The exact P5-015 Rust catalog hash gate failed."
}

$stdout = Join-Path $scratch "quality-gate.stdout.txt"
$stderr = Join-Path $scratch "quality-gate.stderr.txt"
$cargoArguments = @(
    "test",
    "--manifest-path", (Join-Path $repositoryRoot "src-tauri\Cargo.toml"),
    "--locked",
    "--target", "x86_64-pc-windows-msvc",
    "transcription::worker::tests::large_v3_turbo_quality_vulkan_and_cpu_fallback_gate",
    "--",
    "--ignored",
    "--exact",
    "--nocapture"
)
$gate = Start-Process `
    -FilePath "cargo.exe" `
    -ArgumentList $cargoArguments `
    -NoNewWindow `
    -PassThru `
    -RedirectStandardOutput $stdout `
    -RedirectStandardError $stderr
$trackedProcessIds = [System.Collections.Generic.HashSet[int]]::new()
[void] $trackedProcessIds.Add($gate.Id)
$peakProcessTreeWorkingSetBytes = 0L
$peakSingleProcessWorkingSetBytes = 0L
while (-not $gate.HasExited) {
    $gate.Refresh()
    $processRows = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
    do {
        $added = $false
        foreach ($row in $processRows) {
            if ($trackedProcessIds.Contains([int] $row.ParentProcessId) -and
                $trackedProcessIds.Add([int] $row.ProcessId)) {
                $added = $true
            }
        }
    } while ($added)
    $currentTreeWorkingSetBytes = 0L
    foreach ($processId in $trackedProcessIds) {
        $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
        if ($process) {
            $currentTreeWorkingSetBytes += $process.WorkingSet64
            $peakSingleProcessWorkingSetBytes = [Math]::Max(
                $peakSingleProcessWorkingSetBytes,
                $process.PeakWorkingSet64
            )
        }
    }
    $peakProcessTreeWorkingSetBytes = [Math]::Max(
        $peakProcessTreeWorkingSetBytes,
        $currentTreeWorkingSetBytes
    )
    Start-Sleep -Milliseconds 500
}
$gate.WaitForExit()
Get-Content -LiteralPath $stdout
Get-Content -LiteralPath $stderr
if ($gate.ExitCode -ne 0) {
    throw "The exact P5-015 model-quality gate failed."
}

Write-Output "artifact_bytes=$modelBytes artifact_sha256=$modelSha256 fixture_bytes=$fixtureBytes peak_process_tree_working_set_bytes=$peakProcessTreeWorkingSetBytes peak_single_process_working_set_bytes=$peakSingleProcessWorkingSetBytes external_artifacts_only=true"
