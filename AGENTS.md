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

- Task: P6-014 - A mark-only command for the detached transcript window
- Date: 2026-08-25
- Outcome: the detached transcript window can now flag a moment, which is the first deliberate widening of ADR 0008 and is recorded as ADR 0010. The window exists to be watched while the main window is hidden, so the action cannot be routed through the main window: it may not be there. The event route was rejected because Tauri does not scope `core:event:allow-emit` by event name, so granting it would let a window forge any event the application publishes, and the bus is one-way by design. Instead the window holds exactly one command whose request carries three identifiers and a boolean — the request type is the boundary, not the authorization rule, and there is no field of any shape through which the window could write text into the record. Authorization is exact-`transcript` rather than a set including `main`, so the capability layer and the authorization layer say exactly the same thing. The insights window is unchanged and stays display-only. The user chose this over a Rust-owned global shortcut.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 362 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 48 Vitest files/274 tests, the Vite build, and repository policy. Authorization tests admit exact-`transcript` and reject `main`, `insights`, and every near-miss label with zero service accesses; a command test proves the widening reaches no other transcript command; contract tests on both sides refuse a marking request carrying a `text` field; capability tests assert each secondary window holds exactly the commands listed for it and that neither may hold `allow-annotate-`. Audits, the 160-package license inventory, byte-identical lockfiles, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. The only new webview authorization is `allow-mark-transcript-segment`, on the transcript capability alone.
- Limitations to close next: a mark made in the window is view-state only, so reopening it loses the highlight though the mark is durable. Marking is unreachable while click-through is on, which is correct but undiscoverable. The window still cannot correct, copy, or show history. None of it was exercised on a real desktop.
- Memory: [P6-014 checkpoint](docs/project-memory.md#p6-014---a-mark-only-command-for-the-detached-transcript-window)
