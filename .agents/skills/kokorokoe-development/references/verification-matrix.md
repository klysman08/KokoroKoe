# KokoroKoe Verification Matrix

Apply the smallest set that fully covers the task. Add task-specific checks when risk requires them.

| Change area | Required verification evidence |
| --- | --- |
| Documentation or ADR | Link and path checks, terminology consistency, diff review, independent review |
| Repository skill | `quick_validate.py`, metadata inspection, positive and negative forward-tests |
| Rust domain or service | `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, targeted tests |
| TypeScript or React | `pnpm lint`, strict typecheck, targeted Vitest tests |
| Rust/TypeScript contract | Shared golden fixture parsed by Rust and Zod/Vitest |
| Tauri command/capability | Rust test plus capability/authorization review for each window |
| Windows audio | Unit/fixture tests plus applicable hardware prototype gate evidence |
| Transcription/model | Deterministic audio fixture, throughput/backpressure metrics, CPU fallback proof |
| Persistence | Fault injection, replay idempotency, atomic replacement, SQLite rebuild proof |
| OpenRouter | Mock SSE/error/cancellation/budget tests and proof request bodies contain no audio |
| Credential or security | Packaged Windows smoke test, secret canary scan, sanitized error/log inspection |
| Window behavior | Packaged Windows multi-monitor and emergency-shortcut smoke tests |
| Dependency | Version/API source check, license/audit result, lockfile diff review |

Before completing any task, inspect `git diff --check` and `git status --short`. Do not add generated audio, model weights, secrets, build output, or user transcript data.
