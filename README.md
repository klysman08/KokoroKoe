# KokoroKoe

KokoroKoe is a privacy-first Windows meeting assistant under active development. The current repository checkpoint contains the Tauri 2/React foundation plus bounded Windows audio capture, normalization, VAD, utterance segmentation, local Whisper CPU throughput, a supervised Vulkan worker protocol with lazy CPU recovery, transcription-scheduler/backpressure, cross-source live-pipeline ordering, long-run clock/source-recovery, and curated verified Whisper model-installation prototypes.

Product capture controls, local transcription, retained audio, Markdown persistence, OpenRouter integration, and advanced desktop windows are not implemented yet. Prototype audio samples remain in bounded Rust memory and are discarded; the frontend receives only aggregate diagnostics. The foundation includes a reusable inert renderer for future untrusted Markdown content; it does not read project or session files yet.

## Prerequisites

- Windows 10 22H2 or Windows 11 x64
- Node.js 24 LTS
- pnpm 10
- Rust 1.88+ with the `x86_64-pc-windows-msvc` target
- Microsoft Visual C++ Build Tools and Windows SDK
- Microsoft Edge WebView2 Runtime

## Development

```powershell
pnpm install --frozen-lockfile
pnpm dev
```

Run the desktop shell:

```powershell
pnpm tauri dev
```

## Verification

```powershell
pnpm verify:frontend
pnpm verify:rust
```

Dependency audits, production-license inventory, installer builds, CI parity, and the secret-handling policy are documented in [Development and CI](docs/development-and-ci.md).

See [the architecture](docs/architecture.md) and [project memory](docs/project-memory.md) for scope, decisions, and the active handoff.

The focused P3-010 Rust-only model gate is documented in [Whisper model management prototype](docs/whisper-model-management-prototype.md).
