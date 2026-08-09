mod model;

#[cfg(windows)]
mod whisper;

pub(crate) use model::{
    MAX_TRANSCRIPT_BYTES, MAX_TRANSCRIPT_SEGMENTS, TranscriptSegment, TranscriptionError,
    TranscriptionFailure, TranscriptionOutcome, TranscriptionRequest, TranscriptionResult,
    WhisperModelKind,
};

pub(crate) trait TranscriptionEngine {
    fn transcribe(
        &mut self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionResult, TranscriptionError>;
}

pub(crate) struct TranscriptionOwner<E> {
    engine: E,
}

impl<E: TranscriptionEngine> TranscriptionOwner<E> {
    pub(crate) fn new(engine: E) -> Self {
        Self { engine }
    }

    pub(crate) fn transcribe(&mut self, request: TranscriptionRequest<'_>) -> TranscriptionOutcome {
        let source = request.source;
        let utterance_start_ms = request.start_ms;
        let utterance_end_ms = request.end_ms;
        match self.engine.transcribe(request) {
            Ok(result) => TranscriptionOutcome::Final(result),
            Err(error) => TranscriptionOutcome::Failed(TranscriptionFailure {
                source,
                utterance_start_ms,
                utterance_end_ms,
                code: error.code(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::audio::AudioSource;

    use super::*;

    struct SourceLocalFake {
        fail_source: AudioSource,
    }

    impl TranscriptionEngine for SourceLocalFake {
        fn transcribe(
            &mut self,
            request: TranscriptionRequest<'_>,
        ) -> Result<TranscriptionResult, TranscriptionError> {
            let request = request.validate()?;
            if request.source == self.fail_source {
                return Err(TranscriptionError::InferenceFailed);
            }
            Ok(TranscriptionResult {
                source: request.source,
                utterance_start_ms: request.start_ms,
                utterance_end_ms: request.end_ms,
                language: "en".to_owned(),
                text: "bounded result".to_owned(),
                segments: vec![TranscriptSegment {
                    start_ms: request.start_ms,
                    end_ms: request.end_ms,
                    text: "bounded result".to_owned(),
                }],
            })
        }
    }

    fn request(source: AudioSource, start_ms: u64) -> TranscriptionRequest<'static> {
        TranscriptionRequest {
            source,
            start_ms,
            end_ms: start_ms + 1_000,
            samples: &[0.1; 16_000],
        }
    }

    #[test]
    fn request_validation_enforces_finite_bounded_audio_and_exact_timeline() {
        assert!(request(AudioSource::Microphone, 0).validate().is_ok());

        let mut invalid = request(AudioSource::Microphone, 0);
        invalid.end_ms = 999;
        assert_eq!(invalid.validate(), Err(TranscriptionError::InvalidTimeline));

        let invalid_samples = [f32::NAN];
        let invalid = TranscriptionRequest {
            source: AudioSource::Microphone,
            start_ms: 0,
            end_ms: 0,
            samples: &invalid_samples,
        };
        assert_eq!(invalid.validate(), Err(TranscriptionError::InvalidAudio));
    }

    #[test]
    fn one_source_failure_does_not_poison_the_model_owner() {
        let mut owner = TranscriptionOwner::new(SourceLocalFake {
            fail_source: AudioSource::Microphone,
        });

        let failed = owner.transcribe(request(AudioSource::Microphone, 0));
        assert_eq!(
            failed,
            TranscriptionOutcome::Failed(TranscriptionFailure {
                source: AudioSource::Microphone,
                utterance_start_ms: 0,
                utterance_end_ms: 1_000,
                code: "transcription_inference_failed",
            })
        );

        let succeeded = owner.transcribe(request(AudioSource::SystemOutput, 1_000));
        let TranscriptionOutcome::Final(result) = succeeded else {
            panic!("system output should remain transcribable");
        };
        assert_eq!(result.source, AudioSource::SystemOutput);
        assert_eq!(result.text, "bounded result");
    }

    #[test]
    fn public_error_strings_never_include_native_or_path_details() {
        for error in [
            TranscriptionError::AdapterUnavailable,
            TranscriptionError::AdapterIncompatible,
            TranscriptionError::ModelUnavailable,
            TranscriptionError::ModelLoadFailed,
            TranscriptionError::InvalidAudio,
            TranscriptionError::InvalidTimeline,
            TranscriptionError::InferenceFailed,
            TranscriptionError::InvalidNativeResult,
        ] {
            assert_eq!(error.to_string(), error.code());
            assert!(!error.to_string().contains('\\'));
            assert!(!error.to_string().contains('/'));
        }
    }
}
