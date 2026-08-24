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

- Task: P6-006 - Detached insights window
- Date: 2026-08-24
- Outcome: the insights from P5-016 now have their own window, added as the second application of the ADR 0008 per-window capability boundary — and the decision needed no change to accommodate it. The window is display-only: it holds an event-subscription-only capability, so it can neither request a generation nor retrieve an earlier batch, and generation stays an explicit click in the main window so a second window never becomes a second way to spend money. Rust publishes each batch on a `session-insights` event whose payload is deliberately narrower than the command's reply — insights and their project/Session scope, with no cost, budget, attempt, or repair accounting — and a failed publish never fails the generation. The window shows one insight at a time with previous/next navigation; a newer batch replaces the previous one. The two display-only capabilities are now checked by one shared assertion, so a third window cannot pass by being tested more loosely. No new dependency and no lockfile change.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 337 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 45 Vitest files/215 tests, the Vite build, and repository policy. Tests cover the published payload's exact key set and the absence of cost and budget strings, the fixed local window target, the shared display-only capability assertion for both windows, one-at-a-time rendering and navigation, batch replacement, dropping an off-contract batch, the publication contract rejecting cost fields, argument-free open and close, a sanitized open failure, and routing for both detached hashes and an unknown one. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. The only new webview authorizations are `allow-open-insights-window` and `allow-close-insights-window`.
- Limitations to close next: the insights window has no controls of its own, because appearance, geometry, shortcut, and click-through all live in a transcript-specific service. Keying that state by window label is the recommended next task; the `window_state` table is already keyed by label, but the hotkey controller registers exactly one combination and will need real design for two.
- Memory: [P6-006 checkpoint](docs/project-memory.md#p6-006---detached-insights-window)
