use std::{collections::VecDeque, sync::Arc};

use crate::audio::AudioSource;

use super::{TranscriptionError, TranscriptionRequest, model::TRANSCRIPTION_SAMPLE_RATE};

pub(crate) const PARTIAL_DISABLE_SPEECH_MS: u64 = 20_000;
pub(crate) const PARTIAL_RESTORE_SPEECH_MS: u64 = 10_000;
pub(crate) const MAX_QUEUED_FINAL_SPEECH_MS: u64 = 600_000;
pub(crate) const MAX_QUEUED_FINAL_JOBS: usize = 4_096;
const MAX_QUEUED_FINAL_SAMPLES: usize =
    MAX_QUEUED_FINAL_SPEECH_MS as usize * TRANSCRIPTION_SAMPLE_RATE / 1_000;
const FINAL_BACKLOG_GAP_CODE: &str = "transcription_final_backlog_exceeded";

#[derive(Debug, Clone)]
struct OwnedTranscriptionRequest {
    job_id: u64,
    source: AudioSource,
    start_ms: u64,
    end_ms: u64,
    samples: Arc<[f32]>,
}

impl OwnedTranscriptionRequest {
    fn new(
        job_id: u64,
        source: AudioSource,
        start_ms: u64,
        end_ms: u64,
        samples: Arc<[f32]>,
    ) -> Result<Self, TranscriptionError> {
        TranscriptionRequest {
            source,
            start_ms,
            end_ms,
            samples: &samples,
        }
        .validate()?;
        Ok(Self {
            job_id,
            source,
            start_ms,
            end_ms,
            samples,
        })
    }

    const fn duration_ms(&self) -> u64 {
        self.end_ms - self.start_ms
    }

    fn as_request(&self) -> TranscriptionRequest<'_> {
        TranscriptionRequest {
            source: self.source,
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            samples: &self.samples,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScheduledJobKind {
    Final,
    Partial,
}

#[derive(Debug, Clone)]
pub(crate) struct ScheduledTranscriptionJob {
    request: OwnedTranscriptionRequest,
    sequence: u64,
    kind: ScheduledJobKind,
}

impl ScheduledTranscriptionJob {
    pub(crate) const fn job_id(&self) -> u64 {
        self.request.job_id
    }

    pub(crate) const fn source(&self) -> AudioSource {
        self.request.source
    }

    pub(crate) const fn start_ms(&self) -> u64 {
        self.request.start_ms
    }

    pub(crate) const fn end_ms(&self) -> u64 {
        self.request.end_ms
    }

    pub(crate) const fn duration_ms(&self) -> u64 {
        self.request.duration_ms()
    }

    pub(crate) const fn kind(&self) -> ScheduledJobKind {
        self.kind
    }

    pub(crate) fn request(&self) -> TranscriptionRequest<'_> {
        self.request.as_request()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TranscriptionGap {
    pub(crate) job_id: u64,
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) code: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalSubmission {
    Enqueued,
    Gap(TranscriptionGap),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartialSubmission {
    Enqueued,
    Replaced,
    Suppressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchedulerSubmissionError {
    InvalidRequest(TranscriptionError),
    DuplicateJob,
}

impl SchedulerSubmissionError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidRequest(error) => error.code(),
            Self::DuplicateJob => "transcription_job_duplicate",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SchedulerDiagnostics {
    pub(crate) finals_enqueued: u64,
    pub(crate) finals_dequeued: u64,
    pub(crate) final_gaps: u64,
    pub(crate) final_gap_speech_ms: u64,
    pub(crate) partials_enqueued: u64,
    pub(crate) partials_replaced: u64,
    pub(crate) partials_suppressed: u64,
    pub(crate) partials_dequeued: u64,
    pub(crate) partial_disable_transitions: u64,
    pub(crate) partial_restore_transitions: u64,
    pub(crate) peak_queued_final_jobs: usize,
    pub(crate) peak_queued_final_speech_ms: u64,
    pub(crate) peak_logical_queued_samples: usize,
}

#[derive(Default)]
struct PartialSlots {
    microphone: Option<ScheduledTranscriptionJob>,
    system_output: Option<ScheduledTranscriptionJob>,
}

impl PartialSlots {
    fn slot(&self, source: AudioSource) -> &Option<ScheduledTranscriptionJob> {
        match source {
            AudioSource::Microphone => &self.microphone,
            AudioSource::SystemOutput => &self.system_output,
        }
    }

    fn slot_mut(&mut self, source: AudioSource) -> &mut Option<ScheduledTranscriptionJob> {
        match source {
            AudioSource::Microphone => &mut self.microphone,
            AudioSource::SystemOutput => &mut self.system_output,
        }
    }

    fn contains_id(&self, job_id: u64) -> bool {
        self.microphone
            .as_ref()
            .is_some_and(|job| job.job_id() == job_id)
            || self
                .system_output
                .as_ref()
                .is_some_and(|job| job.job_id() == job_id)
    }

    fn remove_id(&mut self, job_id: u64) {
        for source in [AudioSource::Microphone, AudioSource::SystemOutput] {
            if self
                .slot(source)
                .as_ref()
                .is_some_and(|job| job.job_id() == job_id)
            {
                self.slot_mut(source).take();
            }
        }
    }

    fn clear(&mut self) {
        self.microphone = None;
        self.system_output = None;
    }

    fn count(&self) -> usize {
        usize::from(self.microphone.is_some()) + usize::from(self.system_output.is_some())
    }

    fn logical_samples(&self) -> usize {
        self.microphone
            .as_ref()
            .map_or(0, |job| job.request.samples.len())
            .saturating_add(
                self.system_output
                    .as_ref()
                    .map_or(0, |job| job.request.samples.len()),
            )
    }
}

pub(crate) struct TranscriptionScheduler {
    finals: VecDeque<ScheduledTranscriptionJob>,
    partials: PartialSlots,
    queued_final_speech_ms: u64,
    queued_final_samples: usize,
    partials_enabled: bool,
    next_partial_source: AudioSource,
    next_sequence: u64,
    diagnostics: SchedulerDiagnostics,
}

impl Default for TranscriptionScheduler {
    fn default() -> Self {
        Self {
            finals: VecDeque::new(),
            partials: PartialSlots::default(),
            queued_final_speech_ms: 0,
            queued_final_samples: 0,
            partials_enabled: true,
            next_partial_source: AudioSource::Microphone,
            next_sequence: 0,
            diagnostics: SchedulerDiagnostics::default(),
        }
    }
}

impl TranscriptionScheduler {
    pub(crate) fn submit_final(
        &mut self,
        job_id: u64,
        source: AudioSource,
        start_ms: u64,
        end_ms: u64,
        samples: Arc<[f32]>,
    ) -> Result<FinalSubmission, SchedulerSubmissionError> {
        let request = OwnedTranscriptionRequest::new(job_id, source, start_ms, end_ms, samples)
            .map_err(SchedulerSubmissionError::InvalidRequest)?;
        if self.finals.iter().any(|job| job.job_id() == job_id) {
            return Err(SchedulerSubmissionError::DuplicateJob);
        }

        self.partials.remove_id(job_id);
        let duration_ms = request.duration_ms();
        if self.finals.len() >= MAX_QUEUED_FINAL_JOBS
            || duration_ms > MAX_QUEUED_FINAL_SPEECH_MS - self.queued_final_speech_ms
            || request.samples.len() > MAX_QUEUED_FINAL_SAMPLES - self.queued_final_samples
        {
            self.diagnostics.final_gaps = self.diagnostics.final_gaps.saturating_add(1);
            self.diagnostics.final_gap_speech_ms = self
                .diagnostics
                .final_gap_speech_ms
                .saturating_add(duration_ms);
            return Ok(FinalSubmission::Gap(TranscriptionGap {
                job_id,
                source,
                start_ms,
                end_ms,
                code: FINAL_BACKLOG_GAP_CODE,
            }));
        }

        let job = ScheduledTranscriptionJob {
            request,
            sequence: self.take_sequence(),
            kind: ScheduledJobKind::Final,
        };
        let insert_at = self
            .finals
            .iter()
            .position(|queued| final_sort_key(&job) < final_sort_key(queued))
            .unwrap_or(self.finals.len());
        self.queued_final_speech_ms = self.queued_final_speech_ms.saturating_add(duration_ms);
        self.queued_final_samples = self
            .queued_final_samples
            .saturating_add(job.request.samples.len());
        self.finals.insert(insert_at, job);
        self.diagnostics.finals_enqueued = self.diagnostics.finals_enqueued.saturating_add(1);
        self.update_partial_mode();
        self.update_peaks();
        Ok(FinalSubmission::Enqueued)
    }

    pub(crate) fn submit_partial(
        &mut self,
        job_id: u64,
        source: AudioSource,
        start_ms: u64,
        end_ms: u64,
        samples: Arc<[f32]>,
    ) -> Result<PartialSubmission, SchedulerSubmissionError> {
        let request = OwnedTranscriptionRequest::new(job_id, source, start_ms, end_ms, samples)
            .map_err(SchedulerSubmissionError::InvalidRequest)?;
        if self.finals.iter().any(|job| job.job_id() == job_id)
            || (self.partials.contains_id(job_id)
                && self
                    .partials
                    .slot(source)
                    .as_ref()
                    .is_none_or(|job| job.job_id() != job_id))
        {
            return Err(SchedulerSubmissionError::DuplicateJob);
        }
        if !self.partials_enabled {
            self.diagnostics.partials_suppressed =
                self.diagnostics.partials_suppressed.saturating_add(1);
            return Ok(PartialSubmission::Suppressed);
        }

        let sequence = self.take_sequence();
        let slot = self.partials.slot_mut(source);
        let replaced = slot.is_some();
        *slot = Some(ScheduledTranscriptionJob {
            request,
            sequence,
            kind: ScheduledJobKind::Partial,
        });
        let outcome = if replaced {
            self.diagnostics.partials_replaced =
                self.diagnostics.partials_replaced.saturating_add(1);
            PartialSubmission::Replaced
        } else {
            self.diagnostics.partials_enqueued =
                self.diagnostics.partials_enqueued.saturating_add(1);
            PartialSubmission::Enqueued
        };
        self.update_peaks();
        Ok(outcome)
    }

    pub(crate) fn next_job(&mut self) -> Option<ScheduledTranscriptionJob> {
        if let Some(job) = self.next_final_job() {
            return Some(job);
        }

        self.next_partial_job()
    }

    pub(crate) fn next_partial_job(&mut self) -> Option<ScheduledTranscriptionJob> {
        let preferred = self.next_partial_source;
        let alternate = other_source(preferred);
        let (source, job) = if let Some(job) = self.partials.slot_mut(preferred).take() {
            (preferred, job)
        } else if let Some(job) = self.partials.slot_mut(alternate).take() {
            (alternate, job)
        } else {
            return None;
        };
        self.next_partial_source = other_source(source);
        self.diagnostics.partials_dequeued = self.diagnostics.partials_dequeued.saturating_add(1);
        Some(job)
    }

    pub(crate) fn next_final_job(&mut self) -> Option<ScheduledTranscriptionJob> {
        let job = self.finals.pop_front()?;
        self.queued_final_speech_ms = self
            .queued_final_speech_ms
            .saturating_sub(job.duration_ms());
        self.queued_final_samples = self
            .queued_final_samples
            .saturating_sub(job.request.samples.len());
        self.diagnostics.finals_dequeued = self.diagnostics.finals_dequeued.saturating_add(1);
        self.update_partial_mode();
        Some(job)
    }

    pub(crate) fn next_final_start_ms(&self) -> Option<u64> {
        self.finals.front().map(ScheduledTranscriptionJob::start_ms)
    }

    pub(crate) fn discard_partials(&mut self) -> usize {
        let discarded = self.partials.count();
        self.partials.clear();
        discarded
    }

    pub(crate) fn queued_final_jobs(&self) -> usize {
        self.finals.len()
    }

    pub(crate) const fn queued_final_speech_ms(&self) -> u64 {
        self.queued_final_speech_ms
    }

    pub(crate) const fn queued_final_samples(&self) -> usize {
        self.queued_final_samples
    }

    pub(crate) fn queued_partial_jobs(&self) -> usize {
        self.partials.count()
    }

    pub(crate) const fn partials_enabled(&self) -> bool {
        self.partials_enabled
    }

    pub(crate) const fn diagnostics(&self) -> SchedulerDiagnostics {
        self.diagnostics
    }

    fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }

    fn update_partial_mode(&mut self) {
        if self.partials_enabled && self.queued_final_speech_ms >= PARTIAL_DISABLE_SPEECH_MS {
            self.partials_enabled = false;
            self.partials.clear();
            self.diagnostics.partial_disable_transitions = self
                .diagnostics
                .partial_disable_transitions
                .saturating_add(1);
        } else if !self.partials_enabled && self.queued_final_speech_ms < PARTIAL_RESTORE_SPEECH_MS
        {
            self.partials_enabled = true;
            self.diagnostics.partial_restore_transitions = self
                .diagnostics
                .partial_restore_transitions
                .saturating_add(1);
        }
    }

    fn update_peaks(&mut self) {
        self.diagnostics.peak_queued_final_jobs = self
            .diagnostics
            .peak_queued_final_jobs
            .max(self.finals.len());
        self.diagnostics.peak_queued_final_speech_ms = self
            .diagnostics
            .peak_queued_final_speech_ms
            .max(self.queued_final_speech_ms);
        self.diagnostics.peak_logical_queued_samples =
            self.diagnostics.peak_logical_queued_samples.max(
                self.queued_final_samples
                    .saturating_add(self.partials.logical_samples()),
            );
    }
}

fn final_sort_key(job: &ScheduledTranscriptionJob) -> (u64, u64, u64) {
    (job.start_ms(), job.end_ms(), job.sequence)
}

const fn other_source(source: AudioSource) -> AudioSource {
    match source {
        AudioSource::Microphone => AudioSource::SystemOutput,
        AudioSource::SystemOutput => AudioSource::Microphone,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn samples(duration_ms: usize) -> Arc<[f32]> {
        vec![0.1; duration_ms * TRANSCRIPTION_SAMPLE_RATE / 1_000].into()
    }

    fn submit_final(
        scheduler: &mut TranscriptionScheduler,
        job_id: u64,
        source: AudioSource,
        start_ms: u64,
        duration_ms: u64,
        audio: &Arc<[f32]>,
    ) -> FinalSubmission {
        scheduler
            .submit_final(
                job_id,
                source,
                start_ms,
                start_ms + duration_ms,
                Arc::clone(audio),
            )
            .expect("valid final submission")
    }

    fn submit_partial(
        scheduler: &mut TranscriptionScheduler,
        job_id: u64,
        source: AudioSource,
        start_ms: u64,
        duration_ms: u64,
        audio: &Arc<[f32]>,
    ) -> PartialSubmission {
        scheduler
            .submit_partial(
                job_id,
                source,
                start_ms,
                start_ms + duration_ms,
                Arc::clone(audio),
            )
            .expect("valid partial submission")
    }

    #[test]
    fn finals_are_chronological_and_preempt_replaceable_partials() {
        let audio = samples(1_000);
        let mut scheduler = TranscriptionScheduler::default();
        assert_eq!(
            submit_partial(
                &mut scheduler,
                90,
                AudioSource::Microphone,
                0,
                1_000,
                &audio
            ),
            PartialSubmission::Enqueued
        );
        assert_eq!(
            submit_partial(
                &mut scheduler,
                91,
                AudioSource::Microphone,
                200,
                1_000,
                &audio
            ),
            PartialSubmission::Replaced
        );
        submit_final(
            &mut scheduler,
            3,
            AudioSource::SystemOutput,
            2_000,
            1_000,
            &audio,
        );
        submit_final(
            &mut scheduler,
            1,
            AudioSource::SystemOutput,
            0,
            1_000,
            &audio,
        );
        submit_final(
            &mut scheduler,
            2,
            AudioSource::Microphone,
            1_000,
            1_000,
            &audio,
        );

        for expected_id in [1, 2, 3] {
            let job = scheduler.next_job().expect("queued final");
            assert_eq!(job.kind(), ScheduledJobKind::Final);
            assert_eq!(job.job_id(), expected_id);
            assert!(job.request().validate().is_ok());
        }
        let partial = scheduler.next_job().expect("latest partial");
        assert_eq!(partial.kind(), ScheduledJobKind::Partial);
        assert_eq!(partial.job_id(), 91);
        assert!(scheduler.next_job().is_none());
    }

    #[test]
    fn partial_slots_are_source_local_and_selected_fairly() {
        let audio = samples(1_000);
        let mut scheduler = TranscriptionScheduler::default();
        submit_partial(&mut scheduler, 1, AudioSource::Microphone, 0, 1_000, &audio);
        submit_partial(
            &mut scheduler,
            2,
            AudioSource::SystemOutput,
            0,
            1_000,
            &audio,
        );
        assert_eq!(scheduler.queued_partial_jobs(), 2);
        assert_eq!(
            scheduler.next_job().expect("microphone partial").source(),
            AudioSource::Microphone
        );

        submit_partial(
            &mut scheduler,
            3,
            AudioSource::Microphone,
            1_000,
            1_000,
            &audio,
        );
        assert_eq!(
            scheduler
                .next_job()
                .expect("system output gets the next turn")
                .source(),
            AudioSource::SystemOutput
        );
        assert_eq!(
            scheduler
                .next_job()
                .expect("microphone remains queued")
                .source(),
            AudioSource::Microphone
        );
    }

    #[test]
    fn partial_hysteresis_disables_at_twenty_seconds_and_restores_below_ten() {
        let eleven_seconds = samples(11_000);
        let nine_seconds = samples(9_000);
        let partial_audio = samples(1_000);
        let mut scheduler = TranscriptionScheduler::default();
        submit_partial(
            &mut scheduler,
            10,
            AudioSource::Microphone,
            0,
            1_000,
            &partial_audio,
        );
        submit_final(
            &mut scheduler,
            1,
            AudioSource::Microphone,
            0,
            11_000,
            &eleven_seconds,
        );
        submit_final(
            &mut scheduler,
            2,
            AudioSource::SystemOutput,
            11_000,
            9_000,
            &nine_seconds,
        );

        assert!(!scheduler.partials_enabled());
        assert_eq!(scheduler.queued_partial_jobs(), 0);
        assert_eq!(
            submit_partial(
                &mut scheduler,
                11,
                AudioSource::SystemOutput,
                20_000,
                1_000,
                &partial_audio
            ),
            PartialSubmission::Suppressed
        );
        scheduler.next_job().expect("eleven-second final");
        assert!(scheduler.partials_enabled());
        assert_eq!(scheduler.queued_final_speech_ms(), 9_000);
        assert_eq!(scheduler.diagnostics().partial_disable_transitions, 1);
        assert_eq!(scheduler.diagnostics().partial_restore_transitions, 1);
    }

    #[test]
    fn hard_backlog_cap_returns_an_explicit_gap_without_growing() {
        let audio = samples(30_000);
        let mut scheduler = TranscriptionScheduler::default();
        for index in 0..20_u64 {
            assert_eq!(
                submit_final(
                    &mut scheduler,
                    index,
                    if index % 2 == 0 {
                        AudioSource::Microphone
                    } else {
                        AudioSource::SystemOutput
                    },
                    index * 30_000,
                    30_000,
                    &audio,
                ),
                FinalSubmission::Enqueued
            );
        }
        assert_eq!(scheduler.queued_final_speech_ms(), 600_000);
        assert_eq!(scheduler.queued_final_samples(), MAX_QUEUED_FINAL_SAMPLES);

        let rejected = submit_final(
            &mut scheduler,
            21,
            AudioSource::SystemOutput,
            600_000,
            30_000,
            &audio,
        );
        assert_eq!(
            rejected,
            FinalSubmission::Gap(TranscriptionGap {
                job_id: 21,
                source: AudioSource::SystemOutput,
                start_ms: 600_000,
                end_ms: 630_000,
                code: "transcription_final_backlog_exceeded",
            })
        );
        assert_eq!(scheduler.queued_final_jobs(), 20);
        assert_eq!(scheduler.queued_final_speech_ms(), 600_000);
    }

    #[test]
    fn job_count_limit_bounds_minimum_duration_queue_overhead() {
        let one_millisecond = samples(1);
        let mut scheduler = TranscriptionScheduler::default();
        assert_eq!(
            submit_partial(
                &mut scheduler,
                10_000,
                AudioSource::SystemOutput,
                10_000,
                1,
                &one_millisecond,
            ),
            PartialSubmission::Enqueued
        );
        for index in 0..MAX_QUEUED_FINAL_JOBS as u64 {
            assert_eq!(
                submit_final(
                    &mut scheduler,
                    index,
                    AudioSource::Microphone,
                    index,
                    1,
                    &one_millisecond,
                ),
                FinalSubmission::Enqueued
            );
        }
        assert!(matches!(
            submit_final(
                &mut scheduler,
                MAX_QUEUED_FINAL_JOBS as u64,
                AudioSource::SystemOutput,
                MAX_QUEUED_FINAL_JOBS as u64,
                1,
                &one_millisecond,
            ),
            FinalSubmission::Gap(_)
        ));
        assert_eq!(scheduler.queued_final_jobs(), MAX_QUEUED_FINAL_JOBS);
        assert_eq!(
            submit_final(
                &mut scheduler,
                10_000,
                AudioSource::SystemOutput,
                10_000,
                1,
                &one_millisecond,
            ),
            FinalSubmission::Gap(TranscriptionGap {
                job_id: 10_000,
                source: AudioSource::SystemOutput,
                start_ms: 10_000,
                end_ms: 10_001,
                code: FINAL_BACKLOG_GAP_CODE,
            })
        );
        assert_eq!(scheduler.queued_partial_jobs(), 0);
    }

    struct DeterministicSlowInference {
        slowdown: u64,
        busy_until_ms: Option<u64>,
        in_flight: Option<ScheduledTranscriptionJob>,
        started: Vec<(u64, AudioSource, u64)>,
    }

    impl DeterministicSlowInference {
        fn new(slowdown: u64) -> Self {
            assert!(slowdown > 0);
            Self {
                slowdown,
                busy_until_ms: None,
                in_flight: None,
                started: Vec::new(),
            }
        }

        fn tick(&mut self, now_ms: u64, scheduler: &mut TranscriptionScheduler) {
            if self
                .busy_until_ms
                .is_some_and(|deadline| deadline <= now_ms)
            {
                self.busy_until_ms = None;
                self.in_flight = None;
            }
            if self.in_flight.is_none()
                && let Some(job) = scheduler.next_job()
            {
                let service_ms = job.duration_ms().saturating_mul(self.slowdown);
                self.busy_until_ms = Some(now_ms.saturating_add(service_ms));
                self.started
                    .push((job.job_id(), job.source(), job.start_ms()));
                self.in_flight = Some(job);
            }
        }
    }

    #[test]
    fn fake_inference_slowdown_is_deterministic_before_the_soak() {
        let audio = samples(1_000);
        let mut scheduler = TranscriptionScheduler::default();
        submit_final(&mut scheduler, 1, AudioSource::Microphone, 0, 1_000, &audio);
        submit_final(
            &mut scheduler,
            2,
            AudioSource::SystemOutput,
            1_000,
            1_000,
            &audio,
        );
        let mut worker = DeterministicSlowInference::new(5);
        worker.tick(0, &mut scheduler);
        worker.tick(4_999, &mut scheduler);
        assert_eq!(worker.started.len(), 1);
        worker.tick(5_000, &mut scheduler);
        assert_eq!(worker.started.len(), 2);
        assert_eq!(worker.busy_until_ms, Some(10_000));
    }

    #[test]
    fn two_hour_slow_inference_soak_plateaus_and_preserves_both_sources() {
        const TWO_HOURS_SECONDS: u64 = 2 * 60 * 60;
        const PLATEAU_START_SECONDS: u64 = 60 * 60;

        let audio = samples(1_000);
        let mut scheduler = TranscriptionScheduler::default();
        let mut worker = DeterministicSlowInference::new(5);
        let mut explicit_gaps = 0_u64;
        let mut microphone_gaps = 0_u64;
        let mut system_output_gaps = 0_u64;
        let mut plateau_min_samples = usize::MAX;
        let mut plateau_max_samples = 0_usize;

        for second in 0..TWO_HOURS_SECONDS {
            let now_ms = second * 1_000;
            worker.tick(now_ms, &mut scheduler);
            let source = if second % 2 == 0 {
                AudioSource::Microphone
            } else {
                AudioSource::SystemOutput
            };
            match submit_final(&mut scheduler, second, source, now_ms, 1_000, &audio) {
                FinalSubmission::Enqueued => {}
                FinalSubmission::Gap(gap) => {
                    assert_eq!(gap.code, FINAL_BACKLOG_GAP_CODE);
                    explicit_gaps = explicit_gaps.saturating_add(1);
                    match gap.source {
                        AudioSource::Microphone => microphone_gaps += 1,
                        AudioSource::SystemOutput => system_output_gaps += 1,
                    }
                }
            }
            assert!(matches!(
                submit_partial(
                    &mut scheduler,
                    1_000_000 + second,
                    other_source(source),
                    now_ms,
                    1_000,
                    &audio,
                ),
                PartialSubmission::Enqueued
                    | PartialSubmission::Replaced
                    | PartialSubmission::Suppressed
            ));
            worker.tick(now_ms, &mut scheduler);

            assert!(scheduler.queued_final_speech_ms() <= MAX_QUEUED_FINAL_SPEECH_MS);
            assert!(scheduler.queued_final_samples() <= MAX_QUEUED_FINAL_SAMPLES);
            assert!(scheduler.queued_final_jobs() <= MAX_QUEUED_FINAL_JOBS);
            assert!(scheduler.queued_partial_jobs() <= 2);
            if second >= PLATEAU_START_SECONDS {
                plateau_min_samples = plateau_min_samples.min(scheduler.queued_final_samples());
                plateau_max_samples = plateau_max_samples.max(scheduler.queued_final_samples());
            }
        }

        assert!(explicit_gaps > 0);
        assert_eq!(explicit_gaps, scheduler.diagnostics().final_gaps);
        assert!(microphone_gaps.abs_diff(system_output_gaps) <= 1);
        assert_eq!(
            scheduler.queued_final_speech_ms(),
            MAX_QUEUED_FINAL_SPEECH_MS
        );
        assert_eq!(plateau_min_samples, MAX_QUEUED_FINAL_SAMPLES);
        assert_eq!(plateau_max_samples, MAX_QUEUED_FINAL_SAMPLES);
        assert!(!scheduler.partials_enabled());
        assert_eq!(scheduler.queued_partial_jobs(), 0);
        assert!(scheduler.diagnostics().partials_suppressed > 0);
        assert!(scheduler.diagnostics().peak_logical_queued_samples <= MAX_QUEUED_FINAL_SAMPLES);

        let mut ids = HashSet::new();
        let mut previous_start_ms = 0;
        let mut microphone_started = 0_u64;
        let mut system_output_started = 0_u64;
        for (index, (job_id, source, start_ms)) in worker.started.iter().copied().enumerate() {
            assert!(ids.insert(job_id), "accepted final started more than once");
            if index > 0 {
                assert!(start_ms >= previous_start_ms, "final chronology regressed");
            }
            previous_start_ms = start_ms;
            match source {
                AudioSource::Microphone => microphone_started += 1,
                AudioSource::SystemOutput => system_output_started += 1,
            }
        }
        assert!(microphone_started > 0 && system_output_started > 0);
        assert!(microphone_started.abs_diff(system_output_started) <= 1);
        let started_jobs = worker.started.len();
        let queued_jobs = scheduler.queued_final_jobs();
        let dequeued_before_drain = scheduler.diagnostics().finals_dequeued;
        assert_eq!(started_jobs as u64, dequeued_before_drain);
        while let Some(job) = scheduler.next_job() {
            assert_eq!(job.kind(), ScheduledJobKind::Final);
            assert!(ids.insert(job.job_id()), "accepted final was duplicated");
            assert!(
                job.start_ms() >= previous_start_ms,
                "pending final chronology regressed"
            );
            previous_start_ms = job.start_ms();
        }
        assert_eq!(
            started_jobs + queued_jobs,
            scheduler.diagnostics().finals_enqueued as usize
        );
        println!(
            "simulated_seconds={} slowdown=5x finals_enqueued={} finals_started={} final_gaps={} microphone_started={} system_output_started={} microphone_gaps={} system_output_gaps={} plateau_min_samples={} plateau_max_samples={} peak_logical_samples={} max_final_jobs={} partial_slots_max=2",
            TWO_HOURS_SECONDS,
            scheduler.diagnostics().finals_enqueued,
            started_jobs,
            explicit_gaps,
            microphone_started,
            system_output_started,
            microphone_gaps,
            system_output_gaps,
            plateau_min_samples,
            plateau_max_samples,
            scheduler.diagnostics().peak_logical_queued_samples,
            scheduler.diagnostics().peak_queued_final_jobs,
        );
    }

    #[test]
    fn invalid_and_duplicate_jobs_return_fixed_path_free_codes() {
        let audio = samples(1_000);
        let mut scheduler = TranscriptionScheduler::default();
        submit_final(&mut scheduler, 1, AudioSource::Microphone, 0, 1_000, &audio);
        let duplicate = scheduler
            .submit_final(
                1,
                AudioSource::SystemOutput,
                1_000,
                2_000,
                Arc::clone(&audio),
            )
            .expect_err("duplicate rejected");
        assert_eq!(duplicate.code(), "transcription_job_duplicate");

        let invalid = scheduler
            .submit_partial(2, AudioSource::Microphone, 0, 999, Arc::clone(&audio))
            .expect_err("invalid timeline rejected");
        assert_eq!(invalid.code(), "transcription_timeline_invalid");
        for code in [duplicate.code(), invalid.code(), FINAL_BACKLOG_GAP_CODE] {
            assert!(!code.contains(['\\', '/']));
        }
    }
}
