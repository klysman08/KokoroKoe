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

- Task: P6-005 - Recoverable transcript-window click-through
- Date: 2026-08-24
- Outcome: clicks can now pass through the transcript window, and the R-010 click-through question is settled. The rule was to omit the feature unless emergency recovery is reliable, so recovery is guaranteed by construction rather than by care: the state is never persisted and so never survives a restart or a crash, it cannot be engaged while the main window is absent because the main window is what disengages it, and it is cleared automatically when the main window closes. The switch lives in the main window, which never becomes click-through. Rust publishes the interaction state on the same event rail as the appearance, so the transcript window still holds no command permission and can show an explicit indicator — a window passing clicks through otherwise looks exactly like a frozen one. No new dependency and no lockfile change.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 335 ordinary tests, 14 ignored explicit gates, and 6 capability tests; locked frontend checks passed 44 Vitest files/202 tests, the Vite build, and repository policy. Tests cover that click-through never reaches persisted state and that a reopened service accepts pointer input, the recovery rule itself, the controls sending a validated request and staying usable while click-through is on, the detached window showing and clearing the indicator from the event alone without invoking any command, and an off-contract reply being rejected rather than shown as state. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. The only new webview authorizations are `allow-get-transcript-window-interaction` and `allow-set-transcript-window-interaction`.
- Limitations to close next: no test presses a mouse button, so confirming that clicks genuinely reach the application behind the window and that disabling restores input still needs a human desktop pass, together with the translucency readability, multi-monitor restore, and shortcut-focus checks R-010 is waiting on.
- Memory: [P6-005 checkpoint](docs/project-memory.md#p6-005---recoverable-transcript-window-click-through)
