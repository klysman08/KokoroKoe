# ADR 0001: Rust Trust Boundary and Data Ownership

- Status: Accepted
- Date: 2026-08-05

## Context

KokoroKoe handles microphone/system audio, local files, model binaries, credentials, and external text analysis. Important content must remain portable without sacrificing local performance and search.

## Decision

Keep sensitive operations in Rust behind narrow Tauri commands, semantic events, and ordered channels. Give React no generic filesystem, HTTP, shell, process, credential, audio, or model capability.

Treat Markdown with YAML front matter as the portable source of truth for projects, sessions, transcripts, presets, summaries, insights, actions, and questions. Treat SQLite as a rebuildable projection for FTS5, relationships, settings, cache, downloads, metrics, and fast UI loading. Store the OpenRouter key only in Windows Credential Manager.

## Consequences

- Security and privacy checks concentrate in one backend boundary.
- Contracts require Rust/TypeScript/Zod drift tests.
- SQLite corruption can be recovered by rebuilding from Markdown and journals.
- Writes require journaling, atomic replacement, conflict detection, and secure path resolution.
