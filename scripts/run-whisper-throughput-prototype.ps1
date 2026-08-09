param(
    [string] $AudioPath = (Join-Path $PSScriptRoot "..\public\audio_test\OpenAI.mp3"),
    [string] $ScratchPath = (Join-Path ([Environment]::GetFolderPath("LocalApplicationData")) "KokoroKoe\p3-004"),
    [ValidateRange(1, 64)]
    [int] $Threads = [Math]::Min([Environment]::ProcessorCount, 8)
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$audio = (Resolve-Path $AudioPath).Path
$scratch = [System.IO.Path]::GetFullPath($ScratchPath)
$repositoryPrefix = $repositoryRoot.TrimEnd("\") + "\"
if ($scratch.Equals([System.IO.Path]::GetPathRoot($scratch), [StringComparison]::OrdinalIgnoreCase) -or
    $scratch.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "The P3-004 scratch directory must be a dedicated path outside the repository."
}
$source = Join-Path $scratch "whisper.cpp-v1.9.2"
$build = Join-Path $scratch "adapter-build"
$models = Join-Path $scratch "models"
$pinnedCommit = "306c88f4d1286aec1bf96e544632897886af5501"

New-Item -ItemType Directory -Force -Path $scratch, $models | Out-Null

if (-not (Test-Path -LiteralPath $source -PathType Container)) {
    git clone --branch v1.9.2 --depth 1 https://github.com/ggml-org/whisper.cpp.git $source
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to clone the pinned whisper.cpp source."
    }
}
$actualCommit = (git -C $source rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $actualCommit -ne $pinnedCommit) {
    throw "The local whisper.cpp source does not match the P3-004 pinned commit."
}
$origin = (git -C $source remote get-url origin).Trim().TrimEnd("/")
if ($LASTEXITCODE -ne 0 -or $origin -ne "https://github.com/ggml-org/whisper.cpp.git") {
    throw "The local whisper.cpp source does not have the expected official origin."
}
$sourceChanges = git -C $source status --porcelain
if ($LASTEXITCODE -ne 0 -or $sourceChanges) {
    throw "The local whisper.cpp source has uncommitted changes."
}

function Get-VerifiedModel {
    param(
        [Parameter(Mandatory = $true)] [string] $Name,
        [Parameter(Mandatory = $true)] [string] $Sha256
    )

    $destination = Join-Path $models "ggml-$Name.bin"
    if (Test-Path -LiteralPath $destination -PathType Leaf) {
        $actual = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $Sha256) {
            throw "The existing $Name model failed its pinned SHA-256 check."
        }
        return $destination
    }

    $temporary = "$destination.$([Guid]::NewGuid().ToString('N')).download"
    $url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-$Name.bin"
    Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $temporary
    $actual = (Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Sha256) {
        throw "The downloaded $Name model failed its pinned SHA-256 check; the temporary file was retained for inspection."
    }
    Move-Item -LiteralPath $temporary -Destination $destination
    return $destination
}

$tinyModel = Get-VerifiedModel -Name "tiny" -Sha256 "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21"
$baseModel = Get-VerifiedModel -Name "base" -Sha256 "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"

cmake `
    -S (Join-Path $repositoryRoot "src-tauri\native\whisper_adapter") `
    -B $build `
    -A x64 `
    "-DWHISPER_CPP_SOURCE_DIR=$source"
if ($LASTEXITCODE -ne 0) {
    throw "Unable to configure the P3-004 native adapter."
}
cmake --build $build --config Release --target kokorokoe_whisper_adapter
if ($LASTEXITCODE -ne 0) {
    throw "Unable to build the P3-004 native adapter."
}

$adapter = Join-Path $build "bin\Release\kokorokoe_whisper_adapter.dll"
if (-not (Test-Path -LiteralPath $adapter -PathType Leaf)) {
    throw "The built P3-004 adapter DLL is unavailable."
}

$env:KOKOROKOE_WHISPER_ADAPTER = $adapter
$env:KOKOROKOE_WHISPER_TINY_MODEL = $tinyModel
$env:KOKOROKOE_WHISPER_BASE_MODEL = $baseModel
$env:KOKOROKOE_TRANSCRIPTION_AUDIO = $audio
$env:KOKOROKOE_WHISPER_THREADS = $Threads.ToString([Globalization.CultureInfo]::InvariantCulture)

cargo test `
    --manifest-path (Join-Path $repositoryRoot "src-tauri\Cargo.toml") `
    --locked `
    --target x86_64-pc-windows-msvc `
    transcription::whisper::tests::whisper_cpu_throughput_gate `
    -- `
    --ignored `
    --exact `
    --nocapture
if ($LASTEXITCODE -ne 0) {
    throw "The P3-004 CPU throughput gate failed."
}
