# ADR 0003: Local Whisper Engine and Bounded Scheduling

- Status: Accepted
- Date: 2026-08-05

## Context

Local transcription must produce partial and final results for two sources without blocking capture or loading multiple heavy models.

## Decision

Define a `TranscriptionEngine` abstraction and implement the first engine with Whisper-compatible GGML models. Offer multilingual Tiny and Base initially. Treat Base as the provisional default; keep it as default only if it passes the minimum-hardware Phase 3 throughput gate, otherwise select Tiny. Keep one loaded model and one dedicated inference owner.

Use a bounded scheduler where chronological final utterances take priority, each source has at most one replaceable partial job, and partial work stops under lag. Cap queued final speech and create an explicit transcript gap/error if the hard limit is reached.

CPU is mandatory. Prototype Vulkan with runtime CPU recovery; move acceleration to a supervised worker if driver failure cannot be isolated safely in process.

## Consequences

- Memory and workload remain bounded and predictable.
- Partials are provisional repeated-window inference, not true streaming ASR.
- VAD, model throughput, GPU fallback, and wrapper licensing remain explicit Phase 3 gates.
