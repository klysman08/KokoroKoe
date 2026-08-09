use super::{TranscriptionEngine, TranscriptionError, TranscriptionRequest, TranscriptionResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CpuFallbackDiagnostics {
    pub(crate) acceleration_startup_failures: u64,
    pub(crate) acceleration_inference_failures: u64,
    pub(crate) accelerated_results: u64,
    pub(crate) cpu_fallback_attempts: u64,
    pub(crate) cpu_fallback_results: u64,
}

pub(crate) struct CpuFallbackEngine<A, C> {
    accelerated: Option<A>,
    cpu: C,
    diagnostics: CpuFallbackDiagnostics,
}

impl<A, C> CpuFallbackEngine<A, C> {
    pub(crate) fn new(
        accelerated: Result<A, TranscriptionError>,
        cpu: C,
    ) -> CpuFallbackEngine<A, C> {
        let (accelerated, acceleration_startup_failures) = match accelerated {
            Ok(engine) => (Some(engine), 0),
            Err(_) => (None, 1),
        };
        Self {
            accelerated,
            cpu,
            diagnostics: CpuFallbackDiagnostics {
                acceleration_startup_failures,
                ..CpuFallbackDiagnostics::default()
            },
        }
    }

    pub(crate) const fn diagnostics(&self) -> CpuFallbackDiagnostics {
        self.diagnostics
    }
}

impl<A: TranscriptionEngine, C: TranscriptionEngine> TranscriptionEngine
    for CpuFallbackEngine<A, C>
{
    fn transcribe(
        &mut self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        let request = request.validate()?;
        if let Some(accelerated) = self.accelerated.as_mut() {
            match accelerated.transcribe(request) {
                Ok(result) => {
                    self.diagnostics.accelerated_results =
                        self.diagnostics.accelerated_results.saturating_add(1);
                    return Ok(result);
                }
                Err(_) => {
                    self.diagnostics.acceleration_inference_failures = self
                        .diagnostics
                        .acceleration_inference_failures
                        .saturating_add(1);
                    self.accelerated = None;
                }
            }
        }

        self.diagnostics.cpu_fallback_attempts =
            self.diagnostics.cpu_fallback_attempts.saturating_add(1);
        let result = self.cpu.transcribe(request)?;
        self.diagnostics.cpu_fallback_results =
            self.diagnostics.cpu_fallback_results.saturating_add(1);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::audio::AudioSource;

    use super::*;
    use crate::transcription::TranscriptSegment;

    struct CountingEngine {
        calls: usize,
        failure: Option<TranscriptionError>,
        text: &'static str,
    }

    impl CountingEngine {
        const fn succeeding(text: &'static str) -> Self {
            Self {
                calls: 0,
                failure: None,
                text,
            }
        }

        const fn failing(error: TranscriptionError) -> Self {
            Self {
                calls: 0,
                failure: Some(error),
                text: "",
            }
        }
    }

    impl TranscriptionEngine for CountingEngine {
        fn transcribe(
            &mut self,
            request: TranscriptionRequest<'_>,
        ) -> Result<TranscriptionResult, TranscriptionError> {
            self.calls += 1;
            if let Some(error) = self.failure {
                return Err(error);
            }
            Ok(TranscriptionResult {
                source: request.source,
                utterance_start_ms: request.start_ms,
                utterance_end_ms: request.end_ms,
                language: "en".to_owned(),
                text: self.text.to_owned(),
                segments: vec![TranscriptSegment {
                    start_ms: request.start_ms,
                    end_ms: request.end_ms,
                    text: self.text.to_owned(),
                }],
            })
        }
    }

    fn request() -> TranscriptionRequest<'static> {
        TranscriptionRequest {
            source: AudioSource::SystemOutput,
            start_ms: 4_000,
            end_ms: 5_000,
            samples: &[0.25; 16_000],
        }
    }

    #[test]
    fn startup_failure_delivers_one_cpu_result() {
        let cpu = CountingEngine::succeeding("cpu result");
        let mut engine = CpuFallbackEngine::<CountingEngine, _>::new(
            Err(TranscriptionError::BackendUnavailable),
            cpu,
        );

        let result = engine.transcribe(request()).expect("CPU fallback result");
        assert_eq!(result.text, "cpu result");
        assert_eq!(engine.cpu.calls, 1);
        assert_eq!(
            engine.diagnostics(),
            CpuFallbackDiagnostics {
                acceleration_startup_failures: 1,
                cpu_fallback_attempts: 1,
                cpu_fallback_results: 1,
                ..CpuFallbackDiagnostics::default()
            }
        );
    }

    #[test]
    fn inference_failure_retries_once_on_cpu_and_disables_acceleration() {
        let accelerated = CountingEngine::failing(TranscriptionError::InferenceFailed);
        let cpu = CountingEngine::succeeding("recovered result");
        let mut engine = CpuFallbackEngine::new(Ok(accelerated), cpu);

        let result = engine.transcribe(request()).expect("recovered CPU result");
        assert_eq!(result.text, "recovered result");
        assert!(engine.accelerated.is_none());
        assert_eq!(engine.cpu.calls, 1);
        assert_eq!(
            engine.diagnostics(),
            CpuFallbackDiagnostics {
                acceleration_inference_failures: 1,
                cpu_fallback_attempts: 1,
                cpu_fallback_results: 1,
                ..CpuFallbackDiagnostics::default()
            }
        );
    }

    #[test]
    fn accelerated_success_never_calls_cpu() {
        let accelerated = CountingEngine::succeeding("accelerated result");
        let cpu = CountingEngine::succeeding("unexpected CPU result");
        let mut engine = CpuFallbackEngine::new(Ok(accelerated), cpu);

        let result = engine.transcribe(request()).expect("accelerated result");
        assert_eq!(result.text, "accelerated result");
        assert_eq!(engine.cpu.calls, 0);
        assert_eq!(
            engine.diagnostics(),
            CpuFallbackDiagnostics {
                accelerated_results: 1,
                ..CpuFallbackDiagnostics::default()
            }
        );
    }
}
