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

The active Session sidebar state contains only Session identity, title, lifecycle state, and revision. Its live transcript view holds at most 500 scoped inert records in transient React memory; it does not create an additional transcript store and is cleared when the view is destroyed or the app exits. Final transcript authority remains the Rust journal and verified Markdown snapshot. Browser local storage contains only the `light`/`dark` appearance preference—never transcript text, paths, credentials, model catalogs, provider output, or Session metadata.

The application validates paths beneath the configured workspace and rejects traversal, alternate data streams, unexpected absolute paths, and reparse-point escapes. Transcript content and generated Markdown are treated as untrusted data and are never executed.

The foundation Markdown renderer ignores raw HTML, rejects executable or embedded elements and images, applies an explicit sanitize schema, and renders external links inertly after an `http`, `https`, or `mailto` allowlist check. Future project and session views must use this boundary. Opening an external link is intentionally deferred until it can cross a separately authorized Rust command.

For the Windows MVP, native workspace selection accepts only existing folders on fixed local drive-letter volumes. Network shares, device/verbatim paths, drive roots, removable volumes, reserved Windows names, and paths whose ancestor chain contains a symbolic link or reparse point are rejected. This can exclude redirected or OneDrive-backed Documents folders. A successful write probe is only a point-in-time health result; later persistence must revalidate directory identity and containment before every sensitive write. Windows or third-party folder synchronization may still copy data from an otherwise local folder.

The workspace shortcut re-runs the same local-volume, reparse-point, and write-health probe before Rust passes the configured canonical root to Windows File Explorer. React cannot provide a path or process argument, and the main webview receives no generic filesystem, shell, or process capability.

Sanitized error reports contain stable error codes, application/component versions, non-secret configuration categories, and opaque correlation IDs. They exclude API keys, authorization headers, prompts, transcript text, raw provider bodies, and user paths where unnecessary.

## Consent notice

The application must display this meaning during onboarding and before first capture:

> You are responsible for obtaining any consent required to record or transcribe audio and for complying with the laws, workplace policies, and contractual obligations that apply to every participant and location in the meeting.

KokoroKoe does not determine whether recording or transcription is lawful for a particular meeting.

## Windows audio limitations

The P3-001 transport requires explicit consent acknowledgement before its prototype start command succeeds. Capture packets remain in separate bounded Rust memory queues and are consumed without persistence; only device metadata and aggregate health/format/timestamp/drop counters can cross to the authorized main window. This prototype guard does not replace the future first-capture consent UI.

P3-002 processes those packets only in Rust. Each source is decoded, downmixed, and resampled to bounded 16 kHz mono chunks in memory; the prototype sink immediately discards them. The frontend receives only aggregate counts and throttled RMS/peak/clipping metadata. Samples never enter Tauri events, React state, logs, SQLite, Markdown, or external requests, and neither captured nor normalized audio is retained.

P3-003 runs an independent local Earshot detector and bounded utterance segmenter for each normalized source. Active samples are held only in Rust memory, hard-split at 30 seconds, counted, and immediately discarded; neither VAD frames nor utterance samples cross a command/event boundary or enter persistence. The frontend status receives only aggregate classifications, reset/rejection/split counts, bounded buffer occupancy, and the latest finalized timing/end reason. The Silero comparison model is development-only and is not packaged or downloaded by the application.

P3-004 transcribes only explicit finalized utterances inside a local Rust-owned CPU runtime. The opt-in probe decodes one user-supplied MP3 in memory and passes bounded samples through the production VAD and pinned multilingual Tiny/Base models. It prints aggregate timing and result counts only; transcript text, detected language, samples, paths, source audio, model weights, and native build output are not logged, persisted, committed, or sent to React or a service. The application still has no model downloader, live transcription command/event, audio retention, or transcript persistence path.

P3-005 runs optional Vulkan model loading and inference only in isolated probe children. Its speech fixture is generated locally outside the repository, bounded to one finalized utterance, and reused for each CPU/Vulkan case. Child output is suppressed; the gate prints only attestation, isolation, result-count, duplicate/loss, and worker-decision flags. A forced missing driver or native inference abort cannot expose audio or terminate the parent. No audio, transcript text, detected language, path, model, SDK, DLL, crash dump, or driver detail enters React, persistence, logs, the repository, or a service.

P3-006 schedules only generated in-memory test jobs. Production-shaped jobs own bounded `Arc<[f32]>` sample buffers in Rust; the two-hour soak reuses one synthetic buffer and measures logical queued sample storage rather than process RSS. Pending finals are capped at 600 seconds, 9,600,000 samples, and 4,096 jobs. At most two partial jobs exist, one per source, and they are discarded when final lag reaches 20 seconds. Rejected finals return a fixed source/timeline gap record. The prototype neither transcribes nor exposes, persists, logs, or transmits sample content.

P3-007 adds only aggregate capture-health fields to the existing strict status contract. The deterministic two-hour clock test generates timestamps and no audio. The opt-in live probe injects one fixed test-only microphone-loop failure after real dual-source capture begins, retains samples only in the existing bounded Rust pipeline, prints fixed counters without endpoint metadata, and exposes no production injection command, capability, event, or frontend control.

P3-008 sends one bounded finalized utterance at a time through anonymous local pipes to the supervised Vulkan worker. Samples and complete transcript results exist transiently in Rust-owned process memory and are never written, logged, emitted to React, or transmitted over a network. The exact probe reuses the external generated P3-005 fixture and prints only aggregate result/failure counts. Worker startup paths remain inside local IPC and fixed failures expose no paths. All malformed/hang/termination/descendant modes are debug-only and absent from release builds.

P5-013 uses that same supervised local worker for product partial and final transcription when the separately staged private Vulkan runtime attests successfully. The worker receives only the already bounded local sample request and verified model path over inherited anonymous pipes; it has no network or frontend permission. Incomplete accelerated output is discarded, the child is terminated, and the same utterance is retried once by the separately staged CPU runtime. The Vulkan-only product gate substitutes an invalid CPU adapter, prints only aggregate partial/final/gap counts, and therefore proves accelerated execution without retaining or printing transcript text or audio.

P3-009 connects processing/VAD outcomes to scheduling and inference entirely inside Rust. Its ordering frontier contains timestamps and buffer position only, never samples or text. Deterministic integration tests use generated constant samples and fixed result text, retain them only in bounded test memory, and print aggregate counts. Results and explicit gaps remain internal prototype values; no Tauri command/event, React state, file, database, log, retained-audio, or network boundary is added.

P3-010 adds a Rust-owned HTTPS model-download boundary. P5-015 extends the closed catalog from two to three entries with one quantized Large-v3 Turbo artifact; callers still cannot supply a URL or filesystem path. Partial and final model files contain third-party weights under the app-local model root, never user audio or transcript content. The P5-015 external gate keeps its model, generated speech, decoded samples, native build, and output logs under `%LOCALAPPDATA%` and prints only aggregate timing/accounting metrics.

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
- P3-003's generated bake-off corpus is synthetic and favors Earshot; it does not establish performance across real speakers, languages, rooms, noise, music, echo, overlapping speech, or devices. VAD can still miss speech or classify non-speech as speech.
- P3-004's local MP3 is one unlabelled user recording on one CPU, not an accuracy, language, noise, minimum-hardware, or redistributable fixture. Base met the real-time gate but missed the two-second p95 target on seven VAD-finalized utterances.
- P3-005 covers one generated English utterance and one NVIDIA GPU/driver. P3-008 implements the local worker/IPC boundary, but accuracy and compatibility across AMD, Intel, multi-GPU, old drivers, device loss, suspend/resume, and packaged installation layouts remain unproven.
- P3-006 proves the scheduler state machine, and P3-009 proves its Rust-only finalized-VAD/worker-owner integration under deterministic delayed-source pressure. Neither measures allocator/RSS behavior, real thread/channel contention, device recovery during inference, or end-to-end UI/persistence delivery. A rejected, failed, or cancelled final is represented by a gap object, but persistence and UI presentation of that gap remain future work.
- P3-007 calculates shared-QPC timeline error rather than measuring physical acoustic latency or crystal drift between real devices. Its live recovery proof uses one test-only injected microphone-stream failure on one default input/render pair; real unplug/hotplug, fixed-device removal, default switching, Windows Audio restart, suspend/resume, Bluetooth, dock, USB, virtual-device, Remote Desktop, and multi-driver recovery remain unproven.
- P3-009 wires processing/VAD outcomes, ordering, scheduling, and the supervised owner as an in-process Rust prototype, but it does not start product capture sessions or expose transcript results. The worker still lacks model-catalog/installer wiring, signed-binary validation, CPU-only/AMD/Intel coverage, repeated restart policy, device-loss recovery, parent-crash testing, preemptible CPU inference, or memory/RSS measurement. Anonymous pipes are local and private to the spawned child; protocol encryption/authentication is intentionally absent because no cross-user or network transport exists.
- P3-010 proves deterministic installation state transitions and exact existing Tiny/Base hashes, not a real slow/interrupted internet transfer, proxy/authenticated network, bandwidth limit, disk-full race, power-loss durability, concurrent downloader, model redistribution, or packaged-worker layout. Its 512 MiB Tiny and 768 MiB Base memory values are guidance rather than measured RSS or minimum-hardware guarantees. Selection is intentionally non-persistent, and corrupt model cache files are deleted for verified redownload.
- P5-015 proves the exact Turbo artifact and one generated English phrase on one Windows 11 x64 RTX 3070 Ti/Ryzen 7 3700X host. Vulkan RTF was 0.0812; CPU load plus fallback took 18.518 seconds for 4.658 seconds of audio. CPU therefore preserves completion after worker failure but is not presented as real-time for this heavy model. Windows 10, AMD/Intel, CPU-only minimum hardware, installer lifecycle, broad languages/accents/noise, long meetings, and model redistribution remain unverified.

## Transcription limitations

- Whisper is not a native streaming engine. Partial text is produced by repeated inference over recent audio and may change substantially before finalization.
- Accuracy and latency vary with hardware, selected model, language, accent, noise, overlap, and microphone quality.
- Whisper can hallucinate during noise or silence, repeat text, or omit speech. VAD and final-pass processing reduce but cannot eliminate these failures.
- Confidence is displayed only when the engine supplies defensible probability data.
- When inference remains slower than incoming audio, partials are disabled first. At the hard bounded backlog limit, the application records an explicit gap rather than exhausting memory.
- Vulkan availability does not guarantee successful acceleration. Vulkan runs in a supervised worker; worker startup, timeout, protocol, exit, or crash failure discards incomplete output and retries the same finalized utterance once on CPU.
- A persisted Session language is normalized to a supported Whisper primary code and frozen for both Vulkan and CPU inference. Unsupported primary codes stop startup before capture; the separate transient live probe retains automatic language detection. A configured language can reduce detection ambiguity but does not guarantee correct recognition or translation.

## OpenRouter limitations

- OpenRouter and downstream providers are external services with their own availability, retention, pricing, moderation, and cancellation behavior.
- Model metadata and prices can change; local catalog data must be refreshed and actual response usage reconciled.
- A local per-session spending limit is enforced through conservative reservation plus actual response usage, but an in-flight provider request cannot be guaranteed to stop billing immediately after cancellation.
- Streaming requests can fail after HTTP 200; partial output is provisional until the complete validated response is received.
- Strict zero-data-retention routing can reduce the available model/provider set.
- A privacy-qualified model can temporarily have no provider endpoint that simultaneously supports ZDR, data-collection denial, and structured JSON output. KokoroKoe reports this separately and asks the user to refresh the catalog or select another model.
- OpenRouter unavailability never stops local capture, transcription, persistence, or recovery. A summary may remain deferred and be retried later.
- Recent-insight generation is an explicit per-request user action. It sends only the last twelve finalized transcript segments of the selected Session, together with the frozen project/session/preset context, and never sends audio, paths, devices, credentials, or the full transcript. Nothing is generated in the background or on a timer.
- Generated insights are transient. They are shown once in the Session card and are never written to Markdown, SQLite, logs, or events, so they are not recoverable after the panel closes; the finalized transcript they were derived from remains saved locally.
- The final session summary is an explicit per-request user action on a finished Session. It sends a bounded selection of the transcript — an evenly spaced sample spanning the Session plus its closing segments — and never audio, paths, devices, or credentials. It is never generated automatically when a Session stops, and OpenRouter being unavailable never prevents stopping a Session.
- Unlike insights, the summary is persisted: it is written to `summary.md` beside the session transcript, and regenerating replaces that document while retaining one `.bak` copy. The saved summary is model-generated text stored on the user's own machine; deleting the file removes it.
- A summary states how many transcript segments it covered. When the transcript is larger than the selected model's context, the summary is built from a sample rather than the complete transcript, so it can omit material discussed between sampled points.

## Window limitations

- The detached transcript window holds an event-subscription-only capability: it can invoke no command, read no file, reach no network, and manage no window. It receives the same locally emitted transcript events the main window does and displays them; nothing leaves the machine because a second window is open.
- A detached window shows only transcript records emitted while it is open. Opening it partway through a Session does not replay earlier speech; the saved transcript remains the record of the whole Session.
- The detached window displays transcript text on screen in its own frame. Background opacity, always-on-top, compact mode, and a system-wide show/hide shortcut are available, so the transcript can be hidden quickly; still consider who can see the display before opening it in a shared or screen-shared environment.
- The show/hide shortcut is a system-wide hotkey: while KokoroKoe is running it reserves that key combination from every application. It requires Ctrl, Alt, or Win so it cannot capture ordinary typing, it can be turned off entirely, and KokoroKoe registers exactly one such combination. It records no other keystroke and contains no key logging.
- Hiding the window is always reversible: the same combination shows it again, and the main window keeps a visible control that reopens it even when the shortcut is disabled or refused by the system.
- Lowering opacity dims the window background only; transcript text stays fully opaque and readable. Opacity cannot be reduced below a readable floor, so the window can never be made invisible and unrecoverable. A translucent window still shows transcript text over whatever is behind it, which can make that text visible in a screen recording of another application.
- The transcript window's appearance, position, and size are remembered between runs in the local non-secret settings database. This records only window geometry and appearance: no transcript text, session identity, or project content is stored with it. Deleting the local settings database resets it.
- A remembered position is restored only when a currently attached monitor still shows a grabbable portion of the window, so disconnecting a display cannot leave the window stranded off-screen.
- Click-through can be switched on so the transcript window stops accepting the mouse and clicks reach whatever is behind it. While it is on the window cannot be moved, resized, or closed, so the switch lives in the main window and three rules keep pointer control recoverable: it is never written to disk and so never survives a restart or a crash, it cannot be switched on while the main window is absent, and it is switched off automatically when the main window closes. The transcript window shows an explicit indicator while it is on, because a window passing clicks through otherwise looks exactly like a frozen one.

## Desktop-window limitations

Readable opacity requires transparent native windows with alpha applied to CSS background layers, not whole-window opacity that fades text. Click-through ships because recovery does not depend on the user remembering anything: it is never persisted, it cannot be engaged without the main window, and closing the main window turns it off. Multi-monitor positions must be clamped when a display disappears or its scale changes.
