# Privacy, Consent, and Technical Limitations

## Privacy defaults

- Microphone and system-output audio are processed locally.
- Audio retention is disabled by default.
- OpenRouter features are disabled by default.
- Only necessary transcript text may be sent to OpenRouter; audio buffers and audio files are excluded from the LLM module boundary.
- The OpenRouter API key is stored in Windows Credential Manager and is never returned to React or written to Markdown, SQLite, logs, events, reports, or error messages.
- External telemetry is not included in the MVP. Usage, performance, cost, disk, and error metrics remain local.

When an LLM feature is enabled, the interface must show that text is leaving the device, the selected model, the request purpose, and the estimated budget impact. Zero-data-retention and provider data-collection restrictions start enabled. Any relaxation requires an explicit informed confirmation.

## Local data and user control

Important project and session content remains readable Markdown with YAML front matter. SQLite is a rebuildable index and cache. Users can choose the workspace, export without the application, disable LLM features, disable audio retention, delete credentials, and permanently delete projects/sessions through a confirmed inventory.

The application validates paths beneath the configured workspace and rejects traversal, alternate data streams, unexpected absolute paths, and reparse-point escapes. Transcript content and generated Markdown are treated as untrusted data and are never executed.

The foundation Markdown renderer ignores raw HTML, rejects executable or embedded elements and images, applies an explicit sanitize schema, and renders external links inertly after an `http`, `https`, or `mailto` allowlist check. Future project and session views must use this boundary. Opening an external link is intentionally deferred until it can cross a separately authorized Rust command.

For the Windows MVP, native workspace selection accepts only existing folders on fixed local drive-letter volumes. Network shares, device/verbatim paths, drive roots, removable volumes, reserved Windows names, and paths whose ancestor chain contains a symbolic link or reparse point are rejected. This can exclude redirected or OneDrive-backed Documents folders. A successful write probe is only a point-in-time health result; later persistence must revalidate directory identity and containment before every sensitive write. Windows or third-party folder synchronization may still copy data from an otherwise local folder.

Sanitized error reports contain stable error codes, application/component versions, non-secret configuration categories, and opaque correlation IDs. They exclude API keys, authorization headers, prompts, transcript text, raw provider bodies, and user paths where unnecessary.

## Consent notice

The application must display this meaning during onboarding and before first capture:

> You are responsible for obtaining any consent required to record or transcribe audio and for complying with the laws, workplace policies, and contractual obligations that apply to every participant and location in the meeting.

KokoroKoe does not determine whether recording or transcription is lawful for a particular meeting.

## Windows audio limitations

The P3-001 transport requires explicit consent acknowledgement before its prototype start command succeeds. Capture packets remain in separate bounded Rust memory queues and are consumed without persistence; only device metadata and aggregate health/format/timestamp/drop counters can cross to the authorized main window. This prototype guard does not replace the future first-capture consent UI.

P3-002 processes those packets only in Rust. Each source is decoded, downmixed, and resampled to bounded 16 kHz mono chunks in memory; the prototype sink immediately discards them. The frontend receives only aggregate counts and throttled RMS/peak/clipping metadata. Samples never enter Tauri events, React state, logs, SQLite, Markdown, or external requests, and neither captured nor normalized audio is retained.

- WASAPI loopback captures the complete mix rendered through the selected output endpoint, not a single meeting application.
- Protected/DRM audio may not be capturable.
- Exclusive-mode applications and drivers may interrupt shared-mode capture.
- Bluetooth profile changes, docks, virtual devices, Remote Desktop, hotplug, and Windows Audio restarts can change endpoint IDs or formats.
- Output silence may produce no loopback packets.
- A microphone may acoustically recapture speakers, creating duplicated content across sources.
- `microphone` and `system_output` identify capture origin, not verified human identity. The UI labels are “You” and “Participant” for convenience only.
- The MVP does not include acoustic echo cancellation, per-person diarization, or per-application capture.
- Physical speaker-to-microphone latency is not corrected; timestamps describe capture time.
- P3-002 uses equal-weight channel averaging rather than speaker-mask weighting, reports but does not compensate sinc startup delay, and discards partial/tail buffers on format change or prototype stop.

## Transcription limitations

- Whisper is not a native streaming engine. Partial text is produced by repeated inference over recent audio and may change substantially before finalization.
- Accuracy and latency vary with hardware, selected model, language, accent, noise, overlap, and microphone quality.
- Whisper can hallucinate during noise or silence, repeat text, or omit speech. VAD and final-pass processing reduce but cannot eliminate these failures.
- Confidence is displayed only when the engine supplies defensible probability data.
- When inference remains slower than incoming audio, partials are disabled first. At the hard bounded backlog limit, the application records an explicit gap rather than exhausting memory.
- Vulkan availability does not guarantee successful acceleration. The UI reports the backend actually in use and falls back to CPU.

## OpenRouter limitations

- OpenRouter and downstream providers are external services with their own availability, retention, pricing, moderation, and cancellation behavior.
- Model metadata and prices can change; local catalog data must be refreshed and actual response usage reconciled.
- A local per-session spending limit is enforced through conservative reservation plus actual response usage, but an in-flight provider request cannot be guaranteed to stop billing immediately after cancellation.
- Streaming requests can fail after HTTP 200; partial output is provisional until the complete validated response is received.
- Strict zero-data-retention routing can reduce the available model/provider set.
- OpenRouter unavailability never stops local capture, transcription, persistence, or recovery. A summary may remain deferred and be retried later.

## Desktop-window limitations

Readable opacity requires transparent native windows with alpha applied to CSS background layers, not whole-window opacity that fades text. Click-through is optional and ships only if a global emergency shortcut can reliably disable it. Multi-monitor positions must be clamped when a display disappears or its scale changes.
