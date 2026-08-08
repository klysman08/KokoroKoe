# Dependency and License Baseline

Status: Phase 1 candidate assessment plus the resolved Phase 2 foundation inventory, updated on 2026-08-08. `pnpm-lock.yaml` and `src-tauri/Cargo.lock` pin the resolved dependencies. Future candidates remain unresolved until the phase that first adds them.

## P2-001 resolved foundation inventory

| Area | Resolved direct version | License | Notes |
| --- | ---: | --- | --- |
| Desktop runtime | `tauri` 2.11.5 | MIT OR Apache-2.0 | No optional features, commands, or plugins enabled |
| Desktop build | `tauri-build` 2.6.3, Tauri CLI 2.11.4 | MIT OR Apache-2.0 | Windows x64 MSI and NSIS bundles verified |
| UI runtime | React/React DOM 19.2.8 | MIT | Bundled into the WebView frontend |
| UI primitives | Base UI 1.7.0, shadcn CLI/source 4.16.1 | MIT | shadcn is a development dependency; generated components are repository source |
| Styling | Tailwind CSS and Vite plugin 4.3.3, `tw-animate-css` 1.4.0 | MIT | Tailwind v4 Vite workflow |
| State | Zustand 5.0.14 | MIT | UI-only shell/navigation state in P2-001 |
| Icons/font | Lucide React 1.28.0, Inter variable 5.3.0 | ISC; OFL-1.1 | Inter is self-hosted in the bundle |
| Frontend build | Vite 7.3.6, TypeScript 5.8.3 | MIT; Apache-2.0 | Node 24-compatible versions resolved from the official scaffold |
| Frontend tests | Vitest 4.1.10, Testing Library React 16.3.2 | MIT | jsdom-based route and shell tests |

## P2-002 resolved foundation-services inventory

| Area | Resolved direct version | License | Notes |
| --- | ---: | --- | --- |
| Frontend IPC | `@tauri-apps/api` 2.11.1 | MIT OR Apache-2.0 | Only `invoke("get_settings")`; no generic plugin capability |
| Async command cache | TanStack Query 5.101.4 | MIT | Used only for the read-only settings command; no persistence or devtools |
| Boundary validation | Zod 4.4.3 | MIT | Strict settings and command-error transport schemas |
| Rust serialization | Serde 1.0.229; `serde_json` 1.0.151 for tests | MIT OR Apache-2.0 | Camel-case DTOs and shared golden-fixture verification |
| Rust IDs | UUID 1.24.0 | MIT OR Apache-2.0 | UUID v4 correlation IDs and typed preset ID |
| Rust tracing | `tracing` 0.1.44; `tracing-subscriber` 0.3.23 | MIT | Fixed crate-only filters; safe categorical fields; no file sink |

## P2-003 resolved settings-persistence inventory

| Area | Resolved direct version | License | Notes |
| --- | ---: | --- | --- |
| Settings database | `rusqlite` 0.40.2 with bundled SQLite | MIT; SQLite public domain | Rust-only append-only settings revisions; no external SQLite DLL |
| Native folder picker | `rfd` 0.17.2 | MIT | Called only from Rust; no frontend dialog API or capability |
| Workspace probe | `tempfile` 3.27.0; `fs2` 0.4.3 | MIT OR Apache-2.0 | Create/write/sync/delete health probe and available-space query |
| Windows volume policy | `windows-sys` 0.61.2 | MIT OR Apache-2.0 | Fixed-volume classification with the minimum FileSystem feature |
| Windows security tests | `junction` 2.0.0 (development only) | MIT OR Apache-2.0 | Creates an unprivileged NTFS junction to verify ancestor rejection |

## P2-004 resolved rendering inventory

| Area | Resolved direct version | License | Notes |
| --- | ---: | --- | --- |
| Markdown-to-React | `react-markdown` 10.1.0 | MIT | Raw HTML is skipped; no DOM injection API is used |
| GFM parsing | `remark-gfm` 4.0.1 | MIT | Tables, strikethrough, autolinks, and task-list syntax are parsed before sanitization |
| Render-tree sanitization | `rehype-sanitize` 6.0.0 | MIT | Explicit tag, attribute, and protocol schema; images are excluded and links remain inert |

These packages are pure JavaScript and add no native library, DLL, Tauri command, external-network client, or frontend capability. The renderer accepts an in-memory string only; reading Markdown files remains Rust-owned Phase 4 work.

## P3-001 resolved Windows audio-prototype inventory

| Area | Resolved direct version | License | Notes |
| --- | ---: | --- | --- |
| WASAPI wrapper | `wasapi` 0.23.0 | MIT | Windows-only safe wrapper over Core Audio; shared/event capture, render-loopback, endpoint enumeration, native formats, packet QPC timestamps |
| Bounded packet queues | `crossbeam-channel` 0.5.15 | MIT OR Apache-2.0 | Separate fixed-capacity queue per audio source; capture uses nonblocking `try_send` and visible drops |
| Windows API projection | `windows` 0.62.2 (transitive through `wasapi`) | MIT OR Apache-2.0 | Adds a second Windows projection version beside Tauri's 0.61 line; no extra DLL is distributed |
| QPC/test-tone APIs | `windows-sys` 0.61.2 (existing direct dependency, expanded features) | MIT OR Apache-2.0 | Adds only performance-counter APIs and the test-only kernel `Beep` binding |

`cargo deny check licenses bans sources` passes with the new dependency graph. `cargo audit` returns success with the same 17 allowed maintenance/non-Windows GTK warnings; P3-001 introduced no new RustSec advisory. The lockfile's additional duplicate `windows` support crates are accepted prototype overhead from `wasapi` 0.23.0 and must be reconsidered if a later upgrade aligns with Tauri's projection version.

P2-003 audit evidence:

- `pnpm audit --prod --audit-level moderate`: no known vulnerabilities.
- `pnpm licenses list --prod --json`: completed successfully; new production packages are MIT or MIT/Apache dual-licensed.
- `cargo deny --manifest-path src-tauri/Cargo.toml check licenses bans sources`: licenses, bans, and sources passed; duplicate transitive crates remain warnings.
- `cargo audit --file src-tauri/Cargo.lock`: returned success with the same 17 allowed transitive maintenance/non-Windows GTK warnings documented for P2-001.

React Router was evaluated and removed. The available 7.18.2 release was affected by GHSA-qwww-vcr4-c8h2, while the patched 8.3.0 release reported by the advisory was not available from npm. The two-route foundation instead uses a typed local hash store with no routing dependency.

P2-001 audit evidence:

- `pnpm audit --prod --audit-level moderate`: no known vulnerabilities.
- `pnpm licenses list --prod --json`: production licenses are permissive or notice-based; Inter is OFL-1.1 and Lightning CSS is MPL-2.0.
- `cargo deny --manifest-path src-tauri/Cargo.toml check licenses bans sources`: licenses, bans, and sources passed for `x86_64-pc-windows-msvc`; duplicate transitive Tauri crates remain warnings.
- `cargo audit --file src-tauri/Cargo.lock`: no vulnerability failure; it reports unmaintained transitive crates and one unsound `glib` advisory from Tauri's non-Windows GTK dependency set. `glib` is not in the configured Windows target graph used by `cargo deny` or the packaged Windows binary.

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
| Database | `rusqlite` | 0.40.2 | MIT | Resolved with bundled SQLite for settings in P2-003; future FTS5 projection uses the same Rust-only boundary |
| HTTP | `reqwest` | 0.13.4 | MIT OR Apache-2.0 | Rust-only OpenRouter/download client |
| SSE | `eventsource-stream` | 0.2.3 | MIT OR Apache-2.0 | OpenRouter streaming parser |
| Credentials | `keyring` Windows backend | 4.1.x | MIT OR Apache-2.0 | Windows Credential Manager adapter |
| YAML | `serde_yaml_ng` | 0.10.0 | MIT | Markdown front matter |
| Audio files | `hound` | Resolve in Phase 3 | Apache-2.0 | Optional WAV retention |
| Integrity | `sha2`, `crc32fast` | Resolve in implementation phase | MIT OR Apache-2.0 | Downloads and recovery journal |
| Serialization | `serde` 1.0.229, `serde_json` 1.0.151 | Resolved in P2-002 | MIT OR Apache-2.0 | Rust contracts and JSON boundaries; `serde_json` is currently test-only |
| IDs and time | `uuid` 1.24.0; time library unresolved | UUID resolved in P2-002 | MIT OR Apache-2.0 | Correlation and preset IDs now; RFC 3339 time support remains future work |
| Logging/errors | `tracing` 0.1.44, `tracing-subscriber` 0.3.23; `thiserror` unresolved | Tracing resolved in P2-002 | MIT; MIT OR Apache-2.0 | Sanitized structured logs and typed errors |
| Safe files/secrets | `tempfile` 3.27.0; `zeroize` unresolved | `tempfile` resolved in P2-003 | MIT OR Apache-2.0 | Workspace health probe now; atomic staging and secret zeroization remain phase-specific |

`whisper-rs` uses the Unlicense while `whisper.cpp` is MIT. Before accepting `whisper-rs` for distribution, record explicit project/license approval. If rejected, implement a minimal adapter to the MIT `whisper.cpp` C API without copying Handy.

Model catalog entries must retain the model license, source repository and revision, exact byte count, SHA-256, supported languages, and attribution. Downloaded model weights are not covered by the application license.

The initial catalog contains multilingual Tiny and Base. Base is only the provisional default; Phase 3 throughput results decide whether Base or Tiny is the shipped default.

## Frontend and testing candidates

| Area | Candidate baseline | License |
| --- | --- | --- |
| React 19.2.8, Vite 8.2.0 | React resolved; Vite candidate superseded by locked 7.3.6 for the scaffold | MIT |
| TypeScript 7.0.2 | Candidate superseded by locked 5.8.3 for the scaffold | Apache-2.0 |
| Tailwind CSS 4.3.3, shadcn 4.16.1 | Resolved in P2-001; generated component code remains repository source | MIT |
| Zustand 5.0.14, TanStack Query 5.101.4, Zod 4.4.3 | Resolved through P2-002 | MIT |
| React Router 8.3.0 | Candidate; resolve Vite/Tauri compatibility in Phase 2 | MIT |
| `react-markdown` 10.1.0, `remark-gfm` 4.0.1, `rehype-sanitize` 6.0.0 | Candidate; raw HTML remains disabled | MIT |
| Vitest 4.1.10 and Testing Library React 16.3.2 | Candidate; resolve in Phase 2 | MIT |
| Playwright 1.62.1 | Candidate; resolve in Phase 2 | Apache-2.0 |

Use TanStack Query for asynchronous command/cache workflows only; keep short-lived desktop/UI state in Zustand.

## Toolchain baseline and gaps

- Target: `x86_64-pc-windows-msvc`, Windows 10 22H2 and Windows 11.
- Pin Node 24 LTS, pnpm 10, and Rust 1.88 or newer.
- Local pnpm and Rust are present.
- Visual Studio 2022 Build Tools 17.14 with the VCTools workload, Windows SDK 10.0.26100.0, CMake 4.4.1, and `fnm` 1.39.0 with Node 24.19.0 were installed for P2-001. WebView2 Runtime 151.0.4129.59 was already present.
- Vulkan 1.4 and an NVIDIA RTX 3070 Ti are available locally, but CPU-only and non-Vulkan machines remain mandatory test targets.

## Policy and automated gates

Automated policy in the foundation now includes:

- `cargo deny check licenses bans sources` using the checked-in Windows-targeted `deny.toml`.
- `cargo audit` against the Rust lockfile.
- `pnpm audit` and `pnpm licenses list` against the JavaScript lockfile.

A distributable third-party notices document remains required before a release is published. The P2-001 installers are verification artifacts only and are ignored by Git.

Review every new production dependency for API currency, maintenance, license, native/DLL requirements, privacy impact, and whether an existing dependency already provides the capability. Do not add GPL/AGPL dependencies or model terms without explicit approval.

Handy is MIT-licensed and may inform user experience and technical research. KokoroKoe must not copy its source or architecture; any future copied material would require attribution and license-notice compliance.
