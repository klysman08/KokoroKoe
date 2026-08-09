# ADR 0007: Supervised Vulkan Transcription Worker

- Status: Accepted
- Date: 2026-08-09

## Context

ADR 0003 required CPU transcription and left Vulkan in process only if driver failures could be isolated safely. Ordinary native error returns can be caught at the C ABI, but a graphics driver or backend can terminate the process through an abort, access violation, or device-loss path before Rust receives an error. Loading the Vulkan backend in the Tauri process would therefore let an optional acceleration failure terminate capture, persistence, and the UI.

The upstream Whisper context also falls back to CPU when no GPU device is available. KokoroKoe must not report Vulkan merely because GPU use was requested; the selected backend must be attested explicitly.

## Decision

Run Vulkan model loading and inference in a supervised local child process. Keep the Tauri process free of Vulkan driver/backend initialization during normal startup. The worker must attest a Vulkan device before accepting work and use a bounded protocol that carries one validated finalized utterance and returns at most one complete result or one fixed failure.

Treat worker startup failure, backend unavailability, protocol failure, timeout, nonzero exit, and crash-class termination as one failed accelerated attempt. Discard any incomplete accelerated response, terminate the worker if necessary, and retry the same source/timeline/sample request exactly once on the CPU engine. Disable acceleration after an inference failure until an explicit later recovery policy restarts it. Never emit both the accelerated and CPU result for one utterance.

Load the CPU model only after the accelerated worker has failed or is unavailable. This preserves the one-heavy-model rule: a healthy worker owns the Vulkan model; after worker termination releases its resources, the parent-side inference owner loads the CPU model. Model/result bounds, fixed errors, transcript privacy, and process-lifetime native-module ownership apply independently in each process.

## Consequences

- A native Vulkan abort cannot terminate capture, persistence, or the desktop UI.
- Missing Vulkan drivers cannot prevent application startup or CPU transcription.
- Acceleration adds a process boundary, startup cost, bounded IPC protocol, packaging work, timeouts, and worker lifecycle supervision.
- Backend attestation prevents silent CPU execution from being labelled Vulkan.
- P3-005 proves the isolation and fallback rule but does not implement the product worker protocol, scheduler, packaging, commands, events, or UI.

## Evidence

P3-005 built pinned `whisper.cpp` with Vulkan and used one locally generated 4.596-second speech fixture. An attested Vulkan child completed inference. A child constrained to a nonexistent `VK_DRIVER_FILES` manifest returned the fixed backend-unavailable status. A prototype-only native abort inside Vulkan inference terminated only the child; the parent remained alive and produced exactly one CPU result. Both forced failure scenarios reported zero duplicate and zero lost finalized results.

See [P3-005 Vulkan recovery prototype](../whisper-vulkan-recovery-prototype.md).
