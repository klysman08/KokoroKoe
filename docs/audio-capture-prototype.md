# P3-001 Windows Audio-Capture Prototype

Status: implemented and locally hardware-probed on 2026-08-08; the broader Phase 3 device matrix remains open.

## Boundary

P3-001 proves the Windows capture mechanism without implementing transcription or a meeting session. Rust owns endpoint enumeration and capture. The main window receives only bounded device metadata and aggregate diagnostics through four explicit commands:

- `list_audio_devices`
- `start_audio_capture_prototype`
- `get_audio_capture_prototype_status`
- `stop_audio_capture_prototype`

Starting capture requires the caller to set `acknowledgedCaptureConsent: true`; this is a transport guard, not a substitute for the product's future consent UI. The prototype keeps microphone and render-loopback packets in separate bounded in-memory queues, consumes and discards them, and never writes audio samples to disk, logs, events, SQLite, Markdown, or the frontend.

## Capture design

- Each `microphone` and `system_output` source has its own named thread, event-driven shared-mode `IAudioClient`, packet queue, counters, health state, and retry loop.
- Microphone capture activates an input endpoint. System-output capture activates a render endpoint as a capture stream, causing WASAPI loopback mode.
- Both streams use the endpoint mix format and report sample rate, channels, stored/valid bits, block alignment, channel mask, and integer/float type.
- `IAudioCaptureClient::GetBuffer` supplies a QPC timestamp already converted to 100-nanosecond units. Both channels subtract one QPC session epoch; callback arrival order is not used for alignment. Timestamp-error flags are counted and use a fresh QPC reading as an explicit fallback.
- Each source owns a 4-256 packet bounded queue (64 by default). Full queues drop the new packet and increment `queueDrops`; they never grow without bound or block a capture thread.
- Default-role selections re-resolve once per second and restart on a changed endpoint. Fixed selections retry the same endpoint ID. A channel error updates only that channel and retries from 250 ms up to five seconds.
- Endpoint IDs and names are bounded and control-free at the Rust boundary. Only stable categorical error codes reach the frontend or logs.

The implementation follows Microsoft's requirements that loopback use a render endpoint and shared mode, and that event-driven clients set an event handle. Packet QPC positions are interpreted according to the documented 100-nanosecond conversion. See [Loopback Recording](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording), [IAudioClient](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nn-audioclient-iaudioclient), and [IAudioCaptureClient::GetBuffer](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer).

## Verification

The ordinary Rust suite uses deterministic tests for QPC conversion, shared-epoch mapping, full-queue drops, independent per-source queues, consent/config bounds, stable source/role selections, endpoint diagnostic bounds, command authorization, one-channel isolation, and the production supervisor's retry loop.

Run the explicit local hardware probe on Windows with active default input and output endpoints:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc audio::windows::tests::hardware_probe_enumerates_and_captures_both_default_endpoints -- --ignored --nocapture --test-threads=1
```

The probe emits a short local test tone, runs both streams for about five seconds, prints metadata/counters only, and asserts both sources captured packets, all packets were consumed, timestamps did not regress, and neither queue dropped data.

Local evidence on 2026-08-08:

- Windows machine with one tested default microphone path and one tested default render path.
- Microphone mix format: 44.1 kHz, two-channel, 32-bit float.
- Render-loopback mix format: 48 kHz, two-channel, 32-bit float.
- Two final runs passed with one capture attempt per source, zero timestamp errors, zero timestamp regressions, zero queue drops, and captured/consumed packet counts equal.
- One initial data-discontinuity flag was observed per source and retained in diagnostics; this is expected evidence rather than silently discarded state.

## Honest limitations and remaining gates

This is not the Phase 3 device-matrix gate. It does not yet prove unplug/hotplug or Windows Audio restart recovery on real hardware; docks, Bluetooth profiles, USB devices, virtual devices, Remote Desktop, fixed endpoint removal, multiple drivers, PCM integer formats, channel counts beyond stereo, sample-rate variety, or a two-hour run. The deterministic retry/isolation tests prove supervisor behavior under injected failure, while real device recovery remains required evidence for a later Phase 3 task.

The five-second probe proves monotonic shared QPC-derived timestamps on one machine, not the architecture's less-than-20-ms error target over two hours. It also does not normalize, resample, meter, run VAD, retain audio, transcribe, persist sessions, emit product audio-level events, or provide a product capture UI. Protected/exclusive-mode audio and output silence retain the limitations in [Privacy, Consent, and Technical Limitations](privacy-and-limitations.md).
