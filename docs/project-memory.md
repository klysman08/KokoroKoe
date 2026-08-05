# KokoroKoe Project Memory

This checked-in ledger coordinates tasks and handoffs. Do not store secrets, API keys, transcript content, audio, model weights, or personal meeting data here.

## Current phase

- Phase: 1 - Architecture and coordination
- Status: Completed
- Started: 2026-08-05
- Completed: 2026-08-05
- Objective: Materialize and independently validate the approved architecture, coordination workflow, dependency assessment, privacy limitations, and repository skill before product scaffolding.

## Completed task table - Phase 1

| ID | Owner | Status | Scope | Dependencies | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P1-001 | Coordinator | Completed | Initialize, author, and validate the repository development skill and its references | Approved plan | Repository skill initialized, customized, and structurally valid | `init_skill.py`; regenerated `agents/openai.yaml`; `quick_validate.py`: `Skill is valid!` |
| P1-002 | Coordinator | Completed | Create architecture, ADR, dependency/license, privacy/limitations, memory, and repository-instruction artifacts | Approved plan | Phase 1 documents define structure, models, interfaces, strategies, risks, dependencies, and decisions | Required-artifact check passed; local Markdown links resolve; no placeholders/false lockfile claims |
| P1-003 | Coordinator + review agents | Completed | Forward-test normal/security skill behavior and audit Phase 1 documents against the Manifest | P1-001, P1-002 | Independent reviews have no unresolved blocking findings | Phase 2 task forward-test passed; unsafe React/OpenRouter forward-test rejected correctly; independent audit findings fixed; final re-audit: no blockers |
| P1-004 | Coordinator | Completed | Apply findings, run final checks, and record the Phase 1 handoff | P1-003 | Final checks pass; memory and `AGENTS.md` record Phase 1 completion | Skill, artifact, link, placeholder, secret-pattern, whitespace, fence, and Git checks passed on 2026-08-05 |

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

## Known environment facts

- Repository initially contained only `Manifest.md` on clean `master`.
- Node 26.5.1, pnpm 10.30.2, Cargo 1.88.0, and Rust 1.88.0 are installed.
- MSVC Build Tools and CMake were not detected from the current shell.
- Vulkan 1.4 is available with an NVIDIA GeForce RTX 3070 Ti.
- CPU-only and non-Vulkan Windows machines remain required test targets.

## Next handoff

Begin `P2-001`, a bounded Windows Tauri/React application-scaffold task. First install or expose MSVC C++ Build Tools, the Windows SDK, WebView2, and CMake; use Node 24 LTS with pnpm 10 and Rust 1.88+. Then scaffold Tauri 2, Vite, React, strict TypeScript, Tailwind, shadcn/ui, a minimal main-window shell, an explicit minimal capability, and baseline formatting/lint/type/test/audit commands. Keep audio, persistence, OpenRouter, credentials, production settings, and advanced windows out of `P2-001`.
