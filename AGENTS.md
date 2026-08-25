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

- Task: P6-009 - Per-segment transcript actions
- Date: 2026-08-25
- Outcome: saved transcript segments gained the Manifest section 9.2 per-block actions that need no new backend surface. Copying is view-local and carries the speaker, timecode, and text and nothing internal. **Explain**, **Suggest a response**, and **Summarize** are prefilled questions through the one already-verified `ask_manual_question` path rather than three new prompt specs, so the answer stays inside the schema that path validates and no command, capability, prompt version, or dependency was added. Prefilling and not sending is the deliberate part: this application sends meeting text to a third party, so a one-click label must not be what transmits — the preset opens the question box with text in it and the user reads, edits, or abandons it before anything leaves the machine. Search hits get the question actions but not copy, because a hit renders a snippet with match markers rather than the segment's own text.
- Verification: locked frontend checks passed Prettier, ESLint with zero warnings, strict TypeScript, 47 Vitest files/246 tests, the Vite build, and repository policy; locked Rust checks passed unchanged at 341 ordinary tests, 14 ignored explicit gates, and 7 capability tests. Tests prove every preset parses against the manual-question contract, that a preset prefills without invoking the command, that a second preset replaces the text, that the freehand path still opens empty, the exact clipboard text, and that a refused clipboard is reported rather than claimed. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed.
- Process note: `npx tsc -b` reported success on a type error that `pnpm verify:frontend` then caught, because the incremental build info was stale. Treat the composite gate, not a bare `tsc -b`, as the typecheck of record.
- Limitations to close next: marking a segment as important and correcting a finalized segment are both unimplemented and both write to the record of the meeting; take them together. The actions exist on the saved transcript only — the live view and the detached transcript window have none, and a live partial is not addressable because the manual-question path needs a materialized finalized segment. Presets are fixed English and do not follow the Session language.
- Memory: [P6-009 checkpoint](docs/project-memory.md#p6-009---per-segment-transcript-actions)
