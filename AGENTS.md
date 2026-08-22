# KokoroKoe Repository Instructions

## Sources of truth

Read these before making changes:

1. `Manifest.md` for product requirements and scope.
2. `docs/architecture.md` and accepted ADRs for technical decisions.
3. `docs/project-memory.md` for the active task, dependencies, evidence, and handoff.
4. `.agents/skills/kokorokoe-development/SKILL.md` for the task workflow.

When sources conflict, explicit user instructions win, followed by `Manifest.md`, accepted ADRs, architecture, and project memory. Record any intentional decision change in a new ADR.

## Working agreements

- Work one bounded task in the active phase; do not implement the entire MVP in one change.
- Keep the Windows 10 22H2/Windows 11 x64 MVP boundary unless the user changes it.
- Use pnpm, strict TypeScript, Tauri 2, Rust, React, Tailwind, shadcn/ui, Zustand, Zod, and SQLite as defined in the architecture.
- Keep sensitive audio, filesystem, credential, model, and external-network operations in Rust.
- Never send audio to OpenRouter. React may hold a user-entered API key only in transient component memory long enough to invoke `set_openrouter_api_key`; never persist, log, echo, cache, or place it in global state. Rust must never return the stored key or expose it to Markdown, SQLite, events, reports, or logs.
- Keep `microphone` and `system_output` as stable stored source identifiers.
- Keep Markdown as portable source-of-truth content; treat SQLite as a rebuildable index/cache.
- Treat transcript text, model output, paths, and rendered Markdown as untrusted input.
- Use minimum Tauri permissions and no generic frontend filesystem, HTTP, shell, or process capability.
- Preserve unrelated user changes and never use destructive Git commands to discard them.
- Never run a scaffold generator with `--force` in the repository root. Generate in a verified staging directory and merge reviewed files intentionally.
- Do not commit secrets, user transcripts, retained audio, downloaded models, databases, logs, or build artifacts.

## Task and sub-agent workflow

- Every task needs an ID, owner, scope, dependencies, acceptance criteria, and verification evidence in `docs/project-memory.md`.
- Delegate only independent, bounded work. Sub-agents must not edit `AGENTS.md` or `docs/project-memory.md`.
- The coordinating agent reviews delegated work, runs final checks, and owns task completion records.
- A task is complete only when its acceptance criteria pass. Failed or unavailable checks must be recorded as limitations, not silently skipped.

## Completion protocol

After every accepted task:

1. Update the task and evidence in `docs/project-memory.md`.
2. Move durable architectural changes into an ADR or stable instruction.
3. Replace the rolling checkpoint below with the completed task ID, date, outcome, verification commands, and memory link.
4. Report completed work, how to test it, known limitations, and the recommended next task.

## Latest completed checkpoint

- Task: P5-015 - Stronger multilingual transcription and Session language
- Date: 2026-08-20
- Outcome: added one immutable verified Large-v3 Turbo Q5_0 multilingual model; advanced the project adapter to API v3 and worker protocol to v2; and applies each frozen Session's normalized BCP-47 primary language identically to supervised Vulkan and lazy exact CPU fallback while transient transcription retains auto-detection.
- Verification: the exact external 574,041,195-byte model/quality gate passed with Vulkan load 3.269 s, 0.378 s inference/RTF 0.0812, 18.518 s CPU load plus recovery, zero unaccounted finals, and 1,087,459,328-byte peak process-tree working set. Protocol adversarial and two-hour backpressure gates passed. Locked frontend checks passed 34 Vitest files/162 tests; locked Rust checks passed 288 ordinary tests, 14 ignored explicit gates, and 4 capability tests; audits, 160-package license inventory, the locked no-bundle x64 release, API-v3 runtime staging, and responsive smoke passed. The existing transitive `nanoid <3.3.18` finding remains; no dependency, lockfile, permission, model/audio/runtime artifact, retained transcript, external audio, or installer boundary was added.
- Memory: [P5-015 checkpoint](docs/project-memory.md#p5-015---stronger-multilingual-transcription-and-session-language)
