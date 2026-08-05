# KokoroKoe Project Memory

This checked-in ledger coordinates tasks and handoffs. Do not store secrets, API keys, transcript content, audio, model weights, or personal meeting data here.

## Current phase

- Phase: 2 - Foundation
- Status: In progress
- Started: 2026-08-05
- Objective: Establish the secure, typed, tested Windows desktop foundation before adding sensitive services or product data.

## Completed task table - Phase 1

| ID | Owner | Status | Scope | Dependencies | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P1-001 | Coordinator | Completed | Initialize, author, and validate the repository development skill and its references | Approved plan | Repository skill initialized, customized, and structurally valid | `init_skill.py`; regenerated `agents/openai.yaml`; `quick_validate.py`: `Skill is valid!` |
| P1-002 | Coordinator | Completed | Create architecture, ADR, dependency/license, privacy/limitations, memory, and repository-instruction artifacts | Approved plan | Phase 1 documents define structure, models, interfaces, strategies, risks, dependencies, and decisions | Required-artifact check passed; local Markdown links resolve; no placeholders/false lockfile claims |
| P1-003 | Coordinator + review agents | Completed | Forward-test normal/security skill behavior and audit Phase 1 documents against the Manifest | P1-001, P1-002 | Independent reviews have no unresolved blocking findings | Phase 2 task forward-test passed; unsafe React/OpenRouter forward-test rejected correctly; independent audit findings fixed; final re-audit: no blockers |
| P1-004 | Coordinator | Completed | Apply findings, run final checks, and record the Phase 1 handoff | P1-003 | Final checks pass; memory and `AGENTS.md` record Phase 1 completion | Skill, artifact, link, placeholder, secret-pattern, whitespace, fence, and Git checks passed on 2026-08-05 |

## Phase 2 task table

| ID | Owner | Status | Scope | Dependencies | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P2-001 | Coordinator + review agents | Completed | Windows Tauri/React scaffold, strict frontend toolchain, shadcn/Tailwind shell, UI-only state/routing, least-privilege capability, audits, package build, and smoke launch | P1-004; Node 24; MSVC/SDK; CMake; WebView2 | Clean locked install and frontend/Rust checks; explicit main capability with no permissions; x64 bundles and launch smoke pass | Node 24/pnpm 10 checks; 2 Vitest tests; Vite build; Rust fmt/Clippy/test; clean pnpm audit; cargo audit; cargo-deny; MSI/NSIS build; responding-process smoke; two independent read-only reviews |

## Confirmed decisions

| Date | Decision | Rationale |
| --- | --- | --- |
| 2026-08-05 | Target Windows 10 22H2 and Windows 11 x64 | Fixed MVP boundary; ARM64 and other OSes remain future adapters |
| 2026-08-05 | Keep the repository skill in `.agents/skills/kokorokoe-development` | Shareable, versioned workflow for contributors and sub-agents |
| 2026-08-05 | Use this checked-in memory rather than user-local generated memory | Auditable coordination and handoff record |
| 2026-08-05 | Keep Rust as the sensitive-operation trust boundary | Prevent React from accessing audio, arbitrary files, credentials, models, or external APIs |
| 2026-08-05 | Use independent shared-mode WASAPI microphone and render-loopback capture | Required Windows device control, loopback, recovery, and timestamps |
| 2026-08-05 | Use QPC packet timestamps for one session timeline | Align independent endpoints without callback-arrival assumptions |
| 2026-08-05 | Keep Markdown as portable truth and SQLite as a rebuildable projection | Human-readable export plus fast local loading/search |
| 2026-08-05 | Start with multilingual Whisper Tiny and Base; Base is the provisional default | Keep Base as default only if the Phase 3 minimum-hardware throughput gate passes; otherwise select Tiny |
| 2026-08-05 | Use one prioritized model-owning inference worker | Limit memory while protecting final transcription from partial work |
| 2026-08-05 | Keep LLM and audio retention disabled by default | Privacy-first behavior and explicit user control |
| 2026-08-05 | Use Windows Credential Manager for the OpenRouter key | Meets the OS-backed secret-storage requirement |
| 2026-08-05 | Use FTS5 plus recency for MVP retrieval | Avoid a second local embedding model while limiting transcript resend |
| 2026-08-05 | Use shadcn preset `b0` with the current Vite/Tailwind v4 workflow | The official CLI decoded `b0` successfully and generated the selected Nova/Base UI foundation |
| 2026-08-05 | Keep P2-001's main-window capability at zero permissions | The static shell needs no commands, plugins, filesystem, network, process, or credential access |
| 2026-08-05 | Use a typed local hash-navigation store for the two foundation routes | Removes React Router while its available releases are covered by unresolved advisories; revisit only when a patched version is published and needed |
| 2026-08-05 | Keep TanStack Query and Zod out of P2-001 | No asynchronous command/cache or external contract exists yet; add them with the first typed Rust command boundary where they provide value |
| 2026-08-05 | Install Graphify globally as a Codex skill | User-requested local codebase graph tooling; installed through the official `graphifyy` CLI at version 0.9.33 |

## Prototype and risk register

| ID | Gate or risk | Resolution rule | Target phase | Status |
| --- | --- | --- | --- | --- |
| R-001 | WASAPI device/format/recovery matrix | Unaffected channel continues; all gaps and retries are visible | 3 | Pending |
| R-002 | QPC clock alignment and long-run drift | Less than 20 ms calculated error over two hours, excluding physical latency | 3 | Pending |
| R-003 | Earshot versus Silero VAD | Keep Earshot only if it meets the approved miss/false-positive thresholds and remains within two points of Silero | 3 | Pending |
| R-004 | Tiny/Base real-time throughput | Aggregate real-time factor below 1.0 on a supported configuration; target final p95 below two seconds | 3 | Pending |
| R-005 | Vulkan failure isolation | Use in-process fallback only if missing/broken drivers cannot prevent startup; otherwise use a supervised worker | 3 | Pending |
| R-006 | `whisper-rs` Unlicense review | Record approval or use a minimal MIT `whisper.cpp` C API adapter | 3 | Pending |
| R-007 | Journal and atomic replacement | Fault injection must preserve every acknowledged final segment | 4 | Pending |
| R-008 | Credential Manager and secret containment | Development and packaged canary tests find no secret outside the OS store | 5 | Pending |
| R-009 | OpenRouter privacy/cost/error behavior | Mock and controlled tests cover ZDR, typed errors, SSE, cancellation, and budgets | 5 | Pending |
| R-010 | Transparent/click-through windows | Preserve readable text; omit click-through if emergency recovery is unreliable | 6 | Pending |

## Completed checkpoints

### Phase 1 - Architecture and coordination

- Completed: 2026-08-05
- Deliverables: repository skill and metadata, phase/verification references, root instructions, architecture and textual component/data-flow design, source tree, core data and Tauri contracts, four ADRs, dependency/license assessment, privacy/consent/limitations record, prototype gates, and this task ledger.
- Skill forward-test: produced a bounded Phase 2 scaffold task with correct dependencies and exclusions.
- Security forward-test: rejected a direct React/localStorage/OpenRouter implementation and preserved the Rust/Credential Manager boundary.
- Independent audit: initially identified missing source tree, incomplete contracts, false lockfile wording, task schema mismatch, provisional-default inconsistency, and credential wording. All findings were corrected; final audit reported no Phase 1 blockers.
- Verification:
  - `python C:\Users\klysm\.codex\skills\.system\skill-creator\scripts\quick_validate.py .agents\skills\kokorokoe-development`
  - required Phase 1 artifact existence check
  - local Markdown link resolution check
  - placeholder/false lockfile claim scan
  - API-key/bearer-token pattern scan
  - trailing-whitespace and balanced-code-fence checks
  - `git diff --check`
  - `git status --short --branch`
- Result: all checks passed. Files remain intentionally uncommitted and unstaged for user review.
- Known limitations: this phase contains documentation and coordination assets only; no Tauri application exists yet. Native MSVC/CMake prerequisites and Node 24 pinning remain Phase 2 prerequisites. Every audio, model, persistence, OpenRouter, credential, window, performance, and packaging prototype gate remains pending in its assigned phase.

### P2-001 - Windows Tauri/React application scaffold

- Completed: 2026-08-05
- Deliverables: Tauri 2.11 Windows x64 application, React 19/strict TypeScript/Vite foundation, Node 24 and pnpm 10 pins, Tailwind CSS v4 with shadcn preset `b0`, generated Button/Card/Badge/Separator components, branded Home and Settings placeholder routes, UI-only Zustand navigation state, restricted CSP, zero-permission main capability, custom Windows icon, ESLint/Prettier/Vitest configuration, Rust and pnpm lockfiles, Cargo deny policy, and development README.
- Prerequisites installed or verified: `fnm` 1.39.0 with Node 24.19.0, Visual Studio 2022 Build Tools 17.14 VCTools workload, Windows SDK 10.0.26100.0, CMake 4.4.1, Rust/Cargo 1.88.0 MSVC target, pnpm 10.30.2, and WebView2 Runtime 151.0.4129.59.
- Official initialization evidence: `create-tauri-app` 4.6.2 with React TypeScript/Tauri 2; Tailwind v4 Vite plugin; `shadcn preset decode b0`; `shadcn init --preset b0 --template vite`; shadcn 4.16.1.
- Verification:
  - Node 24.19.0: `pnpm install --frozen-lockfile`, `pnpm format:check`, `pnpm lint`, `pnpm typecheck`, `pnpm test` (2 passed), `pnpm build`, and `pnpm audit --prod --audit-level moderate` all passed.
  - Rust 1.88.0: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets --all-features` (1 passed) all passed.
  - `cargo deny --manifest-path src-tauri/Cargo.toml check licenses bans sources` passed for `x86_64-pc-windows-msvc`.
  - `cargo audit --file src-tauri/Cargo.lock` returned success with no vulnerability failure; it reports unmaintained transitive crates and one non-Windows GTK `glib` unsoundness warning documented in `docs/dependency-licenses.md`.
  - `pnpm tauri build --target x86_64-pc-windows-msvc` produced MSI and NSIS bundles; the release executable remained alive and responsive for the five-second launch smoke test.
  - Independent frontend review passed under Node 24; independent security review found no generic frontend filesystem, HTTP, shell, process, credential, Tauri bridge, browser storage, or external navigation access.
- Process note: the generator's `--force` option removed clean tracked planning files despite its narrow description. Git status detected the deletion immediately and the unchanged files were restored from `HEAD` before work continued. Future scaffolds must run in a staging directory and merge intentionally.
- Known limitations: P2-001 does not implement settings persistence, sanitized logging, error boundaries/reports, Zod/Tauri contracts, TanStack Query, Markdown rendering, CI, audio, transcription, projects, sessions, persistence, LLMs, or advanced windows. The application icon is a foundation asset, not an approved final brand. The built MSI/NSIS files are ignored verification artifacts, not release candidates.

## Known environment facts

- The machine-wide shell still defaults to Node 26.5.1; P2 verification initializes `fnm` and uses the repository-pinned Node 24.19.0.
- pnpm 10.30.2, Cargo 1.88.0, and Rust 1.88.0 are installed.
- Visual Studio 2022 Build Tools 17.14, Windows SDK 10.0.26100.0, and CMake 4.4.1 are installed and verified.
- Vulkan 1.4 is available with an NVIDIA GeForce RTX 3070 Ti.
- CPU-only and non-Vulkan Windows machines remain required test targets.

## Next handoff

Begin `P2-002`, a bounded foundation-services task. Add sanitized Rust tracing and typed `AppError` handling, an error boundary and sanitized copy-report UI, the first narrow read-only bootstrap/settings Tauri command, mirrored strict Zod validation with a shared golden JSON fixture, and TanStack Query only for that asynchronous command. Keep workspace mutation, production settings persistence, audio, transcription, projects/sessions, OpenRouter, credentials, and additional native windows out of `P2-002`. Require Rust/TypeScript fixture tests and a per-window capability/authorization review before acceptance.
