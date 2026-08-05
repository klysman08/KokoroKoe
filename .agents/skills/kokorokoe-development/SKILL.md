---
name: kokorokoe-development
description: Implement, review, test, or plan changes in the KokoroKoe Windows meeting-assistant repository. Use for Rust/Tauri backend work, React/TypeScript UI work, Windows audio or local transcription changes, Markdown/SQLite persistence, OpenRouter integration, security reviews, phase completion, and project task handoffs.
---

# KokoroKoe Development

Develop KokoroKoe one bounded, verified task at a time while preserving its local-first privacy and recovery guarantees.

## Start every task

1. Read `Manifest.md`, root `AGENTS.md`, `docs/architecture.md`, and `docs/project-memory.md`.
2. Identify the active phase, task ID, dependencies, acceptance criteria, and permitted scope.
3. Read [phase-gates.md](references/phase-gates.md) for phase boundaries.
4. Read [verification-matrix.md](references/verification-matrix.md) for the required evidence.
5. Inspect the current implementation and working tree before changing files.

## Execute the task

1. Keep sensitive filesystem, credential, audio, model, and network operations in Rust.
2. Preserve typed Rust/TypeScript contracts: JSON fields use `camelCase`; enum values use `snake_case`.
3. Keep audio sources independent as `microphone` and `system_output`.
4. Keep Markdown as the portable source of truth and SQLite as a rebuildable projection.
5. Never send audio to OpenRouter. Send only the minimum text required for the enabled feature.
6. Add no generic frontend filesystem, HTTP, shell, process, or credential capability.
7. Treat transcripts and rendered Markdown as untrusted input.
8. Use bounded queues and explicit error/gap reporting for background pipelines.
9. Preserve unrelated user changes and keep the patch limited to the active task.

## Coordinate sub-agents

- Delegate only concrete, independent subtasks with explicit read/write scope and acceptance evidence.
- Prevent sub-agents from editing `AGENTS.md` or `docs/project-memory.md`; the coordinating agent owns both.
- Review sub-agent output and rerun relevant checks before accepting it.
- Use fresh sub-agents to forward-test substantial skill changes without giving them the expected answer.

## Complete the task

1. Run every applicable check from the verification matrix.
2. Inspect the diff and confirm no secret, transcript content, generated model, audio, or build artifact was added.
3. Update `docs/project-memory.md` with status, decisions, evidence, limitations, and handoff.
4. Update the rolling `Latest completed checkpoint` in root `AGENTS.md`.
5. Report what completed, how to verify it, known limitations, and the next task or phase.

Do not mark a task complete when required evidence is missing or a prototype gate has not passed.
