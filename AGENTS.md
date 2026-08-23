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

- Task: P6-001 - Detached transcript window and per-window capability boundary
- Date: 2026-08-23
- Outcome: opened Phase 6 by adding a Rust-owned detached transcript window and the per-window least-privilege capability boundary the rest of the phase builds on, recorded in ADR 0008. Rust owns the window label, URL, title, and size; the commands take no argument; the new `transcript` capability grants event subscription only and no command, window-management, filesystem, HTTP, shell, or process permission; and every existing command keeps the exact-main rule so capability and authorization independently reject a secondary window.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 309 ordinary tests, 14 ignored explicit gates, and 6 capability tests; locked frontend checks passed 43 Vitest files/187 tests, the Vite build, and repository policy. Capability tests prove the transcript capability holds exactly two event permissions with no command or `core:` grant, that each capability is scoped to one window, and that only the main window is declared at startup. Frontend tests prove the detached surface invokes no command and that both window calls carry no label, URL, path, or dimension. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. The only new webview authorizations are `allow-open-transcript-window` and `allow-close-transcript-window`; the existing transitive `nanoid <3.3.18` finding remains and no dependency or lockfile changed. An incidental fix replaced an OpenRouter cache instant subtraction that panicked on hosts with less than fifteen minutes of uptime.
- Memory: [P6-001 checkpoint](docs/project-memory.md#p6-001---detached-transcript-window-and-per-window-capability-boundary)
