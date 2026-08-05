# KokoroKoe

KokoroKoe is a privacy-first Windows meeting assistant under active development. The current repository checkpoint contains the Tauri 2, React, strict TypeScript, Tailwind CSS, and shadcn/ui foundation only.

Audio capture, local transcription, Markdown persistence, OpenRouter integration, and advanced desktop windows are not implemented yet.

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
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
pnpm build

cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features
```

See [the architecture](docs/architecture.md) and [project memory](docs/project-memory.md) for scope, decisions, and the active handoff.
