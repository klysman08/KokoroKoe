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

- Task: P6-020 - Bookmarking and copying while the meeting runs
- Date: 2026-08-31
- Outcome: the live transcript view now meets the Manifest 9.2 window requirements it was still missing. "Allow manual bookmarks" and "Allow transcript segments to be copied" are listed among the transcription window's requirements, but both stopped at the saved transcript — the live view could only mark. A bookmark dropped live carries no name on purpose: naming is what makes a list of pins scannable, but a text box in a scrolling pane during a call is the wrong moment to ask for one, and the saved transcript already has the naming box. The detached window gained copy but not bookmark: the clipboard is a browser API, so copying needs no command and widens nothing a display-only window may do, whereas a bookmark request can carry a label and so falls outside the reasoning ADR 0010 used to admit one command there. No command, capability, contract, or Rust change.
- Verification: Node 24.19.0 gates passed Prettier, ESLint at zero warnings, strict TypeScript, 51 Vitest files/300 tests, the Vite build, and repository policy; Rust untouched at 377 ordinary tests, 14 ignored gates, and 7 capability tests. The new tests assert the detached window copies speaker, timecode, and text while invoking **no command**, and that a refused clipboard is reported rather than a copy claimed. Lockfiles byte-identical; x64 no-bundle release built with a responsive launch smoke.
- Limitations to close next: the live view cannot correct a segment (deliberate) or ask about one (not deliberate — it is the last Manifest 9.2 per-block action missing there). Live marks and bookmarks are view state, so leaving the session card loses the highlight though the annotation is durable. The detached window still cannot bookmark.
- **Nothing built in Phase 6 has run on a real desktop.** The four human R-010 checks remain the oldest outstanding item.
- Memory: [P6-020 checkpoint](docs/project-memory.md#p6-020---bookmarking-and-copying-while-the-meeting-runs)
