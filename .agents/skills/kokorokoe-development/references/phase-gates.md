# KokoroKoe Phase Gates

Use these gates to prevent work from silently expanding across phases.

## Phase 1 - Architecture and coordination

Complete when architecture, ADRs, dependency/license notes, privacy limitations, the repository skill, project memory, and `AGENTS.md` exist and have been independently reviewed. Do not scaffold product code in this phase.

## Phase 2 - Foundation

Complete when Tauri/React builds on Windows x64; strict TypeScript, formatting, linting, tests, capabilities, sanitized logging, settings, routing, and the base UI shell pass their checks. Do not claim audio or LLM functionality.

## Phase 3 - Audio and transcription

Complete only after the relevant WASAPI, clock, format, VAD, Whisper throughput, GPU fallback, and backpressure prototype gates pass. Require independent source-labelled offline transcription and device recovery without blocking the UI.

## Phase 4 - Projects and persistence

Complete when projects, sessions, presets, Markdown snapshots, checksummed recovery journals, SQLite/FTS projections, export, deletion, and recovery tests pass. Prove SQLite can be rebuilt without losing important content.

## Phase 5 - OpenRouter and insights

Complete when Windows Credential Manager, text-only request construction, privacy controls, prompts, retrieval, structured validation, streaming, cancellation, retries, summaries, insights, and cost controls pass mock and security tests. OpenRouter failure must not stop local work.

## Phase 6 - Desktop experience

Complete when independent windows, readable background opacity, always-on-top, window-state persistence, compact mode, quick-hide, shortcuts, and monitor behavior pass packaged Windows tests. Ship click-through only with a verified emergency escape shortcut.

## Phase 7 - Quality and packaging

Complete when unit, integration, E2E, recovery, offline, hardware, performance, security, installer, audit, and license checks pass and the Windows x64 release contains required notices.

## Phase completion record

At the end of every phase, record completed work, exact verification evidence, known limitations, deferred risks, and the recommended next phase in `docs/project-memory.md`. Then update the rolling checkpoint in `AGENTS.md`.
