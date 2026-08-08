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

It likewise retains no audio. See [P3-002 bounded audio-processing prototype](audio-processing-prototype.md) for supported native formats and processing limitations.
