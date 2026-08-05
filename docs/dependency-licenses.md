# Dependency and License Baseline

Status: Phase 1 candidate assessment, queried from crates.io/npm metadata and upstream documentation on 2026-08-05. The repository has no application manifests or lockfiles yet. Exact compatible versions and the complete transitive inventory will be resolved and recorded during Phase 2 or the phase that first adds the dependency.

## Core Rust dependencies

| Area | Candidate | Baseline | License | Decision |
| --- | --- | ---: | --- | --- |
| Desktop | `tauri` | 2.11.5 | MIT OR Apache-2.0 | Required |
| Windows audio | `wasapi` | 0.23.0 | MIT | Preferred WASAPI wrapper |
| Windows APIs | `windows` | 0.62.x | MIT OR Apache-2.0 | QPC and missing Core Audio APIs |
| Transcription | `whisper-rs` | 0.16.0 | Unlicense | Conditional on license and prototype review |
| Native inference | `whisper.cpp` | pinned commit | MIT | Required underlying engine |
| Resampling | `rubato` | 4.0.0 | MIT OR Apache-2.0 | Required |
| VAD | `earshot` | 1.2.1 | MIT OR Apache-2.0 | Initial candidate; compare with Silero |
| Runtime | `tokio`, `tokio-util` | Resolve in Phase 2 | MIT | Required asynchronous services/cancellation |
| Worker queues | `crossbeam-channel` | Resolve in Phase 3 | MIT OR Apache-2.0 | Bounded native worker queues |
| Database | `rusqlite` | 0.40.1 | MIT | Bundled SQLite/FTS5 projection |
| HTTP | `reqwest` | 0.13.4 | MIT OR Apache-2.0 | Rust-only OpenRouter/download client |
| SSE | `eventsource-stream` | 0.2.3 | MIT OR Apache-2.0 | OpenRouter streaming parser |
| Credentials | `keyring` Windows backend | 4.1.x | MIT OR Apache-2.0 | Windows Credential Manager adapter |
| YAML | `serde_yaml_ng` | 0.10.0 | MIT | Markdown front matter |
| Audio files | `hound` | Resolve in Phase 3 | Apache-2.0 | Optional WAV retention |
| Integrity | `sha2`, `crc32fast` | Resolve in implementation phase | MIT OR Apache-2.0 | Downloads and recovery journal |
| Serialization | `serde`, `serde_json` | Resolve in Phase 2 | MIT OR Apache-2.0 | Rust contracts and JSON boundaries |
| IDs and time | `uuid`, `time` or `chrono` | Resolve in Phase 2 | MIT OR Apache-2.0 | Stable IDs and RFC 3339 UTC values |
| Logging/errors | `tracing`, `thiserror` | Resolve in Phase 2 | MIT; MIT OR Apache-2.0 | Sanitized structured logs and errors |
| Safe files/secrets | `tempfile`, `zeroize` | Resolve in implementation phase | MIT OR Apache-2.0 | Atomic staging and secret zeroization |

`whisper-rs` uses the Unlicense while `whisper.cpp` is MIT. Before accepting `whisper-rs` for distribution, record explicit project/license approval. If rejected, implement a minimal adapter to the MIT `whisper.cpp` C API without copying Handy.

Model catalog entries must retain the model license, source repository and revision, exact byte count, SHA-256, supported languages, and attribution. Downloaded model weights are not covered by the application license.

The initial catalog contains multilingual Tiny and Base. Base is only the provisional default; Phase 3 throughput results decide whether Base or Tiny is the shipped default.

## Frontend and testing candidates

| Area | Candidate baseline | License |
| --- | --- | --- |
| React 19.2.8, Vite 8.2.0 | Candidate; resolve in Phase 2 | MIT |
| TypeScript 7.0.2 | Candidate; resolve compatibility in Phase 2 | Apache-2.0 |
| Tailwind CSS 4.3.3, shadcn 4.16.1 | Candidate; generated component code remains repository source | MIT |
| Zustand 5.0.14, TanStack Query 5.101.4, Zod 4.4.3 | Candidate; resolve in Phase 2 | MIT |
| React Router 8.3.0 | Candidate; resolve Vite/Tauri compatibility in Phase 2 | MIT |
| `react-markdown` 10.1.0, `remark-gfm` 4.0.1, `rehype-sanitize` 6.0.0 | Candidate; raw HTML remains disabled | MIT |
| Vitest 4.1.10 and Testing Library React 16.3.2 | Candidate; resolve in Phase 2 | MIT |
| Playwright 1.62.1 | Candidate; resolve in Phase 2 | Apache-2.0 |

Use TanStack Query for asynchronous command/cache workflows only; keep short-lived desktop/UI state in Zustand.

## Toolchain baseline and gaps

- Target: `x86_64-pc-windows-msvc`, Windows 10 22H2 and Windows 11.
- Pin Node 24 LTS, pnpm 10, and Rust 1.88 or newer.
- Local pnpm and Rust are present.
- MSVC C++ Build Tools, Windows SDK integration, and CMake were not detected in the current shell and must be installed before native Tauri/Whisper work.
- Vulkan 1.4 and an NVIDIA RTX 3070 Ti are available locally, but CPU-only and non-Vulkan machines remain mandatory test targets.

## Policy and automated gates

During Phase 2, add and run:

- `cargo deny check licenses bans sources`
- `cargo audit`
- `pnpm audit`
- a generated production dependency/license inventory
- a checked-in third-party notices document before packaging

Review every new production dependency for API currency, maintenance, license, native/DLL requirements, privacy impact, and whether an existing dependency already provides the capability. Do not add GPL/AGPL dependencies or model terms without explicit approval.

Handy is MIT-licensed and may inform user experience and technical research. KokoroKoe must not copy its source or architecture; any future copied material would require attribution and license-notice compliance.
