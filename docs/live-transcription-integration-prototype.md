# P3-009 Live Transcription Integration Prototype

## Boundary

P3-009 is a Rust-only product-shaped integration gate. It connects `SourceProcessor` output, including finalized source-local VAD utterances, to the P3-006 bounded scheduler and the P3-008 cancellable supervised worker/CPU owner. It does not start capture, load a product-selected model, publish Tauri events, render transcript UI, persist Markdown/SQLite, retain audio, call OpenRouter, or make a packaging claim.

## Cross-source ordering rule

Every open VAD source reports an ordering frontier: the earliest session timestamp at which any future final from that source could still start. The frontier includes:

- the start of an active utterance;
- the beginning of retained pre-roll before an unclassified pending frame; or
- the processed frontier minus retained pre-roll while idle.

Frontiers may only advance. A queued final is runnable when its start is strictly below both open source frontiers. Equality waits because a future utterance could still have the same start and an earlier end. After a source's terminal VAD flush is accepted and the source is sealed, it contributes infinity. The scheduler then preserves its existing `(start_ms, end_ms, arrival_sequence)` tie order.

## Lifecycle and accounting

- Pause first stops new ordinary input, accepts each source's terminal VAD flush, seals both sources, discards provisional work, and drains every accepted final to exactly one result or fixed gap before becoming paused.
- Resume starts with both open frontiers unknown while retaining their last values as regression floors on the same session timeline.
- Stop follows the same terminal flush, seal, provisional-discard, and exact drain rule before becoming stopped.
- Worker cancellation never loads CPU. The cancelled in-flight final and explicitly abandoned queued finals become fixed source/timeline gaps.
- Backlog rejection, invalid inference output, inference failure, and cancellation are distinct fixed gap codes. No accepted final disappears silently or emits twice.

The scheduler retains its 600-second, 9,600,000-sample, and 4,096-job final ceilings. A single VAD handoff is additionally capped at 64 finalized utterances. Transcript result validation rechecks source, utterance timeline, language/text/segment bounds, segment containment, and segment ordering before accepting an engine result.

## Verification

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc transcription::integration -- --nocapture
```

The focused tests prove:

- a delayed source cannot introduce an older final after newer inference begins;
- VAD pre-roll, active speech, and post-final retained silence produce conservative frontiers;
- pause flushes losslessly, resume resets open frontiers, and stop drains;
- one source's inference failure becomes a gap without poisoning the other source;
- one supervised-worker failure shuts the worker down, loads CPU once, and produces one result per final;
- worker cancellation loads no CPU and accounts for pending work as gaps; and
- two simulated hours at 5x service pressure remain chronological, balanced, bounded, and exactly accounted.

The frozen pressure run received 7,200 alternating one-second finals, produced 2,039 results and 5,161 explicit backlog gaps, split results 1,020 microphone/1,019 system output, and peaked at exactly 600 jobs, 600,000 ms, and 9,600,000 logical queued samples. The result/gap IDs were unique and their total equalled all received finals.

## Limitations

The pressure gate uses generated constant samples, deterministic service cadence, and fake bounded transcript text. It is not an accuracy, latency, allocator/RSS, thread-contention, device-recovery, or real-model benchmark. Product session ownership, capture-thread channels, partial-result production, frontend events, persistence, model installation, worker packaging/restart policy, and preemptible CPU inference remain future tasks.
