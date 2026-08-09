# P3-004 Bounded Local Whisper Runtime Prototype

P3-004 proves a CPU-only local transcription boundary for finalized 16 kHz mono utterances. It does not connect transcription to live capture or React and does not add model catalog/download UI, transcript events, partial scheduling, persistence, retained audio, Vulkan, OpenRouter, or a product transcription screen.

## Runtime boundary

- Rust owns `TranscriptionEngine`, strict request/result/error contracts, one model-owning `TranscriptionOwner`, source-labelled outcomes, bounded native-result copying, and all paths and sample buffers.
- Requests accept only one finite normalized utterance of 1 to 480,000 samples with an exact source-local timeline. Native results are limited to 256 segments and 1 MiB of UTF-8 text.
- The first engine loads one multilingual Tiny or Base model and forces `use_gpu = false`. Model-load and inference failures return fixed codes without native messages or paths; one source failure does not poison the owner or the other source.
- A small KokoroKoe-owned C ABI shim calls the official MIT-licensed whisper.cpp API. This resolves R-006 without adding the Unlicense `whisper-rs` crate or another wrapper dependency.
- The native library handle has process lifetime because whisper.cpp dynamically registers GGML backends that fault when unloaded and reloaded. Model contexts and every inference result are still explicitly freed, and the architecture continues to permit only one loaded heavy model.

The prototype DLL is loaded from an explicit Rust-owned path with Windows restricted dependent-DLL search flags. It is not bundled by the application yet. Production model installation, verified deployment of the native DLL set, and product wiring remain separate bounded tasks.

## Pinned native and model inputs

- whisper.cpp tag `v1.9.2`, commit `306c88f4d1286aec1bf96e544632897886af5501`, MIT.
- Multilingual Tiny `ggml-tiny.bin`, SHA-256 `be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21`.
- Multilingual Base `ggml-base.bin`, SHA-256 `60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe`.

The opt-in script obtains the pinned source and model weights only in `%LOCALAPPDATA%\KokoroKoe\p3-004`. The application has no download path, and source, DLLs, models, decoded samples, and transcript text remain outside the repository.

## Frozen throughput method

The criteria were recorded before the first timing run:

1. Decode the user-supplied local MP3 through FFmpeg to finite clamped 16 kHz mono `f32`; reject more than ten minutes.
2. Run the production P3-003 Earshot segmenter in exact 10 ms chunks and use the same finalized utterances for both models.
3. Load and run each multilingual model CPU-only with eight threads. Report model load separately from inference.
4. Compute aggregate real-time factor as total inference wall time divided by finalized speech duration. Require RTF below 1.0 for each supported model.
5. Compute nearest-rank p95 from per-utterance final inference latency and report it against the architecture's two-second target. The target is diagnostic rather than the R-004 hard threshold.
6. Print only model kind, CPU/thread mode, utterance count, speech duration, timing, RTF, p95, and nonempty-result count. Never print transcript text, language content, samples, or local paths.

## Result on the current supported configuration

The final run used Windows 11 x64, an AMD Ryzen 7 3700X with 8 cores/16 logical processors, 31.9 GiB RAM, eight inference threads, and the user-supplied 5:06 MP3. Earshot finalized seven utterances containing 86.772 seconds of speech.

| Model | Load | Inference | Aggregate RTF | Final p95 | Nonempty results | Decision |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Tiny multilingual | 109 ms | 7,945 ms | 0.0916 | 1,831 ms | 6/7 | Passes R-004 and the two-second target |
| Base multilingual | 187 ms | 16,696 ms | 0.1924 | 3,637 ms | 6/7 | Passes R-004; misses the latency target |

Both models are substantially faster than real time on this supported configuration. Under ADR 0003's RTF-based default rule, Base remains the provisional default. Tiny is the lower-latency choice and is the only model that met the two-second p95 target here. A minimum-hardware run is still required before freezing the shipped default.

## Reproduce locally

With FFmpeg, Git, CMake, MSVC, Rust 1.88, and the supplied audio available:

```powershell
.\scripts\run-whisper-throughput-prototype.ps1
```

Use `-AudioPath`, `-ScratchPath`, or `-Threads` to change only the explicit local probe inputs. The ordinary locked Rust suite skips the model/audio-dependent test.

## Limitations

- One local recording on one desktop CPU is throughput evidence, not an accuracy, language, accent, noise, minimum-hardware, or device matrix.
- The MP3 is local user data, is ignored by Git, and is not a redistributable deterministic fixture. Its transcript was not reviewed, saved, or recorded as evidence.
- P95 is based on seven utterances and is sensitive to the 30-second hard split; Base missed the two-second target.
- Model installation/deletion, disk/memory reporting, native-DLL packaging, live-source scheduling, partial/final reconciliation, backpressure, transcript UI/events, and persistence are not implemented.
- Vulkan startup/failure recovery remains R-005. Minimum-hardware default selection and a licensed consented speech/noise quality matrix remain open.
