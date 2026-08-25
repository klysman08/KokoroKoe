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

- Task: P6-010 - Sidebar overflow and a searchable model picker
- Date: 2026-08-25
- Outcome: the sidebar no longer renders content outside itself. It was `h-svh` with a `mt-auto` footer and no scroll container, which held while it carried one window-control panel; P6-007's second panel pushed the total past every screen height. The middle section now scrolls (`min-h-0 flex-1 overflow-y-auto`) and the collapse control is pinned outside it, so a third panel cannot reintroduce the problem. Each window's settings now fold behind a disclosure, with **Pop out** still one click from the top. The OpenRouter role-model field reads as a dropdown: the chevron trigger stays visible instead of being replaced by a clear button, a field that cannot be used says why and is genuinely disabled, each option shows the model id above its provider and context window, and search matches any fragment of the id, name, or provider rather than a leading prefix. No command, contract, or Rust changed.
- Verification: locked frontend checks passed Prettier, ESLint with zero warnings, strict TypeScript, 48 Vitest files/255 tests, the Vite build, and repository policy; locked Rust checks passed unchanged at 341 ordinary tests, 14 ignored explicit gates, and 7 capability tests, with an empty `git diff` over `src-tauri/`. The 160-package license inventory, the locked no-bundle x64 release, and a responsive five-second launch smoke passed.
- Process note: the search filter is a pure exported function tested directly. Driving the combobox's popup from a test asserted framework mechanics rather than the matching rule and failed three times for unrelated reasons; prefer extracting the rule over fighting a primitive in the DOM.
- Limitations to close next: disclosure state is per mount, so both panels fold again on reload. The sidebar scrolls with no visual affordance that it does. Nothing here was seen on a real desktop — the overflow fix is verified structurally and by tests, not by a screenshot.
- Memory: [P6-010 checkpoint](docs/project-memory.md#p6-010---sidebar-overflow-and-a-searchable-model-picker)
