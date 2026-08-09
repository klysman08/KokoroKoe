# P3-008 Supervised Vulkan Worker Protocol Prototype

Status: implemented and externally Vulkan-probed on 2026-08-09; product integration and packaged-worker validation remain open.

## Boundary

P3-008 implements ADR 0007's process and protocol boundary without wiring it to live capture, the transcription scheduler, model management, Tauri commands/events, UI, persistence, retained audio, or OpenRouter. The same KokoroKoe executable enters worker mode only when launched with its internal worker argument. The normal Tauri startup path never initializes Vulkan.

## Protocol version 1

The parent and child communicate over anonymous stdin/stdout pipes inherited only by the worker. Each frame contains a four-byte little-endian length and one strict JSON payload. Empty or unknown messages and frames over 16 MiB are rejected.

Startup carries bounded UTF-16 adapter/model paths, model kind, and 1-64 inference threads. The child loads the Vulkan backend and must return an exact hello attesting `vulkan`, protocol version 1, one maximum in-flight request, and the 480,000-sample ceiling. It cannot silently run CPU.

An inference frame carries a monotonically assigned request ID, stable `microphone` or `system_output` source, exact session-relative start/end timestamps, and at most 30 seconds of validated finite normalized 16 kHz samples. The response must return the same request ID/source/timeline and remain within the existing 256-segment/1 MiB transcript contract. Mismatched, overlapping, malformed, unknown, or oversized results invalidate the entire accelerated attempt.

## Lifecycle and recovery

- Mutable ownership permits only one in-flight request per worker.
- Default ceilings are 30 seconds for startup, 5 seconds for each frame write, and 60 seconds for inference/response read, with cancellation checked every 10 ms.
- The parent assigns the child to a Windows Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; termination therefore includes ordinary descendants.
- Startup, protocol, write-timeout, inference-timeout, nonzero-exit, and inference failures terminate the accelerated worker before the lazy CPU factory loads the model.
- The same validated request is attempted once on CPU. An explicit cancellation terminates the worker and returns cancellation without starting CPU inference.
- Fixed aggregate diagnostics distinguish each failure class, accelerated results, CPU loads, CPU attempts, and CPU results without paths or transcript content.

Debug builds contain fixed fault modes for the explicit gate. Release builds contain the real worker protocol and Vulkan engine but exclude every malformed/hang/termination/descendant hook.

## Accepted evidence

The exact gate reused the SHA-256-verified multilingual Tiny model, API-v2 Vulkan adapter, and generated 73,532-sample P3-005 fixture outside the repository. It produced:

- one attested Vulkan result with no CPU load;
- seven failed accelerated attempts covering missing-driver startup, malformed hello, startup timeout, malformed result, blocked-write timeout, inference timeout, and forced nonzero worker termination;
- seven lazy CPU results, one per non-cancelled failure;
- one explicit cancellation with no CPU load;
- zero duplicate and zero lost finalized results; and
- no surviving descendant after Job Object termination.

The exact gate completed in 30.63 seconds on the current development machine. This includes repeated worker/model startup and CPU recovery and is not a latency target.

## Reproduce

First generate/build the external P3-005 inputs if they do not already exist:

```powershell
.\scripts\run-whisper-vulkan-recovery-prototype.ps1
```

Then run:

```powershell
.\scripts\run-whisper-worker-protocol-prototype.ps1
```

Expected aggregate output includes `protocol_version=1`, `vulkan_attested=true`, `accelerated_results=1`, `failed_accelerated_attempts=7`, `cpu_results=7`, `cancelled_requests=1`, `duplicate_results=0`, `lost_results=0`, and successful protocol/timeout/crash/descendant-isolation flags.

## Honest limitations

Evidence covers one NVIDIA GPU/driver, Tiny, one generated English fixture, and development executable paths. The worker is not yet connected to scheduler jobs or packaged with model/native assets. AMD, Intel, multi-GPU, CPU-only/non-Vulkan hosts, old or broken drivers, device loss, suspend/resume, installer/updater behavior, repeated recovery/re-enable policy, parent crash, protocol-version migration, memory/RSS, and cancellation during parent-side CPU inference remain untested or deliberately deferred.
