param(
    [string] $ScratchPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-005"),
    [string] $VulkanSdkPath = "",
    [ValidateRange(1, 64)]
    [int] $Threads = [Math]::Min([Environment]::ProcessorCount, 8)
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$scratch = [System.IO.Path]::GetFullPath($ScratchPath)
$repositoryPrefix = $repositoryRoot.TrimEnd("\") + "\"
if ($scratch.Equals([System.IO.Path]::GetPathRoot($scratch), [StringComparison]::OrdinalIgnoreCase) -or
    $scratch.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "The P3-005 scratch directory must be a dedicated path outside the repository."
}

$pinnedCommit = "306c88f4d1286aec1bf96e544632897886af5501"
$vulkanSdkVersion = "1.4.350.0"
$vulkanSdkSha256 = "855b27ba05d2d8119c5114c5d4ff870ca38f2c632b11e1bb9923b9b7e6ecfe7b"
$source = Join-Path $scratch "whisper.cpp-v1.9.2"
$models = Join-Path $scratch "models"
$build = Join-Path $scratch "adapter-build"
$fixtureDirectory = Join-Path $scratch "fixture"
$fixtureWav = Join-Path $fixtureDirectory "generated-vulkan-recovery.wav"
$fixtureRaw = Join-Path $fixtureDirectory "generated-vulkan-recovery.f32le"

New-Item -ItemType Directory -Force -Path $scratch, $models, $fixtureDirectory | Out-Null

if (-not (Test-Path -LiteralPath $source -PathType Container)) {
    git clone --branch v1.9.2 --depth 1 https://github.com/ggml-org/whisper.cpp.git $source
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to clone the pinned whisper.cpp source."
    }
}
$actualCommit = (git -C $source rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $actualCommit -ne $pinnedCommit) {
    throw "The local whisper.cpp source does not match the P3-005 pinned commit."
}
$origin = (git -C $source remote get-url origin).Trim().TrimEnd("/")
if ($LASTEXITCODE -ne 0 -or $origin -ne "https://github.com/ggml-org/whisper.cpp.git") {
    throw "The local whisper.cpp source does not have the expected official origin."
}
$sourceChanges = git -C $source status --porcelain
if ($LASTEXITCODE -ne 0 -or $sourceChanges) {
    throw "The local whisper.cpp source has uncommitted changes."
}

function Get-VerifiedFile {
    param(
        [Parameter(Mandatory = $true)] [string] $Destination,
        [Parameter(Mandatory = $true)] [string] $Url,
        [Parameter(Mandatory = $true)] [string] $Sha256,
        [Parameter(Mandatory = $true)] [string] $Label
    )

    if (Test-Path -LiteralPath $Destination -PathType Leaf) {
        $actual = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $Sha256) {
            throw "The existing $Label failed its pinned SHA-256 check."
        }
        return $Destination
    }

    $temporary = "$Destination.$([Guid]::NewGuid().ToString('N')).download"
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $temporary
    $actual = (Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Sha256) {
        throw "The downloaded $Label failed its pinned SHA-256 check; the temporary file was retained for inspection."
    }
    Move-Item -LiteralPath $temporary -Destination $Destination
    return $Destination
}

$tinyModel = Get-VerifiedFile `
    -Destination (Join-Path $models "ggml-tiny.bin") `
    -Url "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin" `
    -Sha256 "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21" `
    -Label "Tiny model"

if ($VulkanSdkPath) {
    $vulkanSdk = (Resolve-Path $VulkanSdkPath).Path
}
else {
    $vulkanSdk = Join-Path $scratch "vulkan-sdk-$vulkanSdkVersion"
}
$glslc = Join-Path $vulkanSdk "Bin\glslc.exe"
if (-not (Test-Path -LiteralPath $glslc -PathType Leaf)) {
    if ($VulkanSdkPath) {
        throw "The supplied Vulkan SDK does not contain Bin\glslc.exe."
    }
    $installer = Get-VerifiedFile `
        -Destination (Join-Path $scratch "vulkansdk-windows-X64-$vulkanSdkVersion.exe") `
        -Url "https://sdk.lunarg.com/sdk/download/$vulkanSdkVersion/windows/vulkan_sdk.exe" `
        -Sha256 $vulkanSdkSha256 `
        -Label "Vulkan SDK installer"
    & $installer `
        --root $vulkanSdk `
        --accept-licenses `
        --default-answer `
        --confirm-command `
        install `
        copy_only=1
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $glslc -PathType Leaf)) {
        throw "Unable to copy the pinned Vulkan SDK into the external scratch directory."
    }
}

Add-Type -AssemblyName System.Speech
$synthesizer = [System.Speech.Synthesis.SpeechSynthesizer]::new()
try {
    $synthesizer.Rate = 0
    $synthesizer.SetOutputToWaveFile($fixtureWav)
    $synthesizer.Speak("KokoroKoe verifies Vulkan recovery on Windows.")
}
finally {
    $synthesizer.Dispose()
}

ffmpeg `
    -nostdin `
    -v error `
    -y `
    -i $fixtureWav `
    -ac 1 `
    -ar 16000 `
    -f f32le `
    $fixtureRaw
if ($LASTEXITCODE -ne 0) {
    throw "Unable to decode the generated P3-005 speech fixture."
}
$fixtureBytes = (Get-Item -LiteralPath $fixtureRaw).Length
if ($fixtureBytes -le 0 -or $fixtureBytes -gt (480000 * 4) -or $fixtureBytes % 4 -ne 0) {
    throw "The generated P3-005 fixture violates the bounded f32 utterance contract."
}

$priorVulkanSdk = $env:VULKAN_SDK
$priorPath = $env:PATH
try {
    $env:VULKAN_SDK = $vulkanSdk
    $env:PATH = (Join-Path $vulkanSdk "Bin") + ";" + $priorPath
    cmake `
        -S (Join-Path $repositoryRoot "src-tauri\native\whisper_adapter") `
        -B $build `
        -A x64 `
        "-DWHISPER_CPP_SOURCE_DIR=$source" `
        -DKK_WHISPER_ENABLE_VULKAN=ON `
        -DKK_WHISPER_PROTOTYPE_FAULT_INJECTION=ON
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to configure the P3-005 Vulkan adapter."
    }
    cmake --build $build --config Release --target kokorokoe_whisper_adapter
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to build the P3-005 Vulkan adapter."
    }
}
finally {
    $env:VULKAN_SDK = $priorVulkanSdk
    $env:PATH = $priorPath
}

$adapter = Join-Path $build "bin\Release\kokorokoe_whisper_adapter.dll"
$vulkanBackend = Join-Path $build "bin\Release\ggml-vulkan.dll"
if (-not (Test-Path -LiteralPath $adapter -PathType Leaf) -or
    -not (Test-Path -LiteralPath $vulkanBackend -PathType Leaf)) {
    throw "The built P3-005 adapter or Vulkan backend DLL is unavailable."
}

$env:KOKOROKOE_WHISPER_ADAPTER = $adapter
$env:KOKOROKOE_WHISPER_TINY_MODEL = $tinyModel
$env:KOKOROKOE_P3_005_FIXTURE = $fixtureRaw
$env:KOKOROKOE_WHISPER_THREADS = $Threads.ToString([Globalization.CultureInfo]::InvariantCulture)

cargo test `
    --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
    --locked `
    --target x86_64-pc-windows-msvc `
    transcription::whisper::tests::whisper_vulkan_failure_isolation_gate `
    -- `
    --ignored `
    --exact `
    --nocapture
if ($LASTEXITCODE -ne 0) {
    throw "The P3-005 Vulkan recovery gate failed."
}
