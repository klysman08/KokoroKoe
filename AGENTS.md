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

- Task: P6-012 - Reading how a segment came to read as it does
- Date: 2026-08-25
- Outcome: the correction history P6-011 left unread is now readable. Replay answers what a segment reads as now by collapsing its corrections; `SessionJournal::segment_history` reads the same records and declines to fold them, which is the only way to recover a wording that was corrected and then corrected again — `transcript.md` carries the current reading and the transcription, and nothing in between. The transcription reported is the journaled record itself, not a reconstruction. The history is bounded at 64 revisions and drops the oldest rather than the newest, so both ends of the story always survive, and truncation is stated. One exact-main command, `get_transcript_segment_history`, serves it; it writes nothing, and the app asks only when the reader opens the disclosure. "Use this wording" fills the correction box rather than saving, because putting an old wording back is itself a correction and goes through the same append as any other.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 356 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 48 Vitest files/266 tests, the Vite build, and repository policy. Tests cover three corrections read back oldest first, an uncorrected segment distinguished from an unknown one, the bound dropping the oldest revision while keeping the transcription and the newest, one segment's corrections staying out of another's history, the full service round trip proving the middle wording is absent from the document, malformed timestamps and unusable wordings refused on both sides of the boundary, the request carrying no transcript text, and the UI reading nothing until the disclosure opens. Audits, the 160-package license inventory, byte-identical lockfiles, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. The only new webview authorization is `allow-get-transcript-segment-history`.
- Limitations to close next: the history shows corrections only, so when a segment was marked important is still unreadable. Restoring fills the box but does not diff. Past 64 revisions the journal still holds the rest but nothing reads it. History, correcting, marking, and copying all stop at the saved transcript — the live and detached transcript views cannot address a segment at all, which is the next task and is a design question, not wiring. None of it was exercised on a real desktop.
- Memory: [P6-012 checkpoint](docs/project-memory.md#p6-012---reading-how-a-segment-came-to-read-as-it-does)
