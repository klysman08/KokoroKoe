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

- Task: P6-027 - First-release presentation (after P6-024 through P6-026)
- Date: 2026-10-11
- Outcome: the sidebar links to the repository and credits "Made by klysman08" with the version, through a new exact-`main` `open_external_page` command that takes only `"repository"` or `"author"` and holds both addresses in Rust. The version has one source: Tauri reads `package.json`, Vite injects it, and the policy check keeps `Cargo.toml` equal. The project is MIT-licensed (`LICENSE`, manifests, installer metadata with `licenseFile`). The header names the current page instead of "Foundation checkpoint". The README is rewritten for users, and `pnpm screenshots` regenerates `docs/assets/og.png` and three screenshots from the real interface with fictional data. P6-024 to P6-026 (real-provider fixes, one-press Start meeting, ADR 0012 and 0013) are in the same uncommitted change.
- Verification: Rust fmt and 430 ordinary tests, 14 ignored gates, 7 capability tests. Node 24.19.0 `pnpm verify:frontend` passed Prettier, ESLint at zero warnings, strict TypeScript, 59 Vitest files/358 tests, the Vite build, and repository policy. The full release build produced the MSI and NSIS installers with the license page. Clippy at `-D warnings` still fails as a whole under the machine's Rust 1.98.1 on lints in untouched files. Not yet committed.
- Limitations to close next: the installer does not contain the Whisper runtimes; `public/audio_test/OpenAI.mp3` ships inside every local build; third-party notices are missing; the social preview must be uploaded in the GitHub settings by hand. P6-026's desktop confirmation and the per-session spending limit that resets at restart remain open.
- **Phase 6 closes with the user's desktop checks:** the four R-010 window checks, the preset export dialog, the dashboard, and the meeting flow. Phase 7 starts with packaging the runtimes.
- Memory: [P6-027 checkpoint](docs/project-memory.md#p6-027---first-release-presentation)
