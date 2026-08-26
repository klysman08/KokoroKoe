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

- Task: P6-015 - Manual bookmarks, distinct from marking important
- Date: 2026-08-25
- Outcome: a finalized segment can be bookmarked, optionally under a one-line name, and the whole set of pins in a Session can be read at once. The Manifest settled the design by listing bookmarks among the transcription window's requirements rather than among the per-block actions that already include "Mark as important": marking is a judgement about a segment, a bookmark is a way back to a place. So it is its own journal event, and a test asserts neither flag ever moves the other in either direction. A name is what makes a list of pins worth scanning, so a bookmark may carry one — bounded to a single scannable line — and removing a pin carries no name, because there is nothing left to name. `transcript.md` carries the pin and its name, emitted only when set, so every transcript written before this still verifies; the frozen golden fixture is unchanged. The set is read from the journal, so a pin dropped during a run is findable before the writer publishes it, and it is shown by switching the transcript to a bookmarks-only view rather than inventing a second list format.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 366 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 48 Vitest files/280 tests, the Vite build, and repository policy. Tests cover the two flags staying independent through bookmark, mark, and unbookmark; every unusable name and a name attached to a removal being refused at append and in both contracts; the full service round trip proving the document carries the pin and its name and loses both when the pin goes; a pin listed before the run published it; and the UI sending a label, sending no label field when unnamed, reading the set only when asked, and reporting an empty session. Audits, the 160-package license inventory, byte-identical lockfiles, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. The only new webview authorization is `allow-list-transcript-bookmarks`, on the main capability.
- Limitations to close next: bookmarking is in the saved transcript only — the in-session live view can mark but not bookmark, and the detached transcript window cannot bookmark at all, since a named bookmark is text and ADR 0010 forbids that window writing text. The bookmarks-only view is a filter rather than a jump, so the surrounding conversation is not visible without switching back. A bookmark carries a one-line name and no note; renaming means bookmarking again. None of it was exercised on a real desktop.
- Memory: [P6-015 checkpoint](docs/project-memory.md#p6-015---manual-bookmarks-distinct-from-marking-important)
