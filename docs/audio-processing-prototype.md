# P3-002 Bounded Audio-Processing Prototype

Status: implemented and locally hardware-probed on 2026-08-08; normalized samples remain an in-memory prototype output and are deliberately discarded.

## Boundary

P3-002 extends the P3-001 Rust capture path with one source-local processing worker and one bounded normalized-output queue for each stable source, `microphone` and `system_output`. It does not add a Tauri command, permission, frontend audio buffer, persisted file, VAD, model, transcription, or product capture UI. Only aggregate processing counters and throttled level diagnostics extend the strict existing status contract.

Each captured packet carries its native format into its own processing worker. The worker validates the format and exact packet size before decoding, then:

1. Converts supported native samples to finite `f32`.
2. Replaces non-finite float input or resampler output with silence and increments a diagnostic counter.
3. Averages all channels with equal weights and clamps the mono result to `[-1.0, 1.0]`.
4. Uses an anti-aliased asynchronous sinc resampler when the native rate is not 16 kHz.
5. Emits exact 160-sample, 10 ms, 16 kHz mono chunks into a separate fixed-capacity queue for that source.
6. Derives RMS dBFS, peak dBFS, and clipping once per 1,600 normalized samples (10 Hz).

Full normalized queues drop the new chunk and increment `processingQueueDrops`; they do not grow or block capture. The prototype sink consumes and discards normalized samples. Status reports produced/consumed samples, processing errors, non-finite substitutions, format changes, resampler delay, pending samples, and the latest bounded level window.

## Supported native input

- Unsigned 8-bit PCM.
- Signed little-endian 16-, 24-, and 32-bit PCM.
- Left-aligned valid PCM bits within supported integer containers, matching `WAVEFORMATEXTENSIBLE`.
- Little-endian 32- and 64-bit IEEE float.
- 1-32 channels and 8-384 kHz input rates after strict block-alignment and sample-format validation.

Windows distinguishes the sample container width from the valid PCM width and specifies that reduced-precision valid bits are left aligned. The decoder follows that rule. See [WAVEFORMATEXTENSIBLE](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible) and [Extensible Wave-Format Descriptors](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/extensible-wave-format-descriptors).

The resampler is `rubato` 4.0.0 with default features disabled. P3-002 uses its asynchronous sinc path, preallocated processing buffers, fixed-input chunks, and the library's default sinc interpolation parameters. The resolved crate is MIT OR Apache-2.0, declares Rust 1.85, and is compatible with the repository's Rust 1.88 pin. See the [rubato 4.0.0 API](https://docs.rs/rubato/4.0.0/rubato/).

## Verification

The ordinary locked Rust suite has deterministic generated in-memory samples, not checked-in audio recordings. It covers the PCM/float container matrix, left-aligned valid bits, NaN sanitization, mono/stereo/four-channel downmix, malformed and truncated input, 8/16/22.05/44.1/48/96 kHz conversion, finite/clamped exact chunks, sinc attenuation above the target Nyquist limit, 10 Hz RMS/peak/clipping windows, format changes, source mismatch, and bounded output-queue isolation.

Run the explicit Windows probe with active default input and output endpoints:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-msvc --lib audio::windows::tests::hardware_probe_processes_both_default_sources_to_16khz_mono -- --ignored --exact --nocapture
```

The probe emits a short local test tone, captures for about five seconds, and prints metadata/counters only. On 2026-08-08 it passed with a 44.1 kHz stereo-float microphone and 48 kHz stereo-float render loopback. Both sources produced and consumed exact 160-sample normalized chunks with zero capture-queue drops, processing-queue drops, processing errors, non-finite substitutions, or timestamp regressions. Level diagnostics were produced for both sources. No sample data was retained.

## Honest limitations and remaining gates

The live probe covers one short run and two stereo-float endpoints. Integer PCM, other channel layouts and rates, hotplug/default changes, USB/Bluetooth/docks/virtual devices, Remote Desktop, Windows Audio restart, and long-run stability remain device-matrix work. Deterministic tests cover the declared format matrix but are not substitutes for driver diversity.

Downmixing is an equal-weight average; it does not yet interpret channel masks or apply speaker-layout weights. Resampler startup delay is reported rather than timestamp-compensated. Partial native and normalized buffers, sinc tail state, and buffered chunks are discarded when a format changes or the bounded prototype stops; there is no flush/finalization contract yet. The latest level is aggregate metadata, not a product event stream. P3-002 does not run VAD, segment speech, retain WAV data, integrate Whisper, schedule inference, emit transcript events, persist sessions, or modify the frontend.
