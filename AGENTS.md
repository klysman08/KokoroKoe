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

- Task: P6-022 - Custom presets
- Date: 2026-09-04
- Outcome: Manifest section 8 is complete. A user can write, edit, duplicate, export, and delete presets of their own from **Manage presets** on a project card. Where they live had to be decided rather than assumed — section 11 draws `presets/` inside each project, while the architecture addressed presets by identifier alone — so **ADR 0011** records the choice: one Markdown document per preset inside `projects/<project>/presets/`, every preset command scoped by a project, and `list_presets` returning the built-ins alone when it is given none. The store reaches those files only through the pinned project directory the project store already opens, derives each name from the preset's own record, refuses a document naming another project even when found in this folder, and reuses the existing synced-temporary write, atomic replace, revision-and-fingerprint check, and Trash. A built-in is refused before the workspace is opened, because a compile-time constant has no document to edit. `version` moves only when the instructions move, so a rename does not claim past meetings ran on different guidance.
- Verification: Node 24.19.0 gates passed Prettier, ESLint at zero warnings, strict TypeScript, 54 Vitest files/330 tests, the Vite build, and repository policy; locked Rust gates passed fmt, Clippy at `-D warnings`, 402 ordinary tests, 14 ignored gates, and 7 capability tests. Store tests cover the round trip, the stale revision, the externally edited document, the rename that moves the file, the deletion that lands in `trash/`, and the foreign-project document that is ignored where it sits; parser tests prove user text containing `---` cannot escape the front matter. Lockfiles byte-identical; x64 no-bundle release built.
- Limitations to close next: a preset cannot be shared between projects inside the application, and there is no import — export writes a document that nothing reads back. **The export save dialog has never been opened outside a test.**
- **Nothing built in Phase 6 has run on a real desktop.** The four human R-010 checks remain the oldest outstanding item, and the export dialog now belongs with them.
- Memory: [P6-022 checkpoint](docs/project-memory.md#p6-022---custom-presets)
