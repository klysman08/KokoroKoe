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

- Task: P6-007 - Window-keyed detached-window state
- Date: 2026-08-24
- Outcome: appearance, geometry, show/hide shortcut, and click-through are now keyed by window, so the transcript and insights windows each have their own — which is what Manifest section 10 requires and what the insights window was missing. One service holds one runtime per window, both hotkeys live on the one message-loop thread with per-window ids and distinct default bindings, and the persisted state shape is unchanged because `window_state` was already keyed by label. Every reply and event names its window, listeners ignore the ones that are not theirs, and the adapter rejects a reply about the wrong window rather than rendering one window's state under the other's controls. ADR 0009 records the one property this changed: the window commands now take a closed `DetachedWindow` enum instead of no argument, which keeps the frontend unable to name, address, or size an arbitrary webview because an unknown value fails to deserialize before the command body runs. Ten window commands became eight. No new dependency and no lockfile change.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 340 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 45 Vitest files/219 tests, the Vite build, and repository policy. Tests cover one window's state not following the other in memory or on disk, distinct hotkey ids and default bindings, two independent registrations where releasing one does not release the other, click-through absent from both windows' persisted state, only the windows Rust owns being nameable, the adapter sending nothing but the window name and rejecting a cross-window reply, and each detached surface ignoring the other's events. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed.
- Limitations to close next: the four R-010 desktop checks now apply to both windows and are still outstanding. A combination assigned to both windows is caught only by the platform refusing the second registration, which is reported as a taken shortcut without naming the other window as the holder. There is still no monitor picker, and maximized state is not distinguished from ordinary geometry.
- Memory: [P6-007 checkpoint](docs/project-memory.md#p6-007---window-keyed-detached-window-state)
