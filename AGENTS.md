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

- Task: P6-013 - Marking a segment while the session is running
- Date: 2026-08-25
- Outcome: a finalized segment can be marked important from the live view during a meeting, and the writer defect that made this unsafe is fixed. Starting the queued live-view task uncovered it: the saved transcript is openable for a running session, so P6-011 made it reachable to republish the document behind the live writer's back. The writer caches the fingerprint it last wrote and its flush neither re-read it nor cleared its pending counters on conflict, so one correction or mark during a meeting froze `transcript.md` and the search index for the rest of that run — nothing lost, because the journal keeps every final and reopening repairs it, but the document silently stopped advancing. The writer now adopts a document that reads back as a verified projection of its own journal, and still refuses one that does not, so a foreign edit is never overwritten. An annotation resolves its segment against the journal rather than the document, so a segment can be marked before the run has published it, and republishes with one retry against the fingerprint that actually landed. The design question the previous handoff recorded turned out not to exist: a finalized live segment already carries the id the journal recorded it under.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 359 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 48 Vitest files/268 tests, the Vite build, and repository policy. A writer test reproduces the freeze end to end and asserts the run keeps publishing and publishes the annotation with it; the pre-existing foreign-edit test is unchanged and still refuses, which is the evidence adoption did not weaken it; a service test annotates a segment that is journaled but unpublished; another proves the retry is what carries a publish whose fingerprint the run has superseded, and fails when the retry budget is zeroed. Audits, the 160-package license inventory, byte-identical lockfiles, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. No new command, capability, or contract.
- Limitations to close next: correcting stays in the reading view by choice, and the live view has no history or copy action. A mark made live is held in view state until the transcript is next read, so leaving the card before then loses the highlight though the mark itself is durable. The detached transcript window still cannot mark anything: it is display-only under ADR 0008, so doing it there means either a deliberate capability widening recorded against that ADR or an event the main window acts on. None of it was exercised on a real desktop.
- Memory: [P6-013 checkpoint](docs/project-memory.md#p6-013---marking-a-segment-while-the-session-is-running)
