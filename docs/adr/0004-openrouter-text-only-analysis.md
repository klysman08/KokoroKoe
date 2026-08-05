# ADR 0004: Text-Only OpenRouter Analysis

- Status: Accepted
- Date: 2026-08-05

## Context

KokoroKoe needs optional external language-model insights without weakening local transcription or exposing audio and credentials.

## Decision

Implement OpenRouter behind a Rust-only `LlmProvider`. Its input accepts text-analysis context and has no audio type. Store its key in Windows Credential Manager. Start LLM features disabled and require a visible external-service state when enabled.

Build prompts through versioned specifications. Combine project/session/preset context, accumulated summary, a recent transcript window, and deduplicated FTS5 retrieval. Delimit transcript material as untrusted input. Use structured outputs when supported and validate every completed result before persistence.

Require zero-data-retention/data-collection restrictions by default. Enforce local session budgets through conservative reservation and actual usage reconciliation. OpenRouter failures never fail local capture/transcription.

## Consequences

- Privacy guarantees are enforceable at the module boundary.
- Strict privacy routing can reduce available providers/models.
- Streaming, cancellation, typed errors, malformed output, and budget behavior require mock and controlled tests.
- Summaries may remain deferred when LLM features are disabled or unavailable.
