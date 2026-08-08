# P3-003 Source-Local VAD and Segmentation Prototype

## Boundary

P3-003 extends each P3-002 Rust processing worker with its own voice-activity detector and bounded utterance segmenter. The stable `microphone` and `system_output` sources never share detector, pre-roll, pending-frame, or active-utterance state. The prototype adds no Tauri command, permission, product event, UI, persistence, retained audio, transcription, model download, or network path.

Normalized 16 kHz mono chunks remain in Rust. Completed utterance samples are counted and immediately dropped by the prototype processing worker; only aggregate VAD counters and the latest utterance timing/end reason enter the existing strict status response.

## Frozen bake-off and result

The acceptance gates were recorded in `docs/project-memory.md` before the first measurement. Earshot 1.2.1 and Silero VAD v6 used threshold `0.5` and identical 512-sample (32 ms) evaluation windows. Earshot's score for each window is the maximum of its two required 256-sample frames. The generated in-memory corpus contains two four-second harmonic speech proxies, silence, deterministic low-amplitude noise, and a two-tone non-speech signal; no audio file or model artifact is checked in.

| Detector | Speech miss | Non-speech false positive | F1 | Test elapsed |
| --- | ---: | ---: | ---: | ---: |
| Earshot 1.2.1 | 0.40% | 1.07% | 0.9901 | 495 ms |
| Silero VAD v6 through `silero` 0.6.0 | 52.80% | 0.00% | 0.6413 | 132 ms |

Earshot passed the frozen requirements: at most 10% speech misses, at most 5% non-speech false positives, and F1 no more than two percentage points below Silero. It is therefore the production prototype dependency. The elapsed values describe one debug test run on one machine and are not product throughput claims. Silero remains development-only so the release does not carry ONNX Runtime or the 2,327,524-byte bundled comparison model.

Run the explicit comparison with:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib audio::vad::tests::earshot_meets_frozen_quality_gates_against_silero_v6 -- --ignored --exact --nocapture
```

## Segmentation contract

- Earshot consumes exact 256-sample (16 ms) frames at threshold `0.5`.
- A source retains at most 300 ms of pre-roll, finalizes after exactly 500 ms of trailing silence, and rejects detections containing less than 160 ms of speech-classified frames.
- An active utterance is hard-split at exactly 30 seconds. The next segment may carry the bounded 300 ms overlap.
- Format changes, normalized-timeline discontinuities, and end-of-stream finalize eligible active speech with an explicit end reason and reset detector state.
- Pending detector input stays below 256 samples. Total source-local VAD/segmenter storage is bounded at 485,055 samples (about 1.94 MB of `f32` data) per source.
- Diagnostics report classified frame counts, rejected short detections, forced splits, resets, buffer occupancy, finalized sample counts, and timing/end reason only.

Deterministic scripted-predictor tests prove the segmentation state machine independently of Earshot's statistical output, including exact pre-roll/tail timing, the minimum duration, hard split/overlap, discontinuity and format reset, finish behavior, bounded memory, and source isolation.

## Limitations

The bake-off corpus is deterministic and reproducible but synthetic: it is not evidence across speakers, languages, accents, rooms, microphones, music, keyboard noise, overlapping speech, or device recovery. The measured Silero miss rate shows that the corpus favors the embedded Earshot feature set and must not be generalized to real-world superiority. R-003 therefore remains open for a broader consented/licensed speech and noise matrix. Detector frames are 16 ms, so activity boundaries are quantized before exact sample-count trimming. P3-003 does not assign people, merge sources, cancel echo, retain utterances, run Whisper, or emit transcript events.

Dependency and model notices are recorded in [Dependency and License Baseline](dependency-licenses.md), and the live transport/normalization evidence remains in [P3-002 Bounded Audio-Processing Prototype](audio-processing-prototype.md).
