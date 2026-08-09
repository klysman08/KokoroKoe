# P3-006 Transcription Scheduler and Backpressure Prototype

Status: Completed on 2026-08-09 for the bounded Phase 3 gate.

## Outcome

The pure Rust scheduler keeps inference work bounded while protecting finalized utterances. Pending finals always run before partials and are ordered by session timeline across `microphone` and `system_output`. Each source owns at most one replaceable partial slot, selected round-robin only when no final is pending.

When queued final speech reaches 20 seconds, the scheduler clears partial slots and suppresses new partial work. It restores partial admission only after final speech falls below 10 seconds. Pending finals are bounded by all of these ceilings:

- 600,000 milliseconds of speech;
- 9,600,000 normalized 16 kHz samples;
- 4,096 job nodes.

If a new whole final would exceed a ceiling, the scheduler preserves already accepted older work and returns `transcription_final_backlog_exceeded` with the rejected source and timeline. It never silently discards or replaces an accepted final.

## Frozen deterministic gate

Before the soak, P3-006 fixed a fake inference owner at 5x the duration of each audio job. The test then simulated 7,200 seconds with one one-second final arriving each second, alternating sources. Partial candidates were also submitted continuously so the hysteresis and two-slot limit remained under pressure.

The exact result was:

```text
simulated_seconds=7200 slowdown=5x finals_enqueued=2040 finals_started=1440 final_gaps=5160 microphone_started=720 system_output_started=720 microphone_gaps=2580 system_output_gaps=2580 plateau_min_samples=9600000 plateau_max_samples=9600000 peak_logical_samples=9600000 max_final_jobs=600 partial_slots_max=2
```

The second simulated hour remained exactly at the 9,600,000-sample logical final backlog ceiling. Both sources started 720 finals and received 2,580 explicit gaps. Every accepted job ID was unique, and start timestamps never regressed across started plus still-pending finals.

## Reproduce

From the repository root:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked transcription::scheduler -- --nocapture
```

The focused suite covers final chronology and priority, source-local partial replacement, round-robin partial selection, hysteresis, speech/sample/job ceilings, fixed sanitized errors, deterministic slowdown, and the two-hour soak. It uses no Whisper model or audio file.

## Boundaries and limitations

- The soak measures logical queued sample storage and job counts. It deliberately shares one immutable generated buffer, so it is not an allocator or process-RSS benchmark.
- The scheduler is not connected to WASAPI, VAD output, Whisper, the supervised Vulkan worker, a Tauri command/event, persistence, or React.
- Chronology is guaranteed among finals pending when the scheduler chooses work. A future live integration must define an arrival watermark if source workers can deliver older utterances after newer inference has already started.
- The gap object is an internal Rust contract. Phase 4 persistence and the future transcript UI must durably record and clearly display it.
- Real inference latency variance, worker crashes/timeouts, channel contention, and pause/stop behavior require integration tests in later bounded tasks.

This prototype implements the scheduling decision in [ADR 0003](adr/0003-local-transcription-and-backpressure.md) without changing that decision.
