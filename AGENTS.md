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

- Task: P5-017 - Final session summary
- Date: 2026-08-23
- Outcome: added exact-main `get_session_summary` and `generate_session_summary` commands that summarize a finished Session through the existing prompt/budget/transport/validation/repair/retry boundaries and publish the result as the portable `summary.md` document required by the Manifest. Generation is refused while a Session is running, so stopping stays local; the document is written atomically with front matter, a retained backup, and byte verification; model output is escaped so it cannot forge Markdown structure; and the saved document is rendered through the existing inert Markdown component. This completes Phase 5.
- Verification: locked Rust checks passed rustfmt, Clippy with warnings denied, 307 ordinary tests, 14 ignored explicit gates, and 4 capability tests; locked frontend checks passed 40 Vitest files/182 tests, the Vite build, and repository policy. A loopback test proved the request spans the Session's first and last statement while no provider secret canary, workspace path, or device endpoint reaches the wire, and that ZDR/data-collection-denied routing is set. Store tests proved atomic round-trip, backup retention, foreign-scope and malformed-document rejection, and Markdown-injection resistance. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed. The only new webview authorizations are `allow-get-session-summary` and `allow-generate-session-summary`; the existing transitive `nanoid <3.3.18` finding remains and no dependency, lockfile, generic permission, event, extra-window, or audio boundary was added.
- Memory: [P5-017 checkpoint](docs/project-memory.md#p5-017---final-session-summary)
