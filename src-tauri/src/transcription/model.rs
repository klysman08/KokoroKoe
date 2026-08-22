use std::fmt;

use crate::audio::{AudioSource, DetectedUtterance};

pub(crate) const TRANSCRIPTION_SAMPLE_RATE: usize = 16_000;
pub(crate) const MAX_TRANSCRIPTION_SAMPLES: usize = 480_000;
pub(crate) const MAX_TRANSCRIPT_BYTES: usize = 1_048_576;
pub(crate) const MAX_TRANSCRIPT_SEGMENTS: usize = 256;
pub(crate) const AUTO_WHISPER_LANGUAGE: &str = "auto";

const WHISPER_LANGUAGE_CODES: &[&str] = &[
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
    "ha", "ba", "jw", "su", "yue",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhisperModelKind {
    Tiny,
    Base,
    LargeV3TurboQ5_0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum WhisperBackend {
    Cpu = 0,
    Vulkan = 1,
}

impl WhisperBackend {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Vulkan => "vulkan",
        }
    }
}

impl WhisperModelKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::LargeV3TurboQ5_0 => "large-v3-turbo-q5_0",
        }
    }
}

pub(crate) fn normalize_session_language(value: &str) -> Result<String, TranscriptionError> {
    let primary = value
        .split('-')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if WHISPER_LANGUAGE_CODES.contains(&primary.as_str()) {
        Ok(primary)
    } else {
        Err(TranscriptionError::UnsupportedLanguage)
    }
}

pub(crate) fn validate_runtime_language(value: &str) -> Result<(), TranscriptionError> {
    if value == AUTO_WHISPER_LANGUAGE || WHISPER_LANGUAGE_CODES.contains(&value) {
        Ok(())
    } else {
        Err(TranscriptionError::UnsupportedLanguage)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TranscriptionRequest<'a> {
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) samples: &'a [f32],
}

impl<'a> From<&'a DetectedUtterance> for TranscriptionRequest<'a> {
    fn from(utterance: &'a DetectedUtterance) -> Self {
        Self {
            source: utterance.source,
            start_ms: utterance.start_ms,
            end_ms: utterance.end_ms,
            samples: &utterance.samples,
        }
    }
}

impl TranscriptionRequest<'_> {
    pub(crate) fn validate(self) -> Result<Self, TranscriptionError> {
        if self.samples.is_empty() || self.samples.len() > MAX_TRANSCRIPTION_SAMPLES {
            return Err(TranscriptionError::InvalidAudio);
        }
        if self
            .samples
            .iter()
            .any(|sample| !sample.is_finite() || !(-1.0..=1.0).contains(sample))
        {
            return Err(TranscriptionError::InvalidAudio);
        }
        let expected_duration_ms =
            self.samples.len() as u64 * 1_000 / TRANSCRIPTION_SAMPLE_RATE as u64;
        if self.end_ms <= self.start_ms
            || self.end_ms.saturating_sub(self.start_ms) != expected_duration_ms
        {
            return Err(TranscriptionError::InvalidTimeline);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptSegment {
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptionResult {
    pub(crate) source: AudioSource,
    pub(crate) utterance_start_ms: u64,
    pub(crate) utterance_end_ms: u64,
    pub(crate) language: String,
    pub(crate) text: String,
    pub(crate) segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranscriptionError {
    AdapterUnavailable,
    AdapterIncompatible,
    ModelUnavailable,
    ModelLoadFailed,
    UnsupportedLanguage,
    BackendUnavailable,
    InvalidAudio,
    InvalidTimeline,
    InferenceFailed,
    InvalidNativeResult,
    WorkerStartupFailed,
    WorkerProtocolFailed,
    WorkerWriteTimeout,
    WorkerInferenceTimeout,
    WorkerCancelled,
    WorkerTerminated,
}

impl TranscriptionError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::AdapterUnavailable => "transcription_adapter_unavailable",
            Self::AdapterIncompatible => "transcription_adapter_incompatible",
            Self::ModelUnavailable => "transcription_model_unavailable",
            Self::ModelLoadFailed => "transcription_model_load_failed",
            Self::UnsupportedLanguage => "transcription_language_unsupported",
            Self::BackendUnavailable => "transcription_backend_unavailable",
            Self::InvalidAudio => "transcription_audio_invalid",
            Self::InvalidTimeline => "transcription_timeline_invalid",
            Self::InferenceFailed => "transcription_inference_failed",
            Self::InvalidNativeResult => "transcription_native_result_invalid",
            Self::WorkerStartupFailed => "transcription_worker_startup_failed",
            Self::WorkerProtocolFailed => "transcription_worker_protocol_failed",
            Self::WorkerWriteTimeout => "transcription_worker_write_timeout",
            Self::WorkerInferenceTimeout => "transcription_worker_inference_timeout",
            Self::WorkerCancelled => "transcription_worker_cancelled",
            Self::WorkerTerminated => "transcription_worker_terminated",
        }
    }
}

impl fmt::Display for TranscriptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for TranscriptionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptionFailure {
    pub(crate) source: AudioSource,
    pub(crate) utterance_start_ms: u64,
    pub(crate) utterance_end_ms: u64,
    pub(crate) code: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TranscriptionOutcome {
    Final(TranscriptionResult),
    Failed(TranscriptionFailure),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_language_normalizes_supported_bcp47_primary_tags() {
        assert_eq!(normalize_session_language("en-GB").unwrap(), "en");
        assert_eq!(normalize_session_language("PT-br").unwrap(), "pt");
        assert_eq!(normalize_session_language("yue-Hant-HK").unwrap(), "yue");
    }

    #[test]
    fn session_language_rejects_auto_and_unsupported_primary_tags() {
        assert_eq!(
            normalize_session_language("auto"),
            Err(TranscriptionError::UnsupportedLanguage)
        );
        assert_eq!(
            normalize_session_language("zz-ZZ"),
            Err(TranscriptionError::UnsupportedLanguage)
        );
    }

    #[test]
    fn runtime_language_accepts_only_auto_or_normalized_codes() {
        assert!(validate_runtime_language(AUTO_WHISPER_LANGUAGE).is_ok());
        assert!(validate_runtime_language("en").is_ok());
        assert_eq!(
            validate_runtime_language("en-GB"),
            Err(TranscriptionError::UnsupportedLanguage)
        );
    }
}
