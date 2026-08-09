# Whisper model management prototype

P3-010 is a bounded Rust-only installation and lifecycle gate for the two curated multilingual Whisper models used by KokoroKoe. It intentionally has no Tauri command, React UI, live-session wiring, SQLite record, generic frontend network/filesystem permission, or packaging claim.

## Curated catalog

Both files come from the official `ggerganov/whisper.cpp` Hugging Face repository at immutable revision `5359861c739e955e79d9a303bcbc70fb988958b1`. The pinned URLs, exact lengths, SHA-256 values, MIT license link, approximate memory guidance, and supported CPU/Vulkan backends live in `src-tauri/src/models/catalog.rs`.

| Model | File | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Tiny multilingual | `ggml-tiny.bin` | 77,691,713 | `be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21` |
| Base multilingual | `ggml-base.bin` | 147,951,465 | `60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe` |

The memory figures are conservative compatibility guidance, not a promise about process RSS or a hardware support verdict. Insufficient disk is a hard pre-download failure; insufficient reported physical memory is a visible warning for later UI integration.

## Installation boundary

Rust owns the HTTP client, model root, filenames, resume sidecars, verification, selection, and deletion. A caller supplies only a catalog model ID. The model root is created and canonicalized once, rejects a link/reparse-point root, and all child names are fixed catalog constants.

Downloads use a 64 KiB copy buffer and a 30-minute request ceiling. A valid partial file plus strict bounded JSON metadata resumes with an HTTP byte range. A matching `206` appends; a `200` response safely truncates and restarts; mismatched lengths, content ranges, or excess bytes fail closed. Cancellation is checked between bounded reads and leaves a synced partial file for the next attempt.

Before installation, Rust verifies the exact file length and streams the entire file through SHA-256. A mismatch clears the staging pair. Installation is a same-directory rename after file sync, so unverified bytes never appear at the final catalog filename. A valid existing install is idempotent; a corrupt install is removed as a reproducible cache artifact and recovered through the same verified path.

Default selection is deliberately in-memory for this prototype and accepts only a fully verified installed catalog model. A selected model cannot be deleted until selection is cleared or changed. Deletion visits only the exact final/staging/metadata filenames derived from the selected catalog descriptor and rejects non-files.

## Verification

Ordinary unit tests use tiny in-memory bytes and a fake Rust download source. They cover exact catalog metadata, clean/idempotent installation, byte-range resume, safe restart when a server ignores the range, cancellation and resume, corrupt staging/install recovery, pre-network disk rejection, disk versus memory diagnostics, selection/deletion, unknown-ID containment, and strict content-range parsing.

The external exact gate reads the already downloaded P3-004 files without copying them into the repository or contacting the network:

```powershell
./scripts/run-whisper-model-installation-prototype.ps1
```

The blocking HTTP client must run on a dedicated blocking worker when product Tauri commands are added. P3-010 does not claim UI responsiveness, proxy/auth support, bandwidth throttling, background transfer persistence, worker/native-DLL packaging, model redistribution notices, or installer behavior.
