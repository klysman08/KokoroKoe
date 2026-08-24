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

- Task: P6-008 - Per-insight actions and cross-batch de-duplication
- Date: 2026-08-24
- Outcome: the insights window gained the Manifest section 9.3 actions that need no command — pin, dismiss, and copy — so its event-only capability is unchanged. Pinned cards survive the next batch, everything else is replaced, and a new batch lands on the first newly arrived card so a pin never hides what the user just asked for. Dismissing an insight keeps it out of later batches that repeat it, identified by the same type/title/content the contract already uses to reject duplicates in one batch; that is what "avoid repeatedly generating the same insight" means from the user's side. Both the dismissed set and the pin count are bounded, and both overflow safely: forgetting a dismissal can only let an insight reappear, and the pin ceiling refuses a new pin rather than discarding an old one. The reading position moved out of component state into the same pure reducer, because every operation moves it. Nothing is persisted; no Rust changed.
- Verification: locked frontend checks passed Prettier, ESLint with zero warnings, strict TypeScript, 46 Vitest files/237 tests, the Vite build, and repository policy; locked Rust checks passed rustfmt, Clippy with warnings denied, 341 ordinary tests, 14 ignored explicit gates, and 7 capability tests, run five consecutive times to confirm the startup race below is gone. Tests cover batch replacement, pinned survival and ordering, landing on the first new card, no duplication of a held card, a dismissal surviving a repeat, the bounded dismissed set forgetting the oldest first, the pin ceiling refusing rather than discarding, dismissing the last card leaving the empty state, and copying reporting both success and a refused clipboard — all with no command invoked. Audits, the 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed.
- Incidental fix: a real startup race in the hotkey thread. A Windows thread has no message queue until it asks for one, and `PostThreadMessageW` fails silently against a thread that has none; the loop announced readiness before its first `GetMessageW` created the queue, so a bind issued in that window posted into nothing and blocked forever. It now forces the queue with `PeekMessageW` before signalling ready, and `bind` waits with a five-second ceiling. `install_shortcuts` runs during Tauri setup, so this could have hung application startup, not just tests. The bug dates from P6-004; a regression test binds immediately after start, twenty-five times.
- Deliberately unmet: section 9.3's provisional-versus-confirmed distinction. Nothing in the pipeline confirms an insight against the transcript; the only signal is the model's self-reported confidence, which the card already shows. Labelling insights "confirmed" on that basis would claim a verification that did not happen.
- Limitations to close next: pins and dismissals are view-local and unsaved, so closing the window discards them. De-duplication is exact-text only, so a rephrased repeat produces a new card. Generating an alternative version is still missing and is the one section 9.3 action that needs a command. Clipboard success was verified against a mock, not against WebView2 on a real desktop.
- Memory: [P6-008 checkpoint](docs/project-memory.md#p6-008---per-insight-actions-and-cross-batch-de-duplication)
