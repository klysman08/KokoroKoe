# P3-005 Vulkan Failure Isolation and CPU Recovery Prototype

Status: Completed on 2026-08-09 for the bounded Phase 3 gate.

## Outcome

Vulkan acceleration must run in a supervised local worker. The exact probe attested and executed the Vulkan backend on the local NVIDIA RTX 3070 Ti, then proved that both missing-driver startup failure and a native inference abort leave the parent alive and allow exactly one CPU result for the same finalized utterance. No result was duplicated or lost.

The project-owned adapter is now API v2. Model load requires an explicit `cpu` or `vulkan` backend, rejects an unavailable Vulkan device instead of silently accepting upstream CPU fallback, and lets Rust verify the backend recorded on the model handle. CPU remains mandatory.

## Frozen gate

Before measurement, P3-005 fixed these requirements:

- Use one generated local speech fixture and the same bounded source, timeline, and sample request in every case.
- Require one successful, explicitly attested Vulkan model load and inference.
- Force Vulkan startup failure in a fresh child process by pointing the official loader override `VK_DRIVER_FILES` at a nonexistent driver manifest.
- Force a crash-class inference failure with a prototype-only native abort hook compiled only into the external P3-005 adapter.
- Bound every child to 60 seconds and discard all incomplete child output.
- After each forced failure, transcribe the same finalized request once on CPU and require one complete source/timeline-preserving result, zero duplicates, and zero losses.
- Keep product commands, capabilities, UI, model management, live events, partial scheduling/backpressure, persistence, retained audio, and external services out of scope.

## Fixture and inputs

The runner uses Windows System Speech to generate a local sentence, then FFmpeg converts it to finite 16 kHz mono `f32` in the external scratch directory. The measured fixture contained 73,532 samples (4.596 seconds). It is generated for the probe, is not a user meeting recording, and is neither committed nor redistributed.

Other verified inputs are:

- official `whisper.cpp` v1.9.2 commit `306c88f4d1286aec1bf96e544632897886af5501`;
- multilingual Tiny model SHA-256 `be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21`;
- LunarG Vulkan SDK 1.4.350.0 installer SHA-256 `855b27ba05d2d8119c5114c5d4ff870ca38f2c632b11e1bb9923b9b7e6ecfe7b`.

The SDK is copied under `%LOCALAPPDATA%\KokoroKoe\p3-005` with LunarG's `copy_only=1` mode. The script does not modify the registry or persistent system `PATH`. Source, SDK, model, generated audio, DLLs, and build output remain outside the repository.

## Failure matrix

| Case | Isolated observation | Parent action | Result |
| --- | --- | --- | --- |
| Vulkan available | Adapter attested Vulkan and the child returned one complete inference result | No CPU call | Passed |
| Driver unavailable at startup | Child returned fixed `transcription_backend_unavailable` semantics | Load CPU after child exit and transcribe the same request once | One CPU result; no duplicate/loss |
| Native Vulkan inference abort | Child terminated below the Rust test boundary | Parent survived, loaded CPU, and transcribed the same request once | One CPU result; no duplicate/loss |

The total exact Rust gate completed in 24.93 seconds on the current machine. That duration includes process/model/inference work for several cases and is not a latency or throughput target.

## Reproduce

From the repository root on Windows x64 with Git, CMake, MSVC, FFmpeg, and a Vulkan-capable driver:

```powershell
.\scripts\run-whisper-vulkan-recovery-prototype.ps1
```

The first run downloads the pinned model and approximately 309 MB Vulkan SDK installer, then copies about 2.18 GB of SDK files into the external scratch directory. Pass `-VulkanSdkPath` to use an existing SDK or `-ScratchPath` to choose another dedicated path outside the repository.

Expected final aggregate output:

```text
vulkan_attested=true startup_failure_isolated=true inference_abort_isolated=true startup_cpu_results=1 inference_cpu_results=1 duplicate_results=0 lost_results=0 supervised_worker_required=true
```

The child processes suppress native output and never print transcript text, detected language, samples, model paths, or driver paths.

## Boundaries and limitations

- The product supervised-worker executable and IPC protocol are not implemented yet; P3-005 freezes their required isolation and retry semantics.
- The real acceleration pass covers one NVIDIA GPU/driver, one Tiny model, and one generated English utterance. AMD, Intel, multi-GPU, old/broken drivers, device loss, suspend/resume, timeout, and packaged-worker behavior remain future validation.
- The forced missing-driver case exercises loader/device unavailability. The abort hook is deliberate crash-class fault injection, not a claim that every real driver failure behaves identically.
- Upstream Vulkan compilation emits its own MSVC warnings. The KokoroKoe adapter itself builds with `/W4 /WX`; upstream warnings are not suppressed or presented as project-source cleanliness.
- Vulkan remains optional. CPU-only and non-Vulkan machines must start and transcribe without loading the Vulkan worker.

The architectural decision is recorded in [ADR 0007](adr/0007-supervised-vulkan-transcription-worker.md).
