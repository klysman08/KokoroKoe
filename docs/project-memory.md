# KokoroKoe Project Memory

This checked-in ledger coordinates tasks and handoffs. Do not store secrets, API keys, transcript content, audio, model weights, or personal meeting data here.

## Current phase

- Phase: 3 - Audio and transcription
- Status: In progress
- Started: 2026-08-08
- Objective: Prove the Windows audio, clock, format, VAD, local-transcription, acceleration-fallback, and backpressure gates before building the product transcription experience.

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
| P2-002 | Coordinator + review agents | Completed | Sanitized tracing/errors, strict Rust/Zod settings boundary, read-only `get_settings`, TanStack Query integration, root/query error UI, and least-privilege command ACL | P2-001 | Shared fixtures parse in Rust/TypeScript; semantic validation matches; raw errors/tokens/paths do not reach logs or reports; only `main` can invoke the command; builds/audits/package smoke pass | 15 Rust tests; 27 Vitest tests; Node 24 format/lint/type/build; generated ACL/static capability checks; secret/path canaries; cargo-deny/audit and pnpm audit; MSI/NSIS rebuild; responding-process smoke; two independent reviews with blockers fixed and rechecked |
| P2-003 | Coordinator + review agents | Completed | Versioned non-secret SQLite settings persistence, validated workspace selection/write-health probe, optimistic settings updates, strict mutation contracts, onboarding UI, and least-privilege commands | P2-002 | Corrupt-record recovery, traversal/ADS/reparse denial, optimistic-revision conflicts, Rust/Zod fixture parity, denied-window checks, audits, package build, and Windows onboarding smoke pass | 37 Rust tests; 51 Vitest tests; Node/Rust quality gates and audits; MSI/NSIS rebuild; packaged picker/update/restart smoke; three independent reviews with no remaining blockers |
| P2-004 | Coordinator + review agents | Completed | Sanitized raw-HTML-disabled Markdown rendering boundary, locked Windows CI, reproducible verification commands, and Phase 2 acceptance/handoff | P2-003 | Untrusted Markdown cannot execute HTML/script, load images, or navigate through unsafe links; Windows CI covers locked frontend/Rust format, lint, type, unit, capability, audit, and desktop build checks; Phase 2 evidence and limitations are independently reviewed | 55 Vitest tests; 37 Rust/capability tests; Node/Rust format/lint/type/build; clean production audit; license inventory; Cargo deny/audit; actionlint; locked MSI/NSIS build; packaged Settings smoke; three independent reviews with no remaining blockers |

## Phase 3 task table

| ID | Owner | Status | Scope | Dependencies | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| P3-001 | Coordinator | Completed | Bounded Windows WASAPI prototype: active endpoint/default-role enumeration, simultaneous shared-mode event-driven microphone and render-loopback capture, QPC-derived session timestamps, independent channel supervision/retry, bounded packet queues, and native-format/health diagnostics | P2-004; ADR 0002; Windows 10 22H2/11 x64; active capture and render endpoints | Deterministic queue/timeline/supervisor tests pass; an explicit Windows hardware probe captures both sources concurrently without retaining samples, reports monotonic QPC-derived timestamps and native formats, bounds memory with visible drop/discontinuity counters, and demonstrates that one channel failure/retry does not stop the other; dependency, capability, privacy, and scope checks pass | 70 Vitest tests; 45 ordinary Rust tests plus 3 capability tests; explicit dual-source hardware probe passed; zero timestamp regressions or queue drops; locked frontend/Rust quality gates, audits, and x64 release build passed |
| P3-002 | Coordinator | Completed | Bounded Rust audio-processing prototype: validate and decode supported native PCM/float packets, sanitize non-finite samples, downmix each source independently, anti-aliased resample to 16 kHz mono, derive throttled RMS/peak/clipping diagnostics, and pass normalized chunks through bounded per-source queues | P3-001; ADR 0002; `rubato` API/license/MSRV verification; deterministic format fixtures | In-memory fixtures cover unsigned 8-bit and signed 16/24/32-bit PCM including left-aligned valid bits, 32/64-bit float and non-finite input, mono/stereo/multichannel downmix, rate conversion and anti-aliasing, level windows, format changes, queue pressure, malformed packets, and source isolation; an explicit Windows probe processes both live sources without retaining samples; full checks, audits, scope review, and release build pass | 72 Vitest tests; 54 ordinary Rust tests plus 3 capability tests; explicit dual-source normalization probe passed on 44.1/48 kHz stereo-float endpoints with zero processing errors or queue drops; locked Node/Rust checks, audits, and x64 release build passed |

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
| 2026-08-05 | Serialize command failures as `{ error: AppError }` | Freezes one strict Rust/Zod rejection envelope without exposing raw exceptions or provider/source errors |
| 2026-08-05 | Restrict `get_settings` through both Tauri ACL and an exact Rust `main` window-label check | Tauri application commands are otherwise available to local windows by default; defense in depth prevents future windows from inheriting access accidentally |
| 2026-08-05 | Keep foundation settings read-only and query-cached only while the Settings view is mounted | P2-002 validates the asynchronous boundary without inventing persistence, copying settings into Zustand, or retaining the workspace path in a persistent cache |
| 2026-08-05 | Use fixed crate-filtered stderr tracing with safe categorical fields only | Prevent dependency/payload tracing and raw source details from entering logs; file logging remains unimplemented |
| 2026-08-08 | Store non-secret settings as an append-only SQLite history retaining three validated revisions | Optimistic mutations, monotonic recovery revisions, and fallback from semantically invalid records avoid stale-write ABA behavior |
| 2026-08-08 | Lazily initialize and access SQLite only in blocking workers | Tauri setup remains responsive and can show sanitized command errors when storage is corrupt, busy, inaccessible, or from a future schema |
| 2026-08-08 | Quarantine and rebuild the non-secret settings database only for SQLite corruption/not-a-database failures | Preserve damaged files for diagnosis while keeping future-schema, busy, permission, and ordinary I/O failures fail-closed without destructive recovery |
| 2026-08-08 | Restrict Windows MVP workspaces to existing fixed local drive-letter directories without reparse points | Establish a conservative Rust-owned boundary; ADR 0005 records the deliberate rejection of network, removable, redirected, and reparse-backed folders |
| 2026-08-08 | Keep workspace selection in a single-flight Rust native picker with no frontend path or dialog capability | React cannot submit an arbitrary path, and cancellation or concurrent selection leaves settings unchanged |
| 2026-08-08 | Route all future Markdown-derived React UI through the fixed `SanitizedMarkdown` boundary | Raw HTML, images, unsafe schemes, and active navigation remain unavailable; ADR 0006 records the policy and Phase 4 input-size responsibility |
| 2026-08-08 | Use one read-only, full-SHA-pinned Windows 2022 CI workflow for the Phase 2 gates | Match the Windows/MSVC product boundary without adding secrets, publishing, signing, or non-Windows claims; local MSI/NSIS smoke remains separate evidence |
| 2026-08-08 | Use `wasapi` 0.23 with separate bounded Crossbeam packet queues for the Phase 3 capture prototype | Keep unsafe Core Audio details behind a maintained Rust wrapper while ensuring neither source blocks the other or grows memory without bound |
| 2026-08-08 | Re-resolve default-role endpoints during capture while fixed selections retry only their stored endpoint ID | Implements ADR 0002 without silently changing a user's explicit selection |
| 2026-08-08 | Normalize each source in its own Rust worker through `rubato` 4.0 asynchronous sinc resampling and a separate bounded 10 ms output queue | Keeps sample data and compute outside React, preserves source isolation, provides anti-aliasing, and makes downstream backpressure visible without blocking capture |
| 2026-08-08 | Use equal-weight downmix and 100 ms aggregate level windows for P3-002 | Establishes a deterministic bounded baseline while leaving speaker-mask weighting and product level events as explicit future decisions |

## Prototype and risk register

| ID | Gate or risk | Resolution rule | Target phase | Status |
| --- | --- | --- | --- | --- |
| R-001 | WASAPI device/format/recovery matrix | Unaffected channel continues; all gaps and retries are visible | 3 | Live 44.1/48 kHz stereo-float processing and deterministic PCM/float/rate/channel conversion proven by P3-001/P3-002; broader live matrix and recovery remain open |
| R-002 | QPC clock alignment and long-run drift | Less than 20 ms calculated error over two hours, excluding physical latency | 3 | Short-run monotonicity proven by P3-001; two-hour gate open |
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

### P2-002 - Typed settings and sanitized error boundary

- Completed: 2026-08-05
- Deliverables: Rust `AppSettings`, `AppError`, and `{ error: AppError }` command contracts; exact semantic Rust deserialization checks matching strict Zod schemas; synthetic shared settings/error fixtures; read-only `get_settings`; Tauri app-command manifest and generated permission; exact `main` capability plus Rust-side window authorization; fixed-filter sanitized tracing; React 19 root error callbacks; root and query error surfaces with explicit sanitized-report copying; TanStack Query used only for the settings command; and a read-only privacy-default preview that does not render the machine-specific workspace path.
- Privacy defaults verified: LLM analysis and audio retention are off; zero-data-retention providers are required; data-collecting providers are denied. No settings persistence, filesystem mutation, HTTP, credential, audio, project/session, or additional-window behavior was added.
- Graphify evidence: Graphify 0.9.33 generated an ignored 262-node/309-edge local code graph and identified the existing settings/shell/Tauri bootstrap change surface. No semantic API key was configured, so `--code-only` was used and checked-in architecture/memory remained the documentation source.
- Verification:
  - Node 24.19.0: `pnpm format:check`, `pnpm lint`, `pnpm typecheck`, `pnpm test` (27 passed), and `pnpm build` passed.
  - Rust 1.88.0: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` passed (12 unit plus 3 capability tests).
  - Shared fixtures are parsed and round-tripped by Rust and strict Zod tests; adversarial cases cover unsafe revisions, empty/bounded strings, invalid UUIDs, zero token limits, malformed fixed decimals, unknown keys, enum casing, and nullable optional detail.
  - Secret/path tests cover Rust technical-detail sanitization, captured tracing output, malformed IPC success/rejection data, React render exceptions, copied reports, and clipboard denial.
  - Tauri tests assert only `main`, only `allow-get-settings`, no remote capability, the generated command mapping, and Rust denial for transcript/insights/unknown labels.
  - `pnpm audit --prod --audit-level moderate` found no known vulnerabilities; production license inventory completed; Cargo deny passed; Cargo audit returned success with the same 17 documented transitive warnings.
  - `pnpm tauri build --target x86_64-pc-windows-msvc` rebuilt MSI and NSIS packages; the release executable remained alive and responsive for the five-second hidden launch smoke.
  - Independent frontend/security reviews found Rust/Zod semantic and React 19 default-console blockers; both were fixed with mirrored validation and explicit root callbacks, then rechecked.
- Known limitations: settings are defaults only and are not persisted or editable; the actual workspace path is returned only to the trusted main window and held in a non-persisted query while mounted; capability denial is unit/static tested rather than invoked from a second real WebView because no second window exists yet; technical-detail redaction is defense in depth rather than permission to pass raw source errors; the packaged smoke does not automate Settings navigation or clipboard interaction; sanitized Markdown rendering and CI are still absent.

### P2-003 - Versioned settings persistence and workspace onboarding

- Completed: 2026-08-08
- Deliverables: lazily opened bundled SQLite settings store; append-only three-revision history; optimistic non-secret settings mutations; monotonic recovery after empty or semantically invalid history; one-shot quarantine/rebuild for physical SQLite corruption; native Rust-only single-flight workspace picker; conservative fixed-volume Windows path validation; ancestor reparse/junction rejection; canonicalization and create/write/sync/delete/free-space probe; strict mirrored Rust/Zod mutation and workspace-status contracts; exact `main`-only command ACL plus Rust authorization; TanStack Query mutation/cache reconciliation; workspace onboarding and privacy-settings UI; explicit confirmation for privacy-default relaxation; recording-law and folder-sync notice; ADR 0005; updated dependency and privacy documentation.
- Graphify evidence: the ignored local graph was refreshed after implementation with Graphify 0.9.33 in code-only mode and contains 582 nodes, 1,072 edges, and 33 communities.
- Verification:
  - Node 24.19.0 and pnpm 10.30.2: `pnpm format:check`, `pnpm lint`, `pnpm typecheck`, `pnpm test` (9 files, 51 tests), `pnpm build`, and `pnpm audit --prod --audit-level moderate` passed.
  - Rust 1.88.0: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed (34 unit plus 3 capability tests).
  - Persistence tests cover concurrent first migration, future-schema preservation, independent-service revision races, same-value updates, semantic-record recovery, monotonic empty-history recovery, non-database quarantine, and valid-header/settings-table-page corruption quarantine and rebuild.
  - Workspace tests cover relative, drive-relative, root, UNC, verbatim/device, traversal, alternate-data-stream, slash, reserved-name, control-character, non-fixed-volume, and ancestor-junction rejection plus write-probe cleanup.
  - Shared Rust/Zod fixtures and adversarial tests cover strict patches, missing/unsafe revisions, explicit `null`, unknown keys, empty updates, bounded decimals/strings, workspace status, malformed success data, and structured command errors.
  - Capability tests assert only `get_settings`, `update_settings`, and `choose_workspace`; only the exact `main` window; no remote URLs or generic dialog/filesystem/HTTP/shell/process permissions; and Rust authorization occurs before store or picker side effects.
  - `cargo deny --manifest-path src-tauri/Cargo.toml check licenses bans sources` passed. `cargo audit --file src-tauri/Cargo.lock` returned success with the same 17 documented allowed transitive maintenance/non-Windows GTK warnings. Production dependency/license inventory passed.
  - `pnpm tauri build --bundles msi,nsis` rebuilt the x64 MSI and NSIS packages from the final source.
  - Packaged Windows UI smoke opened Settings, verified the default Documents workspace, cancelled the native picker without mutation, selected the valid default folder and observed revision/free-space status, changed the token limit from 2048 to 2049, restarted the executable and observed revision 3/value 2049, restored 2048 at revision 4, and closed the process cleanly.
  - Independent frontend/contracts, persistence/concurrency, and security reviews reported no remaining blockers after migration-race, future-schema, corruption-routing, workspace validation, optimistic-cache, cancellation, and ACL findings were corrected.
- Known limitations: this database contains only non-secret application settings; important project/session content remains future Markdown work. The fixed-local-volume policy intentionally rejects network, removable, redirected, OneDrive/reparse-backed, and some enterprise folders. A successful probe is point-in-time evidence, so Phase 4 must pin/revalidate directory identity and containment for every sensitive write. Physical settings corruption is quarantined and rebuilt rather than repaired or exposed through a recovery UI. Denied-window behavior is still static/unit tested because no second product WebView exists. Sanitized Markdown rendering and CI are not yet implemented.

### P2-004 - Sanitized Markdown, Windows CI, and Phase 2 closeout

- Completed: 2026-08-08
- Deliverables: fixed `SanitizedMarkdown` React boundary; CommonMark/GFM parsing; raw-HTML suppression; explicit sanitize schema; image, embedded-content, DOM-injection, and unsafe-scheme denial; inert external links; adversarial tests; exact Markdown dependency locks and license record; read-only Windows 2022 GitHub Actions workflow with full commit-SHA pins; locked Node/pnpm/Rust/audit/build commands; repository policy and parsed frontend license scripts; development/CI guide; ADR 0006; architecture and privacy updates; Phase 2 acceptance handoff.
- Security behavior: the renderer accepts an in-memory string and presentation class only. Feature code cannot replace parser plugins, element policy, sanitizer, or URL transform. Raw HTML, script/style/SVG/object/iframe/form/input, event-handler attributes, raw and Markdown images, relative/protocol-relative links, and JavaScript/data/VBScript/file schemes are excluded. Even allowed `http`, `https`, and `mailto` links render as inert text. No Tauri command, capability, filesystem read, external HTTP client, credential access, OpenRouter feature, project/session schema, audio code, model code, or additional window was added.
- Graphify evidence: the ignored code graph was refreshed after implementation and contains 770 nodes, 1,258 edges, and 46 communities. Graphify reported five JSON fixtures with no structural nodes, the existing missing optional SQL parser, and community-label drift after the incremental rebuild; checked-in fixtures and migration tests remain the authoritative evidence for those files, and graph labels are non-product local tooling output.
- Verification:
  - Node 24.19.0 and pnpm 10.30.2: `pnpm install --frozen-lockfile`, `pnpm verify:frontend`, `pnpm audit:frontend`, and `pnpm licenses:frontend` passed. Prettier, ESLint, strict TypeScript, 10 Vitest files/55 tests, Vite production build, repository policy, a clean production audit, and a parsed 159-package production license inventory passed.
  - Renderer tests cover supported headings/emphasis/lists/tables/task-list handling, raw executable and embedded elements, raw/Markdown images, inert navigation, mixed-case JavaScript, VBScript, file, data, protocol-relative, relative, and control-character addresses.
  - Rust 1.88.0: `pnpm verify:rust` passed locked MSVC Clippy with warnings denied and 34 unit plus 3 capability tests. `pnpm audit:rust` passed Cargo licenses, bans, and sources; RustSec returned success with the same 17 documented allowed transitive maintenance/non-Windows GTK warnings.
  - `actionlint` 1.7.12 passed after its release archive matched SHA-256 `cdc8643b2c8dc890c76ad16095da97e75f86572805cc3573cc13f31ea0f19127`. Action tag mappings were independently checked; the annotated pnpm v6.0.9 tag was corrected to peeled commit `0ebf47130e4866e96fce0953f49152a61190b271`.
  - `pnpm tauri build --ci --bundles msi,nsis --target x86_64-pc-windows-msvc -- --locked` rebuilt `KokoroKoe_0.1.0_x64_en-US.msi` and `KokoroKoe_0.1.0_x64-setup.exe` from final source.
  - Packaged Windows smoke launched the release executable, confirmed the process was responsive, opened Settings through UI Automation, observed workspace onboarding and local privacy defaults, and closed cleanly.
  - Frozen-lockfile reinstall, local Markdown-link resolution, ignored/untracked `.env` verification without reading it, secret-pattern scan, `git diff --check`, scope review, and three independent final reviews passed with no remaining blockers.
- Known limitations: the GitHub workflow is checked, actionlint-clean, and locally mirrored but has not been observed on a hosted runner because the branch has not been pushed. The hosted image is mutable, and a cold run under the 60-minute timeout remains unproven. CI builds the locked release executable; the installer build and UI smoke are local closeout gates. Frontend license automation proves a nonempty parsed inventory but does not yet enforce an SPDX allowlist or generate distribution notices. Markdown parsing is synchronous, so Phase 4 must bound and paginate document input before React receives it. Raw HTML, images, active links, relative links, and task-list checkboxes remain deliberately unavailable. No project/session Markdown loading or persistence exists yet.

### Phase 2 - Foundation completion

- Completed: 2026-08-08
- Result: the Windows x64 Tauri/React foundation now has a strict typed shell, shadcn/Tailwind UI, local UI state and routing, sanitized error/log boundaries, least-privilege commands/capabilities, versioned non-secret settings persistence, conservative native workspace onboarding, a sanitized Markdown rendering boundary, reproducible local gates, Windows CI configuration, audits, and verified MSI/NSIS packaging.
- Deferred by design: all WASAPI capture, audio processing, VAD, local transcription, model management/downloads, project/session schemas and Markdown persistence, FTS/search, Credential Manager/OpenRouter, insights/summaries, extra windows, opacity/shortcuts, and release signing/telemetry remain in their assigned later phases.

### P3-001 - Bounded Windows WASAPI capture prototype

- Completed: 2026-08-08
- Deliverables: active input/render endpoint and default-role enumeration; strict Rust/Zod device-selection and diagnostic contracts; four exact main-window Tauri commands; explicit capture-consent acknowledgement; simultaneous event-driven shared-mode microphone and render-loopback threads; native mix-format reporting; one shared QPC-derived millisecond epoch; source-local health, retry, and default-endpoint re-resolution; separate bounded nonblocking packet queues; aggregate drop, discontinuity, timestamp, packet, frame, and consumption counters; no sample persistence or frontend audio delivery; implementation, privacy, dependency, and hardware-probe documentation.
- Verification:
  - Node 24.14.0 and pnpm 10.30.2: `pnpm verify:frontend` passed Prettier, ESLint, strict TypeScript, 12 Vitest files/70 tests, Vite production build, and repository policy; `pnpm audit:frontend` found no known production vulnerability.
  - Rust 1.88.0: `pnpm verify:rust` passed locked MSVC rustfmt, Clippy with warnings denied, 45 ordinary tests, and 3 capability tests; one explicitly hardware-dependent test remains ignored in the ordinary suite.
  - The opt-in hardware probe passed on the current Windows machine with concurrent 44.1 kHz stereo-float microphone and 48 kHz stereo-float render-loopback capture. Both streams used one capture attempt, consumed every captured packet, and reported zero queue drops, timestamp errors, or timestamp regressions. One initial discontinuity per stream remained visible in diagnostics.
  - `pnpm audit:rust` passed Cargo licenses, bans, and sources; RustSec returned success with the same 17 documented allowed transitive maintenance/non-Windows GTK warnings.
  - `pnpm tauri build --ci --no-bundle --target x86_64-pc-windows-msvc -- --locked` produced the final locked release executable.
  - Final source review, scope review, local-link/secret/artifact checks, `git diff --check`, and `git status --short --branch` passed without retained audio, models, databases, logs, or build artifacts entering the worktree.
- Known limitations: the live evidence covers one default microphone/render pair and a short run only. Real unplug/hotplug, fixed-device removal, default switches, Windows Audio restart, Bluetooth, docks, USB, virtual devices, Remote Desktop, protected/exclusive audio, PCM integer and broader channel/sample-rate formats, and the two-hour QPC drift target remain open Phase 3 gates. P3-001 does not normalize, resample, meter, run VAD, retain audio, transcribe, persist meetings, or provide product capture UI.

### P3-002 - Bounded audio-processing prototype

- Completed: 2026-08-08
- Deliverables: strict native-format and packet-size validation; unsigned 8-bit and signed little-endian 16/24/32-bit PCM decoding with left-aligned valid-bit handling; 32/64-bit float decoding with non-finite substitution; finite/clamped equal-weight mono downmix; source-local `rubato` asynchronous sinc pipelines; exact 160-sample 16 kHz output chunks; independent bounded normalized queues with fair discard sink and visible drops; 100 ms RMS/peak/clipping windows; aggregate Rust/Zod status counters; one shared golden status fixture; dependency, privacy, development, and prototype-boundary documentation; no new command, capability, UI, event, persistence, model, or network path.
- Verification:
  - Node 24.19.0 and pnpm 10.30.2: `pnpm verify:frontend` passed Prettier, ESLint, strict TypeScript, 12 Vitest files/72 tests, Vite production build, and repository policy. The shared processing-status fixture parses under strict Rust and Zod contracts. Production audit found no known vulnerability, and the 159-package production license inventory parsed.
  - Rust 1.88.0: `pnpm verify:rust` passed locked MSVC rustfmt, Clippy with warnings denied, 54 ordinary tests, and 3 capability tests; two device-dependent probes remain explicitly ignored in the ordinary suite.
  - Generated in-memory tests cover 8/16/24/32-bit PCM, reduced left-aligned valid bits, 32/64-bit float, NaN, mono/stereo/four-channel downmix, malformed alignment and packet lengths, 8/16/22.05/44.1/48/96 kHz conversion, exact finite/clamped output chunks, attenuation above the 8 kHz target Nyquist limit, 10 Hz levels, clipping, format changes, source mismatch, bounded queue pressure, and per-source isolation. No audio fixture or recording is checked in.
  - The exact opt-in Windows processing probe passed with concurrent 44.1 kHz stereo-float microphone and 48 kHz stereo-float render loopback. Both produced and consumed exact 160-sample normalized chunks, emitted level windows, and reported zero capture or processing queue drops, processing errors, non-finite substitutions, and timestamp regressions. Only aggregate metadata/counters were printed; samples were discarded.
  - `pnpm audit:rust` passed Cargo licenses, bans, and sources. RustSec returned success with the same 17 documented allowed maintenance/non-Windows GTK warnings. `rubato` 4.0.0 and its seven locked Rust-only packages introduced no advisory failure or DLL.
  - `pnpm tauri build --ci --no-bundle --target x86_64-pc-windows-msvc -- --locked` produced the final locked x64 release executable. Final formatting, policy, local-link, secret-pattern, artifact, and diff checks passed without retained audio, models, databases, logs, or build artifacts entering the worktree.
- Graphify evidence: the existing ignored graph predates P3-001/P3-002 and contains no processing/resampling vocabulary. A code-graph traversal anchored the Manifest audio flow and ADR 0002; query expansion used `audio`, `format`, `capture`, `windows`, and `source`. The missing local Graphify Python environment prevented a refresh/save, so checked-in architecture, tests, and this task ledger remain authoritative.
- Known limitations: live evidence remains one short stereo-float endpoint pair; deterministic conversion coverage is not a driver/device matrix. Equal-weight downmix ignores channel-mask speaker weights. Reported sinc startup delay is not timestamp-compensated, and partial native/normalized buffers plus filter tail are discarded on format change or stop. The normalized sink intentionally discards samples. Device recovery, two-hour clock drift, VAD, retained audio, Whisper/model management, inference/backpressure, transcript events/UI, and session persistence remain open.

## Known environment facts

- The machine-wide shell still defaults to Node 26.5.1; P2 verification initializes `fnm` and uses the repository-pinned Node 24.19.0.
- pnpm 10.30.2, Cargo 1.88.0, and Rust 1.88.0 are installed.
- Visual Studio 2022 Build Tools 17.14, Windows SDK 10.0.26100.0, and CMake 4.4.1 are installed and verified.
- Vulkan 1.4 is available with an NVIDIA GeForce RTX 3070 Ti.
- CPU-only and non-Vulkan Windows machines remain required test targets.

## Next handoff

Begin `P3-003`, a source-local VAD and utterance-segmentation bake-off. Define a small licensed/generated deterministic speech/noise/silence corpus and approved miss/false-positive thresholds; compare Earshot with Silero; then implement bounded per-source pre-roll, trailing-silence finalization, and forced splitting only if the winning approach satisfies R-003. Keep Whisper/model downloads, inference scheduling, retained audio, project/session persistence, OpenRouter, product transcript UI, and additional windows out of P3-003. The broader live device/recovery matrix and two-hour clock gate remain open.
