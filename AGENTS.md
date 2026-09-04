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

- Task: P6-021 - The built-in preset library
- Date: 2026-09-04
- Outcome: the preset is now a choice. Manifest section 8 names nine presets and the application had one, written as an object literal in the session form, so every Session ever created was a technical interview and the field that decides which insight types P5-016 requests could not be set. The nine presets are a Rust-owned compile-time catalog — `&'static str` descriptors in `presets/catalog.rs`, the same shape as the transcription-model catalog — published by one read-only exact-`main` `list_presets` command that reads no file and holds no state. What a Session stores is unchanged: choosing a preset still copies a `PresetSnapshot`, so a meeting keeps reading the way it did when it started, and a Session whose frozen preset the library no longer lists keeps it as its own option rather than being silently rewritten. A project's default preset is now choosable and a new Session opens on it. No write, no file, no per-project preset storage.
- Verification: Node 24.19.0 gates passed Prettier, ESLint at zero warnings, strict TypeScript, 53 Vitest files/310 tests, the Vite build, and repository policy; locked Rust gates passed fmt, Clippy at `-D warnings`, 387 ordinary tests, 14 ignored gates, and 7 capability tests. The load-bearing test asserts every catalog entry passes `valid_preset`, the same rule session creation enforces, so nothing can be offered in the library and then rejected; another asserts the shared `preset-catalog-v1.json` fixture **is** a catalog entry, so the Zod contract cannot drift from Rust while both pass. Lockfiles byte-identical; x64 no-bundle release built.
- Limitations to close next: custom presets do not exist. `create_preset`, `update_preset`, `duplicate_preset`, `export_preset`, and `delete_custom_preset` are specified in the architecture and unbuilt, the per-project `presets/` folder the layout names is still never written, and section 9.1's preset-management surface is missing. That work needs Markdown documents, a journal, and five write commands — a different kind of task from this one.
- **Nothing built in Phase 6 has run on a real desktop.** The four human R-010 checks remain the oldest outstanding item.
- Memory: [P6-021 checkpoint](docs/project-memory.md#p6-021---the-built-in-preset-library)
