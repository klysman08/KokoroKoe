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

- Task: P6-011 - Correcting and marking a finalized segment
- Date: 2026-08-25
- Outcome: a transcript segment can now be corrected and marked important, and the transcription is never overwritten. The user decided the open question — keep the original recoverable — and the append-only journal made that a property of the storage model rather than a rule: a correction is a new `SegmentCorrection` event naming a segment, and the `FinalizedTranscriptSegment` it refers to is left untouched. Replay folds annotations onto the segments they name, last write wins, and hands downstream a `ReplayedSegment` carrying both readings, so the reading view, search, and summaries use the correction while the transcription is still there. `transcript.md` keeps both: the correction as the body, the original quoted beneath it, so the portable folder alone is enough. Annotation metadata is emitted only when true, because `render_transcript` is also the verifier — the unchanged golden fixture is the evidence that transcripts written before this feature still verify byte-for-byte.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 349 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 48 Vitest files/258 tests, the Vite build, and repository policy. Tests cover the original surviving a correction on disk and in memory, a second correction superseding the first while both older readings remain, importance being set and taken back, an orphaned annotation refused before it reaches the file, a correction held to the transcription's text bounds, the full service round trip through the document and back, and the UI showing the correction with the original behind a disclosure. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. The only new webview authorization is `allow-annotate-transcript-segment`.
- Limitations to close next: nothing surfaces correction history — only the newest correction is readable, earlier ones sit in the journal unread, and there is no undo beyond retyping the original. Marking records a flag with no note, and manual bookmarks are still absent. A corrected segment is searchable only by its corrected wording. Corrections apply to the saved transcript only. None of it was exercised on a real desktop.
- Memory: [P6-011 checkpoint](docs/project-memory.md#p6-011---correcting-and-marking-a-finalized-segment)
