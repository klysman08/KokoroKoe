use std::{
    cell::Cell,
    ffi::{CStr, CString, c_char, c_float, c_int, c_void},
    marker::PhantomData,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
    sync::Mutex,
};

use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    LoadLibraryExW,
};

use super::{
    AUTO_WHISPER_LANGUAGE, MAX_TRANSCRIPT_BYTES, MAX_TRANSCRIPT_SEGMENTS, TranscriptSegment,
    TranscriptionEngine, TranscriptionError, TranscriptionRequest, TranscriptionResult,
    WhisperBackend, WhisperModelKind, validate_runtime_language,
};

#[cfg(test)]
use super::model::{MAX_TRANSCRIPTION_SAMPLES, TRANSCRIPTION_SAMPLE_RATE};

const ADAPTER_API_VERSION: u32 = 3;
const MAX_LANGUAGE_BYTES: usize = 15;
static ADAPTER_API: Mutex<Option<(PathBuf, &'static AdapterApi)>> = Mutex::new(None);

type ApiVersionFn = unsafe extern "C" fn() -> u32;
type ModelLoadFn =
    unsafe extern "C" fn(*const c_char, *const c_char, c_int, c_int, *mut *mut c_void) -> c_int;
type ModelFreeFn = unsafe extern "C" fn(*mut c_void);
type ModelBackendFn = unsafe extern "C" fn(*const c_void) -> c_int;
type TranscribeFn =
    unsafe extern "C" fn(*mut c_void, *const c_float, usize, *mut *mut c_void) -> c_int;
type ResultFreeFn = unsafe extern "C" fn(*mut c_void);
type ResultLanguageFn = unsafe extern "C" fn(*const c_void) -> *const c_char;
type ResultSegmentCountFn = unsafe extern "C" fn(*const c_void) -> usize;
type ResultSegmentTimeFn = unsafe extern "C" fn(*const c_void, usize) -> i64;
type ResultSegmentTextFn = unsafe extern "C" fn(*const c_void, usize) -> *const c_char;

#[derive(Debug, Clone)]
pub(crate) struct WhisperConfig {
    pub(crate) adapter_path: PathBuf,
    pub(crate) model_path: PathBuf,
    pub(crate) model_kind: WhisperModelKind,
    pub(crate) language: String,
    pub(crate) threads: usize,
    pub(crate) backend: WhisperBackend,
}

impl WhisperConfig {
    pub(crate) fn cpu(
        adapter_path: impl Into<PathBuf>,
        model_path: impl Into<PathBuf>,
        model_kind: WhisperModelKind,
        threads: usize,
    ) -> Self {
        Self {
            adapter_path: adapter_path.into(),
            model_path: model_path.into(),
            model_kind,
            language: AUTO_WHISPER_LANGUAGE.to_owned(),
            threads,
            backend: WhisperBackend::Cpu,
        }
    }

    pub(crate) fn vulkan(
        adapter_path: impl Into<PathBuf>,
        model_path: impl Into<PathBuf>,
        model_kind: WhisperModelKind,
        threads: usize,
    ) -> Self {
        Self {
            adapter_path: adapter_path.into(),
            model_path: model_path.into(),
            model_kind,
            language: AUTO_WHISPER_LANGUAGE.to_owned(),
            threads,
            backend: WhisperBackend::Vulkan,
        }
    }

    pub(crate) fn with_language(
        mut self,
        language: impl Into<String>,
    ) -> Result<Self, TranscriptionError> {
        self.language = language.into();
        validate_runtime_language(&self.language)?;
        Ok(self)
    }
}

struct AdapterApi {
    // whisper.cpp dynamically registers GGML backends that are unsafe to unload/reload. Keep the
    // native module for process lifetime while model and result handles remain explicit.
    module: usize,
    model_load: ModelLoadFn,
    model_free: ModelFreeFn,
    model_backend: ModelBackendFn,
    transcribe: TranscribeFn,
    result_free: ResultFreeFn,
    result_language: ResultLanguageFn,
    result_segment_count: ResultSegmentCountFn,
    result_segment_start_10ms: ResultSegmentTimeFn,
    result_segment_end_10ms: ResultSegmentTimeFn,
    result_segment_text: ResultSegmentTextFn,
}

impl AdapterApi {
    fn shared(path: &Path) -> Result<&'static Self, TranscriptionError> {
        let canonical_path = path
            .canonicalize()
            .map_err(|_| TranscriptionError::AdapterUnavailable)?;
        let mut loaded = ADAPTER_API
            .lock()
            .map_err(|_| TranscriptionError::AdapterUnavailable)?;
        if let Some((loaded_path, api)) = loaded.as_ref() {
            return if loaded_path == &canonical_path {
                Ok(*api)
            } else {
                Err(TranscriptionError::AdapterIncompatible)
            };
        }
        let api = Box::leak(Box::new(Self::load(&canonical_path)?));
        *loaded = Some((canonical_path, api));
        Ok(api)
    }

    fn load(path: &Path) -> Result<Self, TranscriptionError> {
        if !path.is_file() {
            return Err(TranscriptionError::AdapterUnavailable);
        }
        let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide_path.push(0);
        let module = unsafe {
            // SAFETY: `wide_path` is NUL-terminated and remains alive for the call. The restricted
            // search flags make dependent DLL resolution begin in the adapter's own directory.
            LoadLibraryExW(
                wide_path.as_ptr(),
                ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        };
        if module.is_null() {
            return Err(TranscriptionError::AdapterUnavailable);
        }

        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                let address = unsafe {
                    // SAFETY: `module` is a live library handle and every literal below is
                    // NUL-terminated. The API-version check below freezes the signatures.
                    GetProcAddress(module, concat!($name, "\0").as_ptr())
                };
                let Some(address) = address else {
                    return Err(TranscriptionError::AdapterIncompatible);
                };
                unsafe {
                    // SAFETY: the KokoroKoe adapter header fixes each exported signature for API
                    // version 3. The version is checked before any model operation.
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(address)
                }
            }};
        }

        let api_version = symbol!("kk_whisper_api_version", ApiVersionFn);
        let api = Self {
            module: module as usize,
            model_load: symbol!("kk_whisper_model_load", ModelLoadFn),
            model_free: symbol!("kk_whisper_model_free", ModelFreeFn),
            model_backend: symbol!("kk_whisper_model_backend", ModelBackendFn),
            transcribe: symbol!("kk_whisper_transcribe", TranscribeFn),
            result_free: symbol!("kk_whisper_result_free", ResultFreeFn),
            result_language: symbol!("kk_whisper_result_language", ResultLanguageFn),
            result_segment_count: symbol!("kk_whisper_result_segment_count", ResultSegmentCountFn),
            result_segment_start_10ms: symbol!(
                "kk_whisper_result_segment_start_10ms",
                ResultSegmentTimeFn
            ),
            result_segment_end_10ms: symbol!(
                "kk_whisper_result_segment_end_10ms",
                ResultSegmentTimeFn
            ),
            result_segment_text: symbol!("kk_whisper_result_segment_text", ResultSegmentTextFn),
        };
        if unsafe { (api_version)() } != ADAPTER_API_VERSION {
            return Err(TranscriptionError::AdapterIncompatible);
        }
        Ok(api)
    }
}

pub(crate) struct WhisperEngine {
    api: &'static AdapterApi,
    model: usize,
    model_kind: WhisperModelKind,
    backend: WhisperBackend,
    _not_sync: PhantomData<Cell<()>>,
}

impl WhisperEngine {
    pub(crate) fn load(config: WhisperConfig) -> Result<Self, TranscriptionError> {
        if !config.model_path.is_file() {
            return Err(TranscriptionError::ModelUnavailable);
        }
        if !(1..=64).contains(&config.threads) {
            return Err(TranscriptionError::ModelLoadFailed);
        }
        validate_runtime_language(&config.language)?;
        let model_path = config
            .model_path
            .to_str()
            .ok_or(TranscriptionError::ModelUnavailable)?;
        let model_path =
            CString::new(model_path).map_err(|_| TranscriptionError::ModelUnavailable)?;
        let language =
            CString::new(config.language).map_err(|_| TranscriptionError::UnsupportedLanguage)?;
        let api = AdapterApi::shared(&config.adapter_path)?;
        let mut model = ptr::null_mut();
        let status = unsafe {
            // SAFETY: the adapter API was version-checked, the path is a live C string, the thread
            // count is bounded, and `model` is a valid output slot.
            (api.model_load)(
                model_path.as_ptr(),
                language.as_ptr(),
                config.threads as c_int,
                config.backend as c_int,
                &mut model,
            )
        };
        if status == 4 {
            return Err(TranscriptionError::BackendUnavailable);
        }
        if status != 0 || model.is_null() {
            return Err(TranscriptionError::ModelLoadFailed);
        }
        let loaded_backend = unsafe {
            // SAFETY: the non-null model handle is owned here and the adapter API was version-checked.
            (api.model_backend)(model)
        };
        if loaded_backend != config.backend as c_int {
            unsafe {
                // SAFETY: this branch still uniquely owns the successfully loaded model.
                (api.model_free)(model);
            }
            return Err(TranscriptionError::AdapterIncompatible);
        }
        Ok(Self {
            api,
            model: model as usize,
            model_kind: config.model_kind,
            backend: config.backend,
            _not_sync: PhantomData,
        })
    }

    pub(crate) const fn model_kind(&self) -> WhisperModelKind {
        self.model_kind
    }

    pub(crate) const fn backend(&self) -> WhisperBackend {
        self.backend
    }

    fn copy_native_text(
        pointer: *const c_char,
        maximum_bytes: usize,
    ) -> Result<String, TranscriptionError> {
        if pointer.is_null() {
            return Err(TranscriptionError::InvalidNativeResult);
        }
        let bytes = unsafe {
            // SAFETY: adapter-owned result strings are guaranteed NUL-terminated and remain alive
            // until `kk_whisper_result_free` is called by the guard below.
            CStr::from_ptr(pointer).to_bytes()
        };
        if bytes.len() > maximum_bytes {
            return Err(TranscriptionError::InvalidNativeResult);
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| TranscriptionError::InvalidNativeResult)
    }
}

impl TranscriptionEngine for WhisperEngine {
    fn transcribe(
        &mut self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        let request = request.validate()?;
        let mut native_result = ptr::null_mut();
        let status = unsafe {
            // SAFETY: `self.model` is owned by this non-Sync engine, samples are finite and bounded,
            // and the output pointer remains valid for the duration of the call.
            (self.api.transcribe)(
                self.model as *mut c_void,
                request.samples.as_ptr(),
                request.samples.len(),
                &mut native_result,
            )
        };
        if status != 0 || native_result.is_null() {
            return Err(TranscriptionError::InferenceFailed);
        }
        let result_guard = NativeResult {
            pointer: native_result as usize,
            free: self.api.result_free,
        };

        let segment_count = unsafe {
            // SAFETY: the non-null result handle is alive under `result_guard`.
            (self.api.result_segment_count)(native_result)
        };
        if segment_count > MAX_TRANSCRIPT_SEGMENTS {
            return Err(TranscriptionError::InvalidNativeResult);
        }
        let language = Self::copy_native_text(
            unsafe { (self.api.result_language)(native_result) },
            MAX_LANGUAGE_BYTES,
        )?;
        if language.is_empty()
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        {
            return Err(TranscriptionError::InvalidNativeResult);
        }

        let mut segments = Vec::with_capacity(segment_count);
        let mut text = String::new();
        let duration_ms = request.end_ms - request.start_ms;
        let mut previous_end_ms = request.start_ms;
        for index in 0..segment_count {
            let relative_start =
                unsafe { (self.api.result_segment_start_10ms)(native_result, index) };
            let relative_end = unsafe { (self.api.result_segment_end_10ms)(native_result, index) };
            if relative_start < 0 || relative_end < relative_start {
                return Err(TranscriptionError::InvalidNativeResult);
            }
            let relative_start_ms = (relative_start as u64).saturating_mul(10).min(duration_ms);
            let relative_end_ms = (relative_end as u64).saturating_mul(10).min(duration_ms);
            let start_ms = request.start_ms.saturating_add(relative_start_ms);
            let end_ms = request.start_ms.saturating_add(relative_end_ms);
            if start_ms < previous_end_ms || end_ms < start_ms {
                return Err(TranscriptionError::InvalidNativeResult);
            }
            let remaining = MAX_TRANSCRIPT_BYTES.saturating_sub(text.len());
            let segment_text = Self::copy_native_text(
                unsafe { (self.api.result_segment_text)(native_result, index) },
                remaining,
            )?;
            text.push_str(&segment_text);
            previous_end_ms = end_ms;
            segments.push(TranscriptSegment {
                start_ms,
                end_ms,
                text: segment_text,
            });
        }
        drop(result_guard);

        Ok(TranscriptionResult {
            source: request.source,
            utterance_start_ms: request.start_ms,
            utterance_end_ms: request.end_ms,
            language,
            text: text.trim().to_owned(),
            segments,
        })
    }
}

impl Drop for WhisperEngine {
    fn drop(&mut self) {
        if self.model != 0 {
            unsafe {
                // SAFETY: the engine uniquely owns this model and the adapter API is still loaded.
                (self.api.model_free)(self.model as *mut c_void);
            }
            self.model = 0;
        }
    }
}

struct NativeResult {
    pointer: usize,
    free: ResultFreeFn,
}

impl Drop for NativeResult {
    fn drop(&mut self) {
        if self.pointer != 0 {
            unsafe {
                // SAFETY: the guard uniquely owns this adapter result handle.
                (self.free)(self.pointer as *mut c_void);
            }
            self.pointer = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        io::{Read, Write},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    use crate::audio::{AudioSource, DetectedUtterance, VadProcessOutcome, VadSegmenter};

    use super::*;

    const MAX_PROBE_AUDIO_SECONDS: usize = 600;

    fn decode_audio(path: &Path) -> Vec<f32> {
        assert!(path.is_file(), "probe audio must exist");
        let mut child = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("ffmpeg must be installed for the explicit probe");
        let byte_limit = MAX_PROBE_AUDIO_SECONDS * TRANSCRIPTION_SAMPLE_RATE * 4;
        let mut bytes = Vec::with_capacity(byte_limit.min(32 * 1024 * 1024));
        child
            .stdout
            .take()
            .expect("ffmpeg stdout")
            .take(byte_limit as u64 + 1)
            .read_to_end(&mut bytes)
            .expect("read bounded ffmpeg output");
        if bytes.len() > byte_limit {
            child.kill().expect("stop oversized probe decode");
            child.wait().expect("reap oversized ffmpeg process");
            panic!("probe audio exceeds the ten-minute bound");
        }
        assert!(child.wait().expect("wait for ffmpeg").success());
        assert_eq!(bytes.len() % 4, 0, "ffmpeg output must be f32 aligned");
        let mut samples: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte sample")))
            .collect();
        assert!(
            samples.iter().all(|sample| sample.is_finite()),
            "decoded samples must be finite"
        );
        samples
            .iter_mut()
            .for_each(|sample| *sample = sample.clamp(-1.0, 1.0));
        samples
    }

    fn segment_audio(samples: &[f32]) -> Vec<DetectedUtterance> {
        let mut segmenter = VadSegmenter::new(AudioSource::SystemOutput);
        let mut outcome = VadProcessOutcome::default();
        for (index, chunk) in samples.chunks(160).enumerate() {
            outcome.merge(segmenter.push(AudioSource::SystemOutput, index as u64 * 10, chunk));
        }
        outcome.merge(segmenter.finish());
        assert!(!outcome.utterances.is_empty(), "VAD must find probe speech");
        assert_eq!(outcome.buffered_samples, 0);
        outcome.utterances
    }

    struct Metrics {
        kind: WhisperModelKind,
        utterances: usize,
        speech: Duration,
        model_load: Duration,
        inference: Duration,
        p95: Duration,
        nonempty_results: usize,
    }

    impl Metrics {
        fn rtf(&self) -> f64 {
            self.inference.as_secs_f64() / self.speech.as_secs_f64()
        }
    }

    fn benchmark_model(
        adapter: &Path,
        model: &Path,
        kind: WhisperModelKind,
        threads: usize,
        utterances: &[DetectedUtterance],
    ) -> Metrics {
        let load_started = Instant::now();
        let mut engine = WhisperEngine::load(WhisperConfig::cpu(adapter, model, kind, threads))
            .expect("load CPU Whisper engine");
        let model_load = load_started.elapsed();
        assert_eq!(engine.model_kind(), kind);

        let mut latencies = Vec::with_capacity(utterances.len());
        let mut speech = Duration::ZERO;
        let mut nonempty_results = 0;
        for utterance in utterances {
            let latency_started = Instant::now();
            let result = engine
                .transcribe(TranscriptionRequest::from(utterance))
                .expect("transcribe finalized utterance");
            latencies.push(latency_started.elapsed());
            speech += Duration::from_secs_f64(
                utterance.samples.len() as f64 / TRANSCRIPTION_SAMPLE_RATE as f64,
            );
            assert_eq!(result.source, utterance.source);
            assert_eq!(result.utterance_start_ms, utterance.start_ms);
            assert_eq!(result.utterance_end_ms, utterance.end_ms);
            if !result.text.is_empty() {
                nonempty_results += 1;
            }
        }
        assert!(
            nonempty_results > 0,
            "probe must produce local transcript text"
        );
        latencies.sort_unstable();
        let p95_index = (latencies.len() * 95).div_ceil(100).saturating_sub(1);
        let inference = latencies.iter().copied().sum();
        Metrics {
            kind,
            utterances: utterances.len(),
            speech,
            model_load,
            inference,
            p95: latencies[p95_index],
            nonempty_results,
        }
    }

    #[test]
    #[ignore = "explicit P3-004 local model/audio throughput probe"]
    fn whisper_cpu_throughput_gate() {
        let adapter = PathBuf::from(
            env::var_os("KOKOROKOE_WHISPER_ADAPTER").expect("adapter environment path"),
        );
        let tiny_model = PathBuf::from(
            env::var_os("KOKOROKOE_WHISPER_TINY_MODEL").expect("Tiny model environment path"),
        );
        let base_model = PathBuf::from(
            env::var_os("KOKOROKOE_WHISPER_BASE_MODEL").expect("Base model environment path"),
        );
        let audio = PathBuf::from(
            env::var_os("KOKOROKOE_TRANSCRIPTION_AUDIO").expect("audio environment path"),
        );
        let threads = env::var("KOKOROKOE_WHISPER_THREADS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map_or(4, usize::from)
                    .min(8)
            });
        assert!((1..=64).contains(&threads));
        assert!(fs::metadata(&audio).expect("probe audio metadata").len() > 0);

        let mut invalid_model = tempfile::NamedTempFile::new().expect("temporary invalid model");
        invalid_model
            .write_all(b"not a Whisper model")
            .expect("write invalid model marker");
        match WhisperEngine::load(WhisperConfig::cpu(
            &adapter,
            invalid_model.path(),
            WhisperModelKind::Tiny,
            threads,
        )) {
            Err(TranscriptionError::ModelLoadFailed) => {}
            Err(other) => panic!("unexpected bounded model error: {}", other.code()),
            Ok(_) => panic!("invalid model must not load"),
        }

        let samples = decode_audio(&audio);
        let utterances = segment_audio(&samples);
        for (kind, model) in [
            (WhisperModelKind::Tiny, tiny_model),
            (WhisperModelKind::Base, base_model),
        ] {
            let metrics = benchmark_model(&adapter, &model, kind, threads, &utterances);
            println!(
                "model={} cpu_only=true threads={} utterances={} speech_seconds={:.3} model_load_ms={} inference_ms={} rtf={:.4} final_p95_ms={} nonempty_results={}",
                metrics.kind.as_str(),
                threads,
                metrics.utterances,
                metrics.speech.as_secs_f64(),
                metrics.model_load.as_millis(),
                metrics.inference.as_millis(),
                metrics.rtf(),
                metrics.p95.as_millis(),
                metrics.nonempty_results,
            );
            assert!(
                metrics.rtf() < 1.0,
                "CPU aggregate RTF must remain below 1.0"
            );
        }
    }

    const P3_005_WORKER_TEST: &str = "transcription::whisper::tests::whisper_vulkan_probe_worker";
    const P3_005_STARTUP_UNAVAILABLE_EXIT: i32 = 21;
    const P3_005_WORKER_TIMEOUT: Duration = Duration::from_secs(60);

    fn decode_raw_fixture(path: &Path) -> Vec<f32> {
        let bytes = fs::read(path).expect("read generated raw fixture");
        assert!(!bytes.is_empty(), "generated raw fixture must not be empty");
        assert_eq!(bytes.len() % 4, 0, "raw fixture must be f32 aligned");
        assert!(
            bytes.len() <= MAX_TRANSCRIPTION_SAMPLES * 4,
            "raw fixture exceeds the finalized-utterance bound"
        );
        let samples: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte sample")))
            .collect();
        assert!(
            samples
                .iter()
                .all(|sample| sample.is_finite() && (-1.0..=1.0).contains(sample)),
            "raw fixture samples must be finite and normalized"
        );
        samples
    }

    fn p3_005_request(samples: &[f32]) -> TranscriptionRequest<'_> {
        let duration_ms = samples.len() as u64 * 1_000 / TRANSCRIPTION_SAMPLE_RATE as u64;
        TranscriptionRequest {
            source: AudioSource::SystemOutput,
            start_ms: 12_000,
            end_ms: 12_000 + duration_ms,
            samples,
        }
    }

    fn p3_005_paths() -> (PathBuf, PathBuf, PathBuf, usize) {
        let adapter = PathBuf::from(
            env::var_os("KOKOROKOE_WHISPER_ADAPTER").expect("adapter environment path"),
        );
        let model = PathBuf::from(
            env::var_os("KOKOROKOE_WHISPER_TINY_MODEL").expect("Tiny model environment path"),
        );
        let fixture =
            PathBuf::from(env::var_os("KOKOROKOE_P3_005_FIXTURE").expect("generated fixture path"));
        let threads = env::var("KOKOROKOE_WHISPER_THREADS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(8);
        assert!((1..=64).contains(&threads));
        (adapter, model, fixture, threads)
    }

    #[test]
    #[ignore = "isolated child for the explicit P3-005 Vulkan probe"]
    fn whisper_vulkan_probe_worker() {
        let mode = env::var("KOKOROKOE_P3_005_WORKER_MODE").expect("worker mode");
        let (adapter, model, fixture, threads) = p3_005_paths();
        let engine = WhisperEngine::load(WhisperConfig::vulkan(
            adapter,
            model,
            WhisperModelKind::Tiny,
            threads,
        ));

        if mode == "startup_unavailable" {
            match engine {
                Err(TranscriptionError::BackendUnavailable) => {
                    std::process::exit(P3_005_STARTUP_UNAVAILABLE_EXIT);
                }
                Err(other) => panic!("unexpected startup failure: {}", other.code()),
                Ok(_) => panic!("forced missing driver must not attest Vulkan"),
            }
        }

        let mut engine = engine.expect("load attested Vulkan engine");
        assert_eq!(engine.backend(), WhisperBackend::Vulkan);
        let samples = decode_raw_fixture(&fixture);
        let request = p3_005_request(&samples);
        let result = engine.transcribe(request).expect("Vulkan inference result");
        assert_eq!(result.source, request.source);
        assert_eq!(result.utterance_start_ms, request.start_ms);
        assert_eq!(result.utterance_end_ms, request.end_ms);
        assert!(!result.text.is_empty(), "generated speech must transcribe");
        assert_eq!(mode, "success", "fault injection should abort the worker");
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WorkerExit {
        Exited(Option<i32>),
        TimedOut,
    }

    fn run_p3_005_worker(
        mode: &str,
        broken_driver: Option<&Path>,
        abort_inference: bool,
    ) -> WorkerExit {
        let (adapter, model, fixture, threads) = p3_005_paths();
        let mut command = Command::new(env::current_exe().expect("current test executable"));
        command
            .args([P3_005_WORKER_TEST, "--exact", "--ignored"])
            .env("KOKOROKOE_P3_005_WORKER_MODE", mode)
            .env("KOKOROKOE_WHISPER_ADAPTER", adapter)
            .env("KOKOROKOE_WHISPER_TINY_MODEL", model)
            .env("KOKOROKOE_P3_005_FIXTURE", fixture)
            .env("KOKOROKOE_WHISPER_THREADS", threads.to_string())
            .env_remove("KOKOROKOE_P3_005_ABORT_VULKAN_INFERENCE")
            .env_remove("VK_DRIVER_FILES")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(path) = broken_driver {
            command.env("VK_DRIVER_FILES", path);
        }
        if abort_inference {
            command.env("KOKOROKOE_P3_005_ABORT_VULKAN_INFERENCE", "1");
        }

        let mut child = command.spawn().expect("spawn isolated Vulkan worker");
        let deadline = Instant::now() + P3_005_WORKER_TIMEOUT;
        loop {
            if let Some(status) = child.try_wait().expect("poll isolated Vulkan worker") {
                return WorkerExit::Exited(status.code());
            }
            if Instant::now() >= deadline {
                child.kill().expect("terminate timed-out Vulkan worker");
                child.wait().expect("reap timed-out Vulkan worker");
                return WorkerExit::TimedOut;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn transcribe_p3_005_cpu() -> TranscriptionResult {
        let (adapter, model, fixture, threads) = p3_005_paths();
        let samples = decode_raw_fixture(&fixture);
        let request = p3_005_request(&samples);
        let mut engine = WhisperEngine::load(WhisperConfig::cpu(
            adapter,
            model,
            WhisperModelKind::Tiny,
            threads,
        ))
        .expect("load CPU fallback engine");
        assert_eq!(engine.backend(), WhisperBackend::Cpu);
        let result = engine.transcribe(request).expect("CPU fallback result");
        assert_eq!(result.source, request.source);
        assert_eq!(result.utterance_start_ms, request.start_ms);
        assert_eq!(result.utterance_end_ms, request.end_ms);
        assert!(!result.text.is_empty(), "generated speech must transcribe");
        result
    }

    #[test]
    #[ignore = "explicit P3-005 Vulkan startup/crash isolation and CPU recovery probe"]
    fn whisper_vulkan_failure_isolation_gate() {
        assert_eq!(
            run_p3_005_worker("success", None, false),
            WorkerExit::Exited(Some(0)),
            "the accelerated path must attest and execute Vulkan"
        );

        let broken_driver = env::temp_dir().join("kokorokoe-p3-005-missing-vulkan-driver.json");
        assert!(!broken_driver.exists());
        assert_eq!(
            run_p3_005_worker("startup_unavailable", Some(&broken_driver), false),
            WorkerExit::Exited(Some(P3_005_STARTUP_UNAVAILABLE_EXIT)),
            "a missing Vulkan driver must fail with the fixed unavailable status"
        );
        let startup_recovery = transcribe_p3_005_cpu();

        let inference_exit = run_p3_005_worker("inference_abort", None, true);
        assert_ne!(inference_exit, WorkerExit::TimedOut);
        assert_ne!(inference_exit, WorkerExit::Exited(Some(0)));
        assert_ne!(
            inference_exit,
            WorkerExit::Exited(Some(101)),
            "the native fault hook must terminate below the Rust test boundary"
        );
        let inference_recovery = transcribe_p3_005_cpu();

        assert_eq!(startup_recovery.source, inference_recovery.source);
        assert_eq!(
            startup_recovery.utterance_start_ms,
            inference_recovery.utterance_start_ms
        );
        assert_eq!(
            startup_recovery.utterance_end_ms,
            inference_recovery.utterance_end_ms
        );
        println!(
            "vulkan_attested=true startup_failure_isolated=true inference_abort_isolated=true startup_cpu_results=1 inference_cpu_results=1 duplicate_results=0 lost_results=0 supervised_worker_required=true"
        );
    }
}
