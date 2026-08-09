# Development and CI

KokoroKoe's foundation is verified on Windows 10 22H2 or Windows 11 x64. The checked-in CI workflow uses the Windows Server 2022 runner family, Node 24.19.0, pnpm 10.30.2, and Rust 1.88.0 with the MSVC x64 target.

## Local prerequisites

- Node.js 24.19.0
- pnpm 10.30.2
- Rust 1.88.0 with `x86_64-pc-windows-msvc`, Clippy, and rustfmt
- Visual Studio 2022 Build Tools with the MSVC workload
- Windows SDK and WebView2 Runtime
- `cargo-deny` 0.20.2 and `cargo-audit` 0.22.2 for dependency gates

Install the Rust audit tools once:

```powershell
cargo install cargo-deny --locked --version 0.20.2
cargo install cargo-audit --locked --version 0.22.2
```

## Reproducible verification

Start from a clean checkout and use the lockfiles:

```powershell
pnpm install --frozen-lockfile
pnpm verify:frontend
pnpm verify:rust
pnpm audit:frontend
pnpm licenses:frontend
pnpm audit:rust
```

Build the Windows x64 installers after the checks pass:

```powershell
pnpm tauri build --ci --bundles msi,nsis --target x86_64-pc-windows-msvc -- --locked
```

The generated MSI and NSIS packages are ignored verification artifacts. Do not commit them.

## What the gates cover

- `verify:frontend`: Prettier, ESLint, strict TypeScript, Vitest, the Vite production build, tracked-file policy, immutable GitHub Action pins, and unsafe frontend DOM-injection checks.
- `verify:rust`: rustfmt, Clippy with warnings denied, Rust unit tests, and the Tauri capability tests under `src-tauri/tests`.
- `audit:frontend`: production npm advisory gate at moderate severity or higher.
- `licenses:frontend`: parsed resolved production JavaScript license inventory.
- `audit:rust`: Windows-targeted Cargo license/source/bans policy followed by the RustSec advisory scan.

CI requests read-only repository contents, persists no checkout credential, references no application secret, and publishes no installer. Actions are pinned to full commit SHAs; the adjacent comments record the reviewed release tags. The hosted workflow builds the locked Windows release executable without bundling; the local closeout additionally rebuilds MSI and NSIS installers.

## Secrets and integration tests

`.env` and `.env.*` are ignored except for a future secret-free `.env.example`. Phase 2 requires no OpenRouter key, and CI must never load or print one. OpenRouter work belongs to Phase 5: automated tests should use mock servers by default, while any live provider check must be an explicit local opt-in with secret-canary inspection.

## Sanitized Markdown boundary

All future Markdown-derived UI must use `SanitizedMarkdown`. The component disables raw HTML, applies an explicit sanitize schema, blocks images and unsafe URL schemes, and renders even allowed external links inertly. A later external-link feature must cross a separately authorized Rust command and must not weaken this renderer.

## Windows audio hardware probe

The ordinary locked Rust suite skips the device-dependent P3-001 probe. On a Windows machine with active default input and output endpoints, run it explicitly:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc audio::windows::tests::hardware_probe_enumerates_and_captures_both_default_endpoints -- --ignored --nocapture --test-threads=1
```

The test emits a short local tone and reports only endpoint-format and aggregate capture diagnostics. It writes no audio file. See [P3-001 Windows audio-capture prototype](audio-capture-prototype.md) for the evidence boundary and remaining hardware matrix.

Run the P3-002 processing probe separately to assert that both live sources decode and produce finite exact 160-sample, 16 kHz mono chunks, bounded queue accounting, and throttled level diagnostics:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc --lib audio::windows::tests::hardware_probe_processes_both_default_sources_to_16khz_mono -- --ignored --exact --nocapture
```

It likewise retains no audio. The same probe now also asserts source-local VAD frame accounting and bounded detector/segmenter memory. See [P3-002 bounded audio-processing prototype](audio-processing-prototype.md) for supported native formats and [P3-003 source-local VAD and segmentation prototype](vad-segmentation-prototype.md) for the segmentation contract.

The ordinary suite skips the development-only P3-003 Earshot/Silero comparison. Run the frozen deterministic in-memory bake-off explicitly:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib audio::vad::tests::earshot_meets_frozen_quality_gates_against_silero_v6 -- --ignored --exact --nocapture
```

This command loads Silero's bundled comparison model from the Cargo development dependency and writes no model or audio file to the repository. Its synthetic corpus is a regression gate, not a substitute for the broader consented/licensed real-speech matrix.

## Local Whisper CPU throughput probe

The ordinary suite skips P3-004's model/audio-dependent throughput gate. With FFmpeg, Git, CMake, MSVC, and the local test MP3 available, run:

```powershell
.\scripts\run-whisper-throughput-prototype.ps1
```

The script verifies pinned whisper.cpp source and Tiny/Base SHA-256 values, builds the MIT C adapter, and runs both models CPU-only. All source, DLL, model, decoded-audio, and transcript artifacts remain under `%LOCALAPPDATA%\KokoroKoe\p3-004` or in memory; output contains aggregate metrics only. See [P3-004 bounded local Whisper runtime prototype](whisper-runtime-prototype.md).

## Local Whisper Vulkan recovery probe

The ordinary suite skips P3-005's SDK/model/GPU-dependent failure-isolation gate. With Git, CMake, MSVC, FFmpeg, Windows System Speech, and a Vulkan-capable driver available, run:

```powershell
.\scripts\run-whisper-vulkan-recovery-prototype.ps1
```

The script verifies pinned `whisper.cpp`, Tiny-model, and LunarG SDK hashes; uses the SDK installer in copy-only mode under `%LOCALAPPDATA%\KokoroKoe\p3-005`; generates a short local speech fixture; builds explicit CPU/Vulkan adapter API v2; and runs attested Vulkan success, forced missing-driver startup, native inference-abort isolation, and exact CPU-recovery checks. The first run downloads a roughly 309 MB SDK installer and copies about 2.18 GB of SDK files outside the repository. Use `-VulkanSdkPath` to select an existing SDK. Output contains fixed aggregate status only. See [P3-005 Vulkan recovery prototype](whisper-vulkan-recovery-prototype.md).

## Transcription scheduler and backpressure soak

P3-006 is deterministic, model-free, and part of the ordinary Rust suite. Run its focused tests with:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked transcription::scheduler -- --nocapture
```

The suite first verifies the 5x fake-inference clock, then simulates two hours of alternating one-second final jobs from `microphone` and `system_output`. It checks final priority and chronology, fair source progress, replaceable partial slots, 20-second/10-second partial hysteresis, the 600-second/4,096-job hard backlog bounds, explicit gap accounting, and plateaued logical queued-sample memory. It loads no model, retains no audio, and adds no command, event, capability, UI, or persistence path. See [P3-006 transcription scheduler prototype](transcription-scheduler-prototype.md).

## QPC alignment and source-local recovery gate

P3-007's deterministic two-hour clock gate is part of the ordinary Rust suite. Run it with aggregate output using:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc audio::timeline -- --nocapture
```

On Windows with active default input and render endpoints, run the ignored bounded recovery probe with:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc audio::windows::tests::hardware_probe_recovers_one_source_without_stopping_the_other -- --ignored --exact --nocapture
```

The live test arms one test-only microphone capture-loop failure after both real endpoints are active, then proves the production supervisor retries and closes a durable recovery gap while system-output packets continue. It prints fixed aggregate counters only and retains no audio. See [P3-007 QPC alignment and Windows recovery prototype](audio-clock-recovery-prototype.md).

## Supervised Vulkan worker protocol and lifecycle gate

P3-008's strict codec, lazy CPU ordering, cancellation, fixed-error, and result-validation tests are part of the ordinary Rust suite. Run them alone with:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc transcription::worker -- --nocapture
```

After the external P3-005 adapter, verified Tiny model, and generated fixture exist, run the exact native protocol/lifecycle matrix with:

```powershell
.\scripts\run-whisper-worker-protocol-prototype.ps1
```

The runner verifies the external Tiny-model hash and fixture bound, builds the debug KokoroKoe worker executable, and tests Vulkan attestation, missing-driver startup, malformed hello/response frames, blocked writes, hung and nonzero-exit inference, cancellation, Job Object descendant cleanup, and exact lazy CPU recovery. Output is fixed aggregate status only. Debug fault modes are not compiled into release builds. See [P3-008 supervised Vulkan worker protocol prototype](whisper-worker-protocol-prototype.md).

## P3-009 live transcription integration gate

Run the deterministic Rust-only integration matrix with:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc transcription::integration -- --nocapture
```

This covers conservative source-local VAD ordering frontiers, delayed older finals, chronological exact-once result delivery, the existing backlog ceilings, balanced two-source progress under a two-hour 5x service-pressure simulation, pause/resume/stop draining, fixed inference and cancellation gaps, source isolation, and supervised-worker-to-CPU recovery ordering. It uses no external model, device, audio file, command, capability, UI, persistence, or network access. See [P3-009 live transcription integration prototype](live-transcription-integration-prototype.md).

## P3-010 curated model installation gate

The ordinary locked Rust suite uses a fake bounded download source and tiny generated bytes to test catalog pinning, clean/idempotent installation, range resume, restart when a server ignores a range, cancellation/resume, corruption recovery, disk/memory diagnostics, selection/deletion, and path containment. It contacts no network and writes only to test temporary directories.

Run the exact external hash gate against the already verified P3-004 Tiny and Base files with:

```powershell
.\scripts\run-whisper-model-installation-prototype.ps1
```

Use `-ModelDirectory` if those files live elsewhere. The command reads and hashes the two external files but neither copies them into the repository nor downloads replacements. See [P3-010 Whisper model management prototype](whisper-model-management-prototype.md).
