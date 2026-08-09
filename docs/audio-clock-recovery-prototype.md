# P3-007 QPC Alignment and Windows Recovery Prototype

Status: implemented and locally hardware-probed on 2026-08-09; the broader physical device-recovery matrix remains open.

## Boundary

P3-007 closes the calculated two-hour clock gate and proves that one real WASAPI capture loop can fail and recover without stopping the other live source. It extends the existing aggregate diagnostics contract but adds no product command, event, capability, UI, persistence, retained audio, transcription/model wiring, external request, or production fault-injection surface.

## Frozen clock gate

The deterministic test starts from one large QPC epoch at an intentionally awkward 10,000,003 Hz frequency. It independently simulates 44.1 kHz microphone packets of 441 frames and 48 kHz system-output packets of 512 frames for 7,200 seconds. At the one-hour point, the microphone timeline receives a 250 ms source-local discontinuity. Every absolute tick value passes through the production `qpc_ticks_to_100ns` conversion and the same session-relative millisecond mapping used by capture.

Acceptance requires each source's maximum calculated error to remain below 20 ms, no timestamp regression, and no epoch rebase or compression across recovery. The accepted run covered 720,001 microphone timestamps and 675,001 system timestamps with 0 ms maximum calculated error for both. The recovery transition advanced 260 ms: the 250 ms gap plus the next 10 ms microphone packet cadence.

## Live recovery gate

The ignored Windows test starts both default endpoints through the production capture and processing pipeline. After both sources are active and have captured packets, a Rust-test-only atomic hook returns the fixed `audio_injected_capture_failure` error once from the microphone event loop. The ordinary production supervisor performs the retry. The hook cannot be configured or invoked in a production build.

Diagnostics distinguish transient and historical state:

- `captureAttempts` and `captureFailures` expose every attempted start and failed capture cycle.
- `recoveryPendingSinceMs` exposes an open source-local gap while reconnecting.
- `recoveryGaps`, `recoveryGapMs`, and `lastRecoveryGapMs` retain completed gap evidence.
- `lastCaptureFailureCode` retains the fixed historical cause after `lastErrorCode` clears on recovery.

Two accepted local runs restarted the microphone on attempt 2 after one injected failure, closed one 264-265 ms gap, advanced system output by 51-53 packets during and after the recovery observation window, and captured 24-26 more microphone packets after restart. Both sources reported zero timestamp regressions.

## Verification

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc audio::timeline -- --nocapture

cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc audio::windows::tests::hardware_probe_recovers_one_source_without_stopping_the_other -- --ignored --exact --nocapture
```

## Honest limitations

The clock gate verifies integer conversion and shared-epoch alignment, not physical acoustic latency or long-run oscillator drift between two actual devices. The live probe covers one default microphone/render pair and one injected microphone-stream failure. Real default changes, fixed endpoint removal, unplug/hotplug, Windows Audio restart, sleep/resume, Bluetooth transitions, docks, USB/virtual endpoints, Remote Desktop, diverse drivers, and repeated or simultaneous failures still require a broader consented hardware matrix.
