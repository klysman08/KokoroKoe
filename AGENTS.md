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

- Task: P6-016 - Showing which transcript moment an insight came from
- Date: 2026-08-25
- Outcome: an insight now names where it came from — a speaker and a timecode — instead of counting the segments it cited, in both the main window's panel and the detached insights window. Rust resolves the citation at the point where the segments the model was shown are still in hand, so it is a statement about the transcript rather than a claim the model made. It carries a place and never the speech at that place: keeping the text out means publishing an insight adds no meeting content to the event beyond what the insight already says, which preserves ADR 0008's property that the published batch is narrower than the command's reply. The insights window stays display-only, with no command and no capability change. Investigating this settled a question worth remembering: `llm/results.rs` already refuses a batch citing a segment outside the window the model was shown, so every surviving citation names something the context holds and resolution is a plain lookup.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 368 ordinary tests, 14 ignored explicit gates, and 7 capability tests; locked frontend checks passed 49 Vitest files/286 tests, the Vite build, and repository policy. Tests cover the loopback resolving a citation to the seeded segment's own place, a batch citing outside the window being refused after a repair attempt, a moment naming a segment the insight never cited being refused on both sides, and the UI naming the place, falling back to the count when nothing resolved, ordering moments by time, and growing to hours only when the meeting has. Both lockfiles and every capability file are byte-identical; the locked no-bundle x64 release built with a responsive five-second launch smoke.
- **Audit baseline moved and it is not from this change.** `cargo audit` now reports 18 allowed warnings, not the 17 earlier checkpoints recorded. The addition is not a vulnerability: `chacha20 0.10.1` is yanked upstream, reaching the lockfile through `rand 0.10.2`, and `cargo tree -i` finds neither reachable from any target, so neither is compiled in. The lockfile was left byte-identical on purpose — regenerating it is a dependency change, not part of a feature. Expect 18 until that is done deliberately.
- Limitations to close next: a moment is a label, not a link — clicking it scrolls nothing, in either surface, and the insights window has no way to reach the transcript at all. An insight citing many segments lists every place. The timecode is the segment's start, not the instant within it. None of it was exercised on a real desktop.
- Memory: [P6-016 checkpoint](docs/project-memory.md#p6-016---showing-which-transcript-moment-an-insight-came-from)
