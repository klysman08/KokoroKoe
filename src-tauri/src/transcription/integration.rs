use std::sync::{Arc, atomic::AtomicBool};

use crate::audio::{AudioSource, DetectedUtterance, PartialUtteranceSnapshot, ProcessingOutcome};

use super::{
    MAX_TRANSCRIPT_BYTES, MAX_TRANSCRIPT_SEGMENTS, TranscriptionEngine, TranscriptionError,
    TranscriptionRequest, TranscriptionResult,
    scheduler::{
        FinalSubmission, PartialSubmission, SchedulerDiagnostics, SchedulerSubmissionError,
        TranscriptionGap, TranscriptionScheduler,
    },
};

const MAX_VAD_BATCH_UTTERANCES: usize = 64;
const CANCELLED_GAP_CODE: &str = "transcription_pipeline_cancelled";
const INVALID_RESULT_GAP_CODE: &str = "transcription_native_result_invalid";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LivePipelineState {
    Running,
    Pausing,
    Paused,
    Stopping,
    Stopped,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LivePipelineError {
    InvalidState,
    SourceSealed,
    WatermarkRegression,
    InvalidVadOutput,
    JobIdExhausted,
}

impl LivePipelineError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidState => "transcription_pipeline_state_invalid",
            Self::SourceSealed => "transcription_pipeline_source_sealed",
            Self::WatermarkRegression => "transcription_pipeline_watermark_regressed",
            Self::InvalidVadOutput => "transcription_pipeline_vad_output_invalid",
            Self::JobIdExhausted => "transcription_pipeline_job_id_exhausted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LivePipelineEvent {
    Partial {
        job_id: u64,
        result: TranscriptionResult,
    },
    Final {
        job_id: u64,
        result: TranscriptionResult,
    },
    Gap(TranscriptionGap),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LivePipelineDiagnostics {
    pub(crate) finals_received: u64,
    pub(crate) finals_enqueued: u64,
    pub(crate) final_results: u64,
    pub(crate) backlog_gaps: u64,
    pub(crate) inference_gaps: u64,
    pub(crate) cancellation_gaps: u64,
    pub(crate) watermark_blocks: u64,
    pub(crate) discarded_partials: u64,
    pub(crate) partials_received: u64,
    pub(crate) partials_enqueued: u64,
    pub(crate) partials_replaced: u64,
    pub(crate) partials_suppressed: u64,
    pub(crate) partial_results: u64,
    pub(crate) partial_failures: u64,
}

#[derive(Debug, Default)]
struct PartialIdentities {
    microphone: Option<(u64, u64)>,
    system_output: Option<(u64, u64)>,
}

impl PartialIdentities {
    fn get(&self, source: AudioSource) -> Option<(u64, u64)> {
        match source {
            AudioSource::Microphone => self.microphone,
            AudioSource::SystemOutput => self.system_output,
        }
    }

    fn set(&mut self, source: AudioSource, value: Option<(u64, u64)>) {
        match source {
            AudioSource::Microphone => self.microphone = value,
            AudioSource::SystemOutput => self.system_output = value,
        }
    }

    fn clear(&mut self) {
        self.microphone = None;
        self.system_output = None;
    }
}

#[derive(Debug, Default)]
struct SourceFrontier {
    current_ms: Option<u64>,
    last_ms: Option<u64>,
    sealed: bool,
}

impl SourceFrontier {
    fn validate(&self, watermark_ms: Option<u64>) -> Result<(), LivePipelineError> {
        if self.sealed {
            return Err(LivePipelineError::SourceSealed);
        }
        if let (Some(previous), Some(next)) = (self.last_ms, watermark_ms)
            && next < previous
        {
            return Err(LivePipelineError::WatermarkRegression);
        }
        Ok(())
    }

    fn update(&mut self, watermark_ms: Option<u64>) {
        if let Some(watermark_ms) = watermark_ms {
            self.current_ms = Some(watermark_ms);
            self.last_ms = Some(watermark_ms);
        }
    }

    fn seal(&mut self) {
        self.sealed = true;
        self.current_ms = None;
    }

    fn resume(&mut self) {
        self.current_ms = None;
        self.sealed = false;
    }

    const fn readiness_ms(&self) -> Option<u64> {
        if self.sealed {
            Some(u64::MAX)
        } else {
            self.current_ms
        }
    }
}

#[derive(Debug, Default)]
struct SourceFrontiers {
    microphone: SourceFrontier,
    system_output: SourceFrontier,
}

impl SourceFrontiers {
    fn get(&self, source: AudioSource) -> &SourceFrontier {
        match source {
            AudioSource::Microphone => &self.microphone,
            AudioSource::SystemOutput => &self.system_output,
        }
    }

    fn get_mut(&mut self, source: AudioSource) -> &mut SourceFrontier {
        match source {
            AudioSource::Microphone => &mut self.microphone,
            AudioSource::SystemOutput => &mut self.system_output,
        }
    }

    fn readiness_ms(&self) -> Option<u64> {
        Some(
            self.microphone
                .readiness_ms()?
                .min(self.system_output.readiness_ms()?),
        )
    }

    const fn both_sealed(&self) -> bool {
        self.microphone.sealed && self.system_output.sealed
    }

    fn resume(&mut self) {
        self.microphone.resume();
        self.system_output.resume();
    }
}

pub(crate) struct LiveFinalTranscriptionPipeline<E> {
    scheduler: TranscriptionScheduler,
    engine: E,
    frontiers: SourceFrontiers,
    state: LivePipelineState,
    next_job_id: u64,
    partial_identities: PartialIdentities,
    last_started_key: Option<(u64, u64, u64)>,
    diagnostics: LivePipelineDiagnostics,
}

impl<E: TranscriptionEngine> LiveFinalTranscriptionPipeline<E> {
    pub(crate) fn new(engine: E) -> Self {
        Self {
            scheduler: TranscriptionScheduler::default(),
            engine,
            frontiers: SourceFrontiers::default(),
            state: LivePipelineState::Running,
            next_job_id: 1,
            partial_identities: PartialIdentities::default(),
            last_started_key: None,
            diagnostics: LivePipelineDiagnostics::default(),
        }
    }

    pub(crate) const fn state(&self) -> LivePipelineState {
        self.state
    }

    pub(crate) const fn diagnostics(&self) -> LivePipelineDiagnostics {
        self.diagnostics
    }

    pub(crate) const fn scheduler_diagnostics(&self) -> SchedulerDiagnostics {
        self.scheduler.diagnostics()
    }

    pub(crate) fn queued_final_jobs(&self) -> usize {
        self.scheduler.queued_final_jobs()
    }

    #[cfg(test)]
    fn engine(&self) -> &E {
        &self.engine
    }

    pub(crate) fn submit_finalized_vad(
        &mut self,
        source: AudioSource,
        utterances: Vec<DetectedUtterance>,
        ordering_watermark_ms: Option<u64>,
    ) -> Result<Vec<LivePipelineEvent>, LivePipelineError> {
        if !matches!(
            self.state,
            LivePipelineState::Running | LivePipelineState::Pausing | LivePipelineState::Stopping
        ) {
            return Err(LivePipelineError::InvalidState);
        }
        if utterances.len() > MAX_VAD_BATCH_UTTERANCES {
            return Err(LivePipelineError::InvalidVadOutput);
        }
        self.frontiers.get(source).validate(ordering_watermark_ms)?;
        for utterance in &utterances {
            if utterance.source != source
                || TranscriptionRequest::from(utterance).validate().is_err()
            {
                return Err(LivePipelineError::InvalidVadOutput);
            }
        }
        let required_ids =
            u64::try_from(utterances.len()).map_err(|_| LivePipelineError::JobIdExhausted)?;
        self.next_job_id
            .checked_add(required_ids)
            .ok_or(LivePipelineError::JobIdExhausted)?;

        let mut events = Vec::new();
        for utterance in utterances {
            let job_id = match self.partial_identities.get(source) {
                Some((start_ms, job_id)) if start_ms == utterance.start_ms => {
                    self.partial_identities.set(source, None);
                    job_id
                }
                _ => self.take_job_id()?,
            };
            self.diagnostics.finals_received = self.diagnostics.finals_received.saturating_add(1);
            let submission = self
                .scheduler
                .submit_final(
                    job_id,
                    utterance.source,
                    utterance.start_ms,
                    utterance.end_ms,
                    Arc::from(utterance.samples),
                )
                .map_err(map_scheduler_error)?;
            match submission {
                FinalSubmission::Enqueued => {
                    self.diagnostics.finals_enqueued =
                        self.diagnostics.finals_enqueued.saturating_add(1);
                }
                FinalSubmission::Gap(gap) => {
                    self.diagnostics.backlog_gaps = self.diagnostics.backlog_gaps.saturating_add(1);
                    events.push(LivePipelineEvent::Gap(gap));
                }
            }
        }
        self.frontiers.get_mut(source).update(ordering_watermark_ms);
        Ok(events)
    }

    pub(crate) fn submit_partial(
        &mut self,
        snapshot: PartialUtteranceSnapshot,
    ) -> Result<(), LivePipelineError> {
        if self.state != LivePipelineState::Running {
            return Err(LivePipelineError::InvalidState);
        }
        if self.frontiers.get(snapshot.source).sealed {
            return Err(LivePipelineError::SourceSealed);
        }
        if (TranscriptionRequest {
            source: snapshot.source,
            start_ms: snapshot.start_ms,
            end_ms: snapshot.end_ms,
            samples: &snapshot.samples,
        })
        .validate()
        .is_err()
        {
            return Err(LivePipelineError::InvalidVadOutput);
        }
        self.diagnostics.partials_received = self.diagnostics.partials_received.saturating_add(1);
        let job_id = match self.partial_identities.get(snapshot.source) {
            Some((start_ms, job_id)) if start_ms == snapshot.start_ms => job_id,
            _ => {
                let job_id = self.take_job_id()?;
                self.partial_identities
                    .set(snapshot.source, Some((snapshot.start_ms, job_id)));
                job_id
            }
        };
        match self
            .scheduler
            .submit_partial(
                job_id,
                snapshot.source,
                snapshot.start_ms,
                snapshot.end_ms,
                Arc::from(snapshot.samples),
            )
            .map_err(map_scheduler_error)?
        {
            PartialSubmission::Enqueued => {
                self.diagnostics.partials_enqueued =
                    self.diagnostics.partials_enqueued.saturating_add(1);
            }
            PartialSubmission::Replaced => {
                self.diagnostics.partials_replaced =
                    self.diagnostics.partials_replaced.saturating_add(1);
            }
            PartialSubmission::Suppressed => {
                self.diagnostics.partials_suppressed =
                    self.diagnostics.partials_suppressed.saturating_add(1);
            }
        }
        Ok(())
    }

    pub(crate) fn submit_processing_outcome(
        &mut self,
        source: AudioSource,
        outcome: ProcessingOutcome,
    ) -> Result<Vec<LivePipelineEvent>, LivePipelineError> {
        if let Some(partial) = outcome.partial {
            self.submit_partial(partial)?;
        }
        self.submit_finalized_vad(
            source,
            outcome.utterances,
            outcome.vad_ordering_watermark_ms,
        )
    }

    fn take_job_id(&mut self) -> Result<u64, LivePipelineError> {
        let job_id = self.next_job_id;
        self.next_job_id = self
            .next_job_id
            .checked_add(1)
            .ok_or(LivePipelineError::JobIdExhausted)?;
        Ok(job_id)
    }

    pub(crate) fn pump_one(&mut self, cancelled: &AtomicBool) -> Option<LivePipelineEvent> {
        let final_ready = self
            .scheduler
            .next_final_start_ms()
            .is_some_and(|next_start_ms| {
                self.frontiers.readiness_ms().is_some_and(|readiness_ms| {
                    self.frontiers.both_sealed() || next_start_ms < readiness_ms
                })
            });
        if final_ready {
            let job = self
                .scheduler
                .next_final_job()
                .expect("ready final must remain available");
            return Some(self.transcribe_final(job, cancelled));
        }
        if self.scheduler.next_final_start_ms().is_some() {
            self.diagnostics.watermark_blocks = self.diagnostics.watermark_blocks.saturating_add(1);
        }
        if self.state == LivePipelineState::Running
            && let Some(job) = self.scheduler.next_partial_job()
        {
            let result = self.engine.transcribe_with_cancel(job.request(), cancelled);
            return match result {
                Ok(result) if result_matches_job(&result, &job) => {
                    self.diagnostics.partial_results =
                        self.diagnostics.partial_results.saturating_add(1);
                    Some(LivePipelineEvent::Partial {
                        job_id: job.job_id(),
                        result,
                    })
                }
                Ok(_) | Err(_) => {
                    self.diagnostics.partial_failures =
                        self.diagnostics.partial_failures.saturating_add(1);
                    None
                }
            };
        }
        self.settle_boundary_if_empty();
        None
    }

    fn transcribe_final(
        &mut self,
        job: super::scheduler::ScheduledTranscriptionJob,
        cancelled: &AtomicBool,
    ) -> LivePipelineEvent {
        let key = (job.start_ms(), job.end_ms(), job.job_id());
        debug_assert!(self.last_started_key.is_none_or(|previous| previous <= key));
        self.last_started_key = Some(key);
        let result = self.engine.transcribe_with_cancel(job.request(), cancelled);
        let event = match result {
            Ok(result) if result_matches_job(&result, &job) => {
                self.diagnostics.final_results = self.diagnostics.final_results.saturating_add(1);
                LivePipelineEvent::Final {
                    job_id: job.job_id(),
                    result,
                }
            }
            Ok(_) => {
                self.diagnostics.inference_gaps = self.diagnostics.inference_gaps.saturating_add(1);
                LivePipelineEvent::Gap(job_gap(&job, INVALID_RESULT_GAP_CODE))
            }
            Err(error) => {
                if error == TranscriptionError::WorkerCancelled {
                    self.diagnostics.cancellation_gaps =
                        self.diagnostics.cancellation_gaps.saturating_add(1);
                } else {
                    self.diagnostics.inference_gaps =
                        self.diagnostics.inference_gaps.saturating_add(1);
                }
                LivePipelineEvent::Gap(job_gap(&job, error.code()))
            }
        };
        self.settle_boundary_if_empty();
        event
    }

    pub(crate) fn begin_pause(&mut self) -> Result<(), LivePipelineError> {
        if self.state != LivePipelineState::Running {
            return Err(LivePipelineError::InvalidState);
        }
        self.state = LivePipelineState::Pausing;
        self.discard_partials();
        Ok(())
    }

    pub(crate) fn resume(&mut self) -> Result<(), LivePipelineError> {
        if self.state != LivePipelineState::Paused || self.scheduler.queued_final_jobs() != 0 {
            return Err(LivePipelineError::InvalidState);
        }
        self.frontiers.resume();
        self.state = LivePipelineState::Running;
        Ok(())
    }

    pub(crate) fn begin_stop(&mut self) -> Result<(), LivePipelineError> {
        if !matches!(
            self.state,
            LivePipelineState::Running | LivePipelineState::Paused
        ) {
            return Err(LivePipelineError::InvalidState);
        }
        if self.state == LivePipelineState::Paused {
            self.state = LivePipelineState::Stopped;
            return Ok(());
        }
        self.state = LivePipelineState::Stopping;
        self.discard_partials();
        Ok(())
    }

    pub(crate) fn seal_source(&mut self, source: AudioSource) -> Result<(), LivePipelineError> {
        if !matches!(
            self.state,
            LivePipelineState::Pausing | LivePipelineState::Stopping
        ) {
            return Err(LivePipelineError::InvalidState);
        }
        let frontier = self.frontiers.get_mut(source);
        if frontier.sealed {
            return Err(LivePipelineError::SourceSealed);
        }
        frontier.seal();
        self.settle_boundary_if_empty();
        Ok(())
    }

    pub(crate) fn cancel_pending(&mut self) -> Vec<LivePipelineEvent> {
        self.state = LivePipelineState::Cancelled;
        self.frontiers.microphone.seal();
        self.frontiers.system_output.seal();
        self.discard_partials();
        let mut events = Vec::new();
        while let Some(job) = self.scheduler.next_final_job() {
            self.diagnostics.cancellation_gaps =
                self.diagnostics.cancellation_gaps.saturating_add(1);
            events.push(LivePipelineEvent::Gap(job_gap(&job, CANCELLED_GAP_CODE)));
        }
        events
    }

    fn discard_partials(&mut self) {
        self.partial_identities.clear();
        self.diagnostics.discarded_partials = self
            .diagnostics
            .discarded_partials
            .saturating_add(self.scheduler.discard_partials() as u64);
    }

    fn settle_boundary_if_empty(&mut self) {
        if self.scheduler.queued_final_jobs() != 0 || !self.frontiers.both_sealed() {
            return;
        }
        self.state = match self.state {
            LivePipelineState::Pausing => LivePipelineState::Paused,
            LivePipelineState::Stopping => LivePipelineState::Stopped,
            other => other,
        };
    }
}

fn map_scheduler_error(error: SchedulerSubmissionError) -> LivePipelineError {
    debug_assert!(matches!(error, SchedulerSubmissionError::InvalidRequest(_)));
    LivePipelineError::InvalidVadOutput
}

fn job_gap(
    job: &super::scheduler::ScheduledTranscriptionJob,
    code: &'static str,
) -> TranscriptionGap {
    TranscriptionGap {
        job_id: job.job_id(),
        source: job.source(),
        start_ms: job.start_ms(),
        end_ms: job.end_ms(),
        code,
    }
}

fn result_matches_job(
    result: &TranscriptionResult,
    job: &super::scheduler::ScheduledTranscriptionJob,
) -> bool {
    result.source == job.source()
        && result.utterance_start_ms == job.start_ms()
        && result.utterance_end_ms == job.end_ms()
        && !result.language.is_empty()
        && result.language.len() <= 64
        && result.text.len() <= MAX_TRANSCRIPT_BYTES
        && result.segments.len() <= MAX_TRANSCRIPT_SEGMENTS
        && result.segments.iter().all(|segment| {
            segment.start_ms >= job.start_ms()
                && segment.end_ms > segment.start_ms
                && segment.end_ms <= job.end_ms()
                && segment.text.len() <= MAX_TRANSCRIPT_BYTES
        })
        && result
            .segments
            .windows(2)
            .all(|pair| pair[0].end_ms <= pair[1].start_ms)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Mutex, atomic::AtomicBool},
    };

    use crate::audio::{PartialUtteranceSnapshot, UtteranceEndReason};

    use super::*;
    #[cfg(windows)]
    use crate::transcription::worker::{
        AcceleratedWorker, SupervisedFallbackEngine, WorkerFailure, WorkerFailureKind,
    };

    fn utterance(source: AudioSource, start_ms: u64, duration_ms: u64) -> DetectedUtterance {
        DetectedUtterance {
            source,
            start_ms,
            end_ms: start_ms + duration_ms,
            samples: vec![0.1; duration_ms as usize * 16],
            reason: UtteranceEndReason::TrailingSilence,
        }
    }

    fn result(request: TranscriptionRequest<'_>, text: &str) -> TranscriptionResult {
        TranscriptionResult {
            source: request.source,
            utterance_start_ms: request.start_ms,
            utterance_end_ms: request.end_ms,
            language: "en".to_owned(),
            text: text.to_owned(),
            segments: vec![super::super::TranscriptSegment {
                start_ms: request.start_ms,
                end_ms: request.end_ms,
                text: text.to_owned(),
            }],
        }
    }

    #[derive(Default)]
    struct RecordingEngine {
        calls: Vec<(AudioSource, u64, u64)>,
        fail_microphone_once: bool,
    }

    impl TranscriptionEngine for RecordingEngine {
        fn transcribe(
            &mut self,
            request: TranscriptionRequest<'_>,
        ) -> Result<TranscriptionResult, TranscriptionError> {
            self.calls
                .push((request.source, request.start_ms, request.end_ms));
            if request.source == AudioSource::Microphone && self.fail_microphone_once {
                self.fail_microphone_once = false;
                return Err(TranscriptionError::InferenceFailed);
            }
            Ok(result(request, "bounded result"))
        }
    }

    fn submit(
        pipeline: &mut LiveFinalTranscriptionPipeline<impl TranscriptionEngine>,
        source: AudioSource,
        utterances: Vec<DetectedUtterance>,
        watermark_ms: Option<u64>,
    ) -> Vec<LivePipelineEvent> {
        pipeline
            .submit_finalized_vad(source, utterances, watermark_ms)
            .expect("valid VAD batch")
    }

    fn partial(source: AudioSource, start_ms: u64, duration_ms: u64) -> PartialUtteranceSnapshot {
        PartialUtteranceSnapshot {
            source,
            start_ms,
            end_ms: start_ms + duration_ms,
            samples: vec![0.1; duration_ms as usize * 16],
        }
    }

    #[test]
    fn replaceable_partials_keep_identity_and_eligible_finals_run_first() {
        let mut pipeline = LiveFinalTranscriptionPipeline::new(RecordingEngine::default());
        pipeline
            .submit_partial(partial(AudioSource::SystemOutput, 1_000, 500))
            .unwrap();
        pipeline
            .submit_partial(partial(AudioSource::SystemOutput, 1_000, 1_000))
            .unwrap();
        let partial_id = match pipeline.pump_one(&AtomicBool::new(false)).unwrap() {
            LivePipelineEvent::Partial { job_id, result } => {
                assert_eq!(result.utterance_end_ms, 2_000);
                job_id
            }
            other => panic!("expected partial, got {other:?}"),
        };
        assert_eq!(partial_id, 1);

        submit(
            &mut pipeline,
            AudioSource::SystemOutput,
            vec![utterance(AudioSource::SystemOutput, 1_000, 1_500)],
            Some(5_000),
        );
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            Vec::new(),
            Some(5_000),
        );
        assert!(matches!(
            pipeline.pump_one(&AtomicBool::new(false)),
            Some(LivePipelineEvent::Final { job_id: 1, .. })
        ));

        pipeline
            .submit_partial(partial(AudioSource::SystemOutput, 4_000, 500))
            .unwrap();
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            vec![utterance(AudioSource::Microphone, 3_000, 500)],
            Some(5_000),
        );
        assert!(matches!(
            pipeline.pump_one(&AtomicBool::new(false)),
            Some(LivePipelineEvent::Final { job_id: 3, .. })
        ));
        assert!(matches!(
            pipeline.pump_one(&AtomicBool::new(false)),
            Some(LivePipelineEvent::Partial { job_id: 2, .. })
        ));
        let diagnostics = pipeline.diagnostics();
        assert_eq!(diagnostics.partials_enqueued, 2);
        assert_eq!(diagnostics.partials_replaced, 1);
        assert_eq!(diagnostics.partial_results, 2);
    }

    #[test]
    fn partial_failure_is_diagnostic_only_and_stop_discards_provisional_state() {
        let mut pipeline = LiveFinalTranscriptionPipeline::new(RecordingEngine {
            fail_microphone_once: true,
            ..RecordingEngine::default()
        });
        pipeline
            .submit_partial(partial(AudioSource::Microphone, 0, 500))
            .unwrap();
        assert!(pipeline.pump_one(&AtomicBool::new(false)).is_none());
        assert_eq!(pipeline.diagnostics().partial_failures, 1);
        pipeline
            .submit_partial(partial(AudioSource::Microphone, 0, 1_000))
            .unwrap();
        pipeline.begin_stop().unwrap();
        assert_eq!(pipeline.diagnostics().discarded_partials, 1);
        assert_eq!(pipeline.scheduler.queued_partial_jobs(), 0);
        assert!(pipeline.cancel_pending().is_empty());
        assert_eq!(pipeline.diagnostics().inference_gaps, 0);
        assert_eq!(pipeline.diagnostics().cancellation_gaps, 0);
    }

    #[test]
    fn delayed_source_watermark_prevents_a_late_older_final() {
        let mut pipeline = LiveFinalTranscriptionPipeline::new(RecordingEngine::default());
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            vec![utterance(AudioSource::Microphone, 1_000, 1_000)],
            Some(3_000),
        );
        submit(
            &mut pipeline,
            AudioSource::SystemOutput,
            Vec::new(),
            Some(200),
        );
        assert!(pipeline.pump_one(&AtomicBool::new(false)).is_none());

        submit(
            &mut pipeline,
            AudioSource::SystemOutput,
            vec![utterance(AudioSource::SystemOutput, 200, 500)],
            Some(2_000),
        );
        let mut starts = Vec::new();
        while let Some(event) = pipeline.pump_one(&AtomicBool::new(false)) {
            let LivePipelineEvent::Final { result, .. } = event else {
                panic!("both valid finals must transcribe");
            };
            starts.push(result.utterance_start_ms);
        }
        assert_eq!(starts, [200, 1_000]);
        assert_eq!(pipeline.diagnostics().watermark_blocks, 1);
    }

    #[test]
    fn pause_flushes_losslessly_resume_resets_frontiers_and_stop_drains() {
        let mut pipeline = LiveFinalTranscriptionPipeline::new(RecordingEngine::default());
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            Vec::new(),
            Some(1_000),
        );
        submit(
            &mut pipeline,
            AudioSource::SystemOutput,
            Vec::new(),
            Some(1_000),
        );
        pipeline.begin_pause().unwrap();
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            vec![utterance(AudioSource::Microphone, 500, 500)],
            None,
        );
        submit(&mut pipeline, AudioSource::SystemOutput, Vec::new(), None);
        pipeline.seal_source(AudioSource::Microphone).unwrap();
        pipeline.seal_source(AudioSource::SystemOutput).unwrap();
        assert!(matches!(
            pipeline.pump_one(&AtomicBool::new(false)),
            Some(LivePipelineEvent::Final { .. })
        ));
        assert_eq!(pipeline.state(), LivePipelineState::Paused);

        pipeline.resume().unwrap();
        assert!(pipeline.pump_one(&AtomicBool::new(false)).is_none());
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            vec![utterance(AudioSource::Microphone, 2_000, 500)],
            Some(3_000),
        );
        submit(
            &mut pipeline,
            AudioSource::SystemOutput,
            Vec::new(),
            Some(3_000),
        );
        pipeline.begin_stop().unwrap();
        pipeline.seal_source(AudioSource::Microphone).unwrap();
        pipeline.seal_source(AudioSource::SystemOutput).unwrap();
        assert!(matches!(
            pipeline.pump_one(&AtomicBool::new(false)),
            Some(LivePipelineEvent::Final { .. })
        ));
        assert_eq!(pipeline.state(), LivePipelineState::Stopped);
        assert_eq!(pipeline.diagnostics().final_results, 2);
    }

    #[test]
    fn stop_from_an_already_drained_pause_is_immediate() {
        let mut pipeline = LiveFinalTranscriptionPipeline::new(RecordingEngine::default());
        pipeline.begin_pause().unwrap();
        submit(&mut pipeline, AudioSource::Microphone, Vec::new(), None);
        submit(&mut pipeline, AudioSource::SystemOutput, Vec::new(), None);
        pipeline.seal_source(AudioSource::Microphone).unwrap();
        pipeline.seal_source(AudioSource::SystemOutput).unwrap();
        assert_eq!(pipeline.state(), LivePipelineState::Paused);
        pipeline.begin_stop().unwrap();
        assert_eq!(pipeline.state(), LivePipelineState::Stopped);
    }

    #[test]
    fn inference_failure_is_an_explicit_gap_and_does_not_poison_the_other_source() {
        let mut pipeline = LiveFinalTranscriptionPipeline::new(RecordingEngine {
            calls: Vec::new(),
            fail_microphone_once: true,
        });
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            vec![utterance(AudioSource::Microphone, 0, 500)],
            Some(2_000),
        );
        submit(
            &mut pipeline,
            AudioSource::SystemOutput,
            vec![utterance(AudioSource::SystemOutput, 1_000, 500)],
            Some(2_000),
        );
        let first = pipeline.pump_one(&AtomicBool::new(false)).unwrap();
        let second = pipeline.pump_one(&AtomicBool::new(false)).unwrap();
        assert!(matches!(
            first,
            LivePipelineEvent::Gap(TranscriptionGap {
                source: AudioSource::Microphone,
                code: "transcription_inference_failed",
                ..
            })
        ));
        assert!(matches!(
            second,
            LivePipelineEvent::Final {
                result: TranscriptionResult {
                    source: AudioSource::SystemOutput,
                    ..
                },
                ..
            }
        ));
        assert_eq!(pipeline.diagnostics().inference_gaps, 1);
    }

    #[test]
    fn two_hour_delayed_source_pressure_is_bounded_chronological_and_exactly_accounted() {
        let mut pipeline = LiveFinalTranscriptionPipeline::new(RecordingEngine::default());
        let never_cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        for second in 0_u64..7_200 {
            let source = if second % 2 == 0 {
                AudioSource::Microphone
            } else {
                AudioSource::SystemOutput
            };
            let microphone_frontier = second.saturating_add(1) * 1_000;
            let system_frontier = second.saturating_sub(1) * 1_000;
            events.extend(submit(
                &mut pipeline,
                source,
                vec![utterance(source, second * 1_000, 1_000)],
                Some(match source {
                    AudioSource::Microphone => microphone_frontier,
                    AudioSource::SystemOutput => system_frontier,
                }),
            ));
            let other = match source {
                AudioSource::Microphone => AudioSource::SystemOutput,
                AudioSource::SystemOutput => AudioSource::Microphone,
            };
            events.extend(submit(
                &mut pipeline,
                other,
                Vec::new(),
                Some(match other {
                    AudioSource::Microphone => microphone_frontier,
                    AudioSource::SystemOutput => system_frontier,
                }),
            ));
            if second % 5 == 4
                && let Some(event) = pipeline.pump_one(&never_cancel)
            {
                events.push(event);
            }
        }
        pipeline.begin_stop().unwrap();
        pipeline.seal_source(AudioSource::Microphone).unwrap();
        pipeline.seal_source(AudioSource::SystemOutput).unwrap();
        while let Some(event) = pipeline.pump_one(&never_cancel) {
            events.push(event);
        }

        let final_keys: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                LivePipelineEvent::Final { job_id, result } => Some((
                    result.utterance_start_ms,
                    result.utterance_end_ms,
                    *job_id,
                    result.source,
                )),
                LivePipelineEvent::Partial { .. } | LivePipelineEvent::Gap(_) => None,
            })
            .collect();
        assert!(final_keys.windows(2).all(|pair| {
            (pair[0].0, pair[0].1, pair[0].2) <= (pair[1].0, pair[1].1, pair[1].2)
        }));
        let unique: HashSet<_> = events
            .iter()
            .map(|event| match event {
                LivePipelineEvent::Final { job_id, .. } => *job_id,
                LivePipelineEvent::Partial { job_id, .. } => *job_id,
                LivePipelineEvent::Gap(gap) => gap.job_id,
            })
            .collect();
        let diagnostics = pipeline.diagnostics();
        let scheduler = pipeline.scheduler_diagnostics();
        let microphone_results = final_keys
            .iter()
            .filter(|key| key.3 == AudioSource::Microphone)
            .count();
        let system_results = final_keys
            .iter()
            .filter(|key| key.3 == AudioSource::SystemOutput)
            .count();
        assert_eq!(unique.len() as u64, diagnostics.finals_received);
        assert_eq!(events.len() as u64, diagnostics.finals_received);
        assert!(diagnostics.backlog_gaps > 0);
        assert!(microphone_results.abs_diff(system_results) <= 1);
        assert!(scheduler.peak_queued_final_jobs <= 600);
        assert!(scheduler.peak_queued_final_speech_ms <= 600_000);
        assert!(scheduler.peak_logical_queued_samples <= 9_600_000);
        assert_eq!(pipeline.queued_final_jobs(), 0);
        assert_eq!(pipeline.state(), LivePipelineState::Stopped);
        println!(
            "duration_s=7200 received={} results={} gaps={} microphone_results={} system_results={} peak_jobs={} peak_speech_ms={} peak_samples={} watermark_blocks={}",
            diagnostics.finals_received,
            diagnostics.final_results,
            diagnostics.backlog_gaps,
            microphone_results,
            system_results,
            scheduler.peak_queued_final_jobs,
            scheduler.peak_queued_final_speech_ms,
            scheduler.peak_logical_queued_samples,
            diagnostics.watermark_blocks,
        );
    }

    #[cfg(windows)]
    struct FailingWorker {
        failure: WorkerFailureKind,
        shutdowns: Arc<Mutex<u64>>,
    }

    #[cfg(windows)]
    impl AcceleratedWorker for FailingWorker {
        fn transcribe_accelerated(
            &mut self,
            _request: TranscriptionRequest<'_>,
            _cancelled: &AtomicBool,
        ) -> Result<TranscriptionResult, WorkerFailure> {
            Err(WorkerFailure::new(self.failure))
        }

        fn shutdown(&mut self) {
            *self.shutdowns.lock().unwrap() += 1;
        }
    }

    #[cfg(windows)]
    #[derive(Default)]
    struct CpuEngine {
        calls: u64,
    }

    #[cfg(windows)]
    impl TranscriptionEngine for CpuEngine {
        fn transcribe(
            &mut self,
            request: TranscriptionRequest<'_>,
        ) -> Result<TranscriptionResult, TranscriptionError> {
            self.calls += 1;
            Ok(result(request, "cpu result"))
        }
    }

    #[cfg(windows)]
    #[test]
    fn supervised_worker_failure_recovers_once_then_keeps_cpu_owner() {
        let shutdowns = Arc::new(Mutex::new(0));
        let engine = SupervisedFallbackEngine::new(
            Ok(FailingWorker {
                failure: WorkerFailureKind::Terminated,
                shutdowns: Arc::clone(&shutdowns),
            }),
            || Ok(CpuEngine::default()),
        );
        let mut pipeline = LiveFinalTranscriptionPipeline::new(engine);
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            vec![utterance(AudioSource::Microphone, 0, 500)],
            Some(2_000),
        );
        submit(
            &mut pipeline,
            AudioSource::SystemOutput,
            vec![utterance(AudioSource::SystemOutput, 1_000, 500)],
            Some(2_000),
        );
        assert!(matches!(
            pipeline.pump_one(&AtomicBool::new(false)),
            Some(LivePipelineEvent::Final { .. })
        ));
        assert!(matches!(
            pipeline.pump_one(&AtomicBool::new(false)),
            Some(LivePipelineEvent::Final { .. })
        ));
        let diagnostics = pipeline.engine().diagnostics();
        assert_eq!(*shutdowns.lock().unwrap(), 1);
        assert_eq!(diagnostics.terminations, 1);
        assert_eq!(diagnostics.cpu_load_attempts, 1);
        assert_eq!(diagnostics.cpu_fallback_attempts, 2);
        assert_eq!(diagnostics.cpu_fallback_results, 2);
    }

    #[cfg(windows)]
    #[test]
    fn worker_cancellation_never_loads_cpu_and_pending_work_becomes_gaps() {
        let shutdowns = Arc::new(Mutex::new(0));
        let engine = SupervisedFallbackEngine::new(
            Ok(FailingWorker {
                failure: WorkerFailureKind::Cancelled,
                shutdowns: Arc::clone(&shutdowns),
            }),
            || Ok(CpuEngine::default()),
        );
        let mut pipeline = LiveFinalTranscriptionPipeline::new(engine);
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            vec![utterance(AudioSource::Microphone, 0, 500)],
            Some(2_000),
        );
        submit(
            &mut pipeline,
            AudioSource::SystemOutput,
            vec![utterance(AudioSource::SystemOutput, 1_000, 500)],
            Some(2_000),
        );
        assert!(matches!(
            pipeline.pump_one(&AtomicBool::new(false)),
            Some(LivePipelineEvent::Gap(TranscriptionGap {
                code: "transcription_worker_cancelled",
                ..
            }))
        ));
        let pending = pipeline.cancel_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pipeline.state(), LivePipelineState::Cancelled);
        let diagnostics = pipeline.engine().diagnostics();
        assert_eq!(*shutdowns.lock().unwrap(), 1);
        assert_eq!(diagnostics.cancellations, 1);
        assert_eq!(diagnostics.cpu_load_attempts, 0);
        assert_eq!(pipeline.diagnostics().cancellation_gaps, 2);
    }

    #[test]
    fn invalid_batches_and_watermark_regressions_are_fixed_and_non_mutating() {
        let mut pipeline = LiveFinalTranscriptionPipeline::new(RecordingEngine::default());
        submit(
            &mut pipeline,
            AudioSource::Microphone,
            Vec::new(),
            Some(1_000),
        );
        assert_eq!(
            pipeline.submit_finalized_vad(AudioSource::Microphone, Vec::new(), Some(999)),
            Err(LivePipelineError::WatermarkRegression)
        );
        assert_eq!(
            pipeline
                .submit_finalized_vad(
                    AudioSource::Microphone,
                    vec![utterance(AudioSource::SystemOutput, 0, 500)],
                    Some(1_000),
                )
                .unwrap_err()
                .code(),
            "transcription_pipeline_vad_output_invalid"
        );
        assert_eq!(pipeline.diagnostics().finals_received, 0);
    }
}
