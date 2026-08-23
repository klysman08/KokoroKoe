# KokoroKoe

KokoroKoe is a privacy-first Windows meeting assistant under active development. Audio never leaves
the machine: capture, transcription, and storage are local, and only bounded transcript text is sent
to OpenRouter, and only when you ask for it.

## What works today

- **Dual-source capture.** Windows WASAPI capture of the microphone and system output as separate
  channels, with normalization, source-local VAD, utterance segmentation, QPC clock alignment, and
  visible gaps whenever a source drops or recovers.
- **Local transcription.** Whisper inference through a project-owned C ABI shim over pinned MIT
  whisper.cpp v1.9.2, running in a supervised Vulkan worker with a lazy exact CPU fallback, plus a
  prioritized scheduler with bounded queues and backpressure. Partial and final results are labelled
  by source and ordered chronologically across both channels.
- **Curated model management.** A closed catalog of three SHA-256-pinned Whisper models — Tiny,
  Base, and Large-v3 Turbo Q5_0 (multilingual) — with resumable verified downloads, disk checks, and
  deletion, all owned by Rust.
- **Projects and sessions.** Markdown is the source of truth: pinned `project.md` and `session.md`
  snapshots with YAML front matter, a durable append-only journal, incremental transcript
  materialization, interrupted-session recovery, and a rebuildable SQLite index for discovery and
  full-text search.
- **OpenRouter integration.** The API key lives only in Windows Credential Manager and is never
  returned to the frontend. Rust owns validation, a privacy-filtered zero-data-retention model
  catalog, per-role model selection, versioned prompts, conservative cost reservation with
  reconciliation against a per-Session spending limit, bounded retries, and strict schema validation
  with a single JSON repair attempt.
- **Questions and insights.** Ask a question about a saved transcript segment, or generate insights
  over the recent transcript of a running Session. Both are explicit, per-request user actions that
  return transient, validated, typed results with their token and cost accounting.
- **Final session summary.** Once a Session is finished, generate an executive summary with main
  topics, decisions, action items (with owners and deadlines), risks, open questions, and next
  steps. It is written to `summary.md` beside the transcript as portable Markdown, states how much
  of the transcript it covered, and can be regenerated.
- **Detached transcript window.** Pop the live transcript into its own window during a Session, with
  background opacity, always-on-top, and compact mode. Opacity dims the background only, so text
  stays fully readable, and it cannot be lowered past a readable floor. Position, size, and
  appearance are remembered between runs, and a window left on a display that is later disconnected
  returns to a centered default instead of reopening off-screen. A configurable system-wide shortcut
  hides and shows it for quick-hide during a meeting, and clicks can be made to pass straight
  through it so it sits over another application without getting in the way. Click-through never
  survives a restart, cannot be switched on without the main window, and switches off when the main
  window closes, so pointer control is always recoverable. The window is created by Rust and holds
  an event-subscription-only capability: it can invoke no command and read no file.

## Not implemented yet

- The rolling in-session summary that accumulates while a Session runs; only the final summary
  exists today.
- An explicit monitor picker. Position, size, appearance, and the show/hide shortcut are already
  remembered between runs.
- The separate insights window, and the per-segment transcript actions from the Manifest.
- A packaged installer. Native whisper.cpp runtimes and model weights are external, unbundled inputs
  staged beside the executable by `scripts/prepare-poc-transcription-runtime.ps1`.

Verification so far covers Windows 11 x64 on a single RTX 3070 Ti / Ryzen 7 3700X host. Windows 10,
AMD and Intel GPUs, CPU-only minimum hardware, and broad real-speech accuracy across speakers,
languages, accents, and noise remain unverified.

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

Dependency audits, production-license inventory, installer builds, CI parity, and the secret-handling
policy are documented in [Development and CI](docs/development-and-ci.md).

See [the architecture](docs/architecture.md) and [project memory](docs/project-memory.md) for scope,
decisions, and the active handoff. [Privacy and limitations](docs/privacy-and-limitations.md)
records what leaves the machine and what is still unverified.

The focused P3-010 Rust-only model gate is documented in
[Whisper model management prototype](docs/whisper-model-management-prototype.md).
