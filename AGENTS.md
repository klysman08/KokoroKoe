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

- Task: P6-003 - Durable transcript-window geometry and appearance
- Date: 2026-08-23
- Outcome: made the transcript window's position, size, and appearance survive a restart. Window state lives in a dedicated table in the non-secret settings database at schema version 3, keyed by window label and deliberately outside the revisioned settings record so a window drag never bumps the settings revision. Geometry is stored in physical pixels, written at most every two seconds plus once on close, and restored only when an attached monitor still shows a grabbable portion of the window, so a disconnected display cannot strand it off-screen. No frontend change was required.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 323 ordinary tests, 14 ignored explicit gates, and 6 capability tests; locked frontend checks passed unchanged at 44 Vitest files/194 tests, the Vite build, and repository policy. Tests prove state survives a simulated restart, moves are throttled while a close always writes, absurd geometry is never recorded, unreadable state falls back to defaults, off-screen and sliver-visible positions are refused, an absent row reads as none, and a version-2 database upgrades in place without disturbing stored settings. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. No new webview authorization, dependency, or lockfile change.
- Limitations to close next: the translucent rendering and multi-monitor restore were verified by contract and unit tests only. A human must confirm on a real desktop that text stays readable at the opacity floor, and that geometry restores correctly across display rearrangement, before R-010 can record its readable-text half.
- Memory: [P6-003 checkpoint](docs/project-memory.md#p6-003---durable-transcript-window-geometry-and-appearance)
