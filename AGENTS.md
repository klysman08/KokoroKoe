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

- Task: P6-023 - Local dashboard
- Date: 2026-10-11
- Outcome: MVP item 16 is built, so every item in Manifest section 17 now has an implementation. **Dashboard** shows sessions, transcribed speech, the most used projects and Whisper and language models, tokens, estimated cost, insights generated, disk space, and recent errors, for the last 7 or 30 days or all time, for one project or all. Token use, cost, and insight counts had no durable home — P5-006's ledger is process-local and `Session.usage` never advances — so the new `metrics` module owns `metrics.sqlite3`: one row per reconciled OpenRouter request written at `reconcile_prompt_usage` (keyed by request id, integer picodollars), one per delivered insight batch, and the sanitized envelope of the latest 200 failures written at `record_app_error`, without technical detail. It is the only database that is not a Markdown projection and it shares no lock with settings, because a usage row is written from inside an OpenRouter call. A failed write is logged and never fails the operation. `get_dashboard_metrics` is exact-`main` and read-only; speech time merges crosstalk; disk is all-time, link-free, and bounded; the reply says when the ledger began.
- Verification: Node 24.19.0 gates passed Prettier, ESLint at zero warnings, strict TypeScript, 56 Vitest files/344 tests, the Vite build, and repository policy. Rust passed fmt and 422 ordinary tests, 14 ignored gates, and 7 capability tests; the shared `dashboard-metrics-v1.json` fixture is asserted equal to Rust's serialization and parsed by Zod. Lockfiles byte-identical; x64 no-bundle release built and launch smoke responded. **Clippy at `-D warnings` fails as a whole** under the machine's Rust 1.98.1 (the pinned 1.88.0 is no longer installed), on newer lints in nine files this task did not touch; no diagnostic falls in a changed file.
- Limitations to close next: spending before this version was never recorded; a reservation retained without final usage has no row; deleting a project keeps its rows; the per-session spending limit still resets at restart because the ledger does not yet seed the budget baseline.
- **Nothing built in Phase 6 has run on a real desktop.** The four human R-010 checks, the preset export dialog, and a first look at the dashboard are the next step and close Phase 6.
- Memory: [P6-023 checkpoint](docs/project-memory.md#p6-023---local-dashboard)
