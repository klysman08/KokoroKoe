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

- Task: P6-017 - Asking again about one insight
- Date: 2026-08-25
- Outcome: an insight can be asked again, which completes the Manifest section 9.3 actions apart from the provisional-versus-confirmed distinction P6-008 deliberately declined. It stays in the main window: ADR 0010 admitted a command to a secondary window because its request could carry nothing but a flag, and that reasoning does not extend to one that spends money and reaches OpenRouter, so the insights window stays display-only. It also needed no new prompt version — an alternative is the same versioned insight prompt narrowed twice, to the one type and to the segments that insight cited, with the recent window still travelling as surrounding context. It sends no insight text: the transcript is the input, not what the model said before. A moment that has since scrolled out of the recent window is refused rather than answered about approximately, and nothing is spent when it is. The original stays beside the alternative because the user asked to compare, and the cost figures move with the newest generation.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 371 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 49 Vitest files/288 tests, the Vite build, and repository policy. Tests cover the request carrying only the one type with the distant window still absent and the ZDR flags set, a scrolled-away moment refused with nothing sent, a request naming no segments refused at the contract, and the UI sending the insight's own type and citations while carrying none of its content, keeping the original, and moving the cost figure. Both lockfiles are byte-identical and the secondary-window capabilities are unchanged; the locked no-bundle x64 release built with a responsive five-second launch smoke. `cargo audit` reports the 18 warnings the P6-016 checkpoint explains.
- Limitations to close next: an insight that cited nothing offers no Ask again control. Alternatives accumulate under the original and only a fresh batch clears them. The detached insights window shows an alternative as an ordinary card, because the published batch carries no notion of one insight answering another. Nothing prevents asking again repeatedly, and each attempt spends.
- **Nothing built in Phase 6 has run on a real desktop.** The four human R-010 checks are the oldest outstanding item in the phase and are the recommended next task; they gate a risk recorded as resolved.
- Memory: [P6-017 checkpoint](docs/project-memory.md#p6-017---asking-again-about-one-insight)
