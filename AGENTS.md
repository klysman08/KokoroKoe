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

- Task: P6-018 - Deleting a project or session, and two UI fixes
- Date: 2026-08-31
- Outcome: three items reported after using the built application. A project or session can now be deleted, and the user chose what that means: the folder **moves to `trash/` in the workspace** rather than being destroyed, so a mis-click costs a drag back and never a meeting — the same instinct that keeps a transcription under every correction. Deleting is refused while a run is in progress rather than moving a folder out from under its writer, and the removal is verified by the item being *absent* from a rebuilt index, which is the mirror of how a write is verified. The detached windows now wear the application theme: they render the same document under a different route, so only the shell was painting itself, and a toggle travels between windows as a `storage` event. The OpenRouter catalog scrolls inside its own box instead of pushing Settings off the page.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 375 ordinary tests, 14 ignored gates, and 7 capability tests; locked frontend checks passed 51 Vitest files/296 tests, the Vite build, and repository policy. Tests cover a project and a session moving to the trash with their files byte-identical, a delete-restore-delete keeping both copies, the running-state predicate in both directions, the confirmation asking before it removes and saying nothing is erased, and every surface wearing the theme. Lockfiles byte-identical; x64 no-bundle release built with a responsive launch smoke. Two new main-window permissions, `allow-delete-project` and `allow-delete-session`; no secondary-window change. `cargo audit` reports the 18 warnings P6-016 explains.
- **Limitation worth knowing before promising more:** restoring from the trash is a file operation the application does not watch for. A folder dragged back into `projects/` is rediscovered only when the index next rebuilds, which today happens on the next write. The trash is never emptied automatically and has no in-app view.
- **Nothing built in Phase 6 has run on a real desktop.** The four human R-010 checks remain the oldest outstanding item and are still the recommended next task; deletion in particular is worth exercising by hand before it is trusted.
- Memory: [P6-018 checkpoint](docs/project-memory.md#p6-018---deleting-a-project-or-session-and-two-ui-fixes)
