use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::RecvTimeoutError;

use crate::audio::{
    AudioSource, BoundedReceiver, BoundedSender, FinalizedAudioUpdate, bounded_queue,
};

use super::{
    TranscriptionEngine,
    integration::{
        LiveFinalTranscriptionPipeline, LivePipelineDiagnostics, LivePipelineEvent,
        LivePipelineState,
    },
};

#[cfg(windows)]
use super::{
    TranscriptionError, WhisperModelKind,
    whisper::{WhisperConfig, WhisperEngine},
};
#[cfg(windows)]
use crate::models::VerifiedModelArtifact;

const COORDINATOR_POLL: Duration = Duration::from_millis(10);
const PIPELINE_FAILURE_CODE: &str = "transcription_live_pipeline_failed";

#[cfg(windows)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn load_verified_cpu_engine(
    artifact: VerifiedModelArtifact,
    adapter_path: impl Into<std::path::PathBuf>,
    language: &str,
    threads: usize,
) -> Result<WhisperEngine, TranscriptionError> {
    let model_kind = whisper_model_kind(&artifact)?;
    WhisperEngine::load(
        WhisperConfig::cpu(adapter_path, artifact.path, model_kind, threads)
            .with_language(language)?,
    )
}

#[cfg(windows)]
pub(crate) fn whisper_model_kind(
    artifact: &VerifiedModelArtifact,
) -> Result<WhisperModelKind, TranscriptionError> {
    Ok(match artifact.model_id.as_str() {
        "whisper-tiny-multilingual" => WhisperModelKind::Tiny,
        "whisper-base-multilingual" => WhisperModelKind::Base,
        "whisper-large-v3-turbo-q5_0-multilingual" => WhisperModelKind::LargeV3TurboQ5_0,
        _ => return Err(TranscriptionError::ModelUnavailable),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LiveCoordinatorSummary {
    pub(crate) state: LivePipelineState,
    pub(crate) diagnostics: LivePipelineDiagnostics,
    pub(crate) updates_received: u64,
    pub(crate) terminal_sources: u8,
    pub(crate) error_code: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LiveCoordinatorError {
    pub(crate) code: &'static str,
}

pub(crate) struct LiveCoordinatorHandle {
    cancelled: Arc<AtomicBool>,
    join: Option<JoinHandle<LiveCoordinatorSummary>>,
}

impl LiveCoordinatorHandle {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn join(mut self) -> Result<LiveCoordinatorSummary, LiveCoordinatorError> {
        self.join
            .take()
            .expect("live coordinator join handle")
            .join()
            .map_err(|_| LiveCoordinatorError {
                code: "transcription_coordinator_panicked",
            })
    }
}

impl Drop for LiveCoordinatorHandle {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

pub(crate) fn spawn_live_coordinator<E>(
    engine: E,
    updates: BoundedReceiver<FinalizedAudioUpdate>,
    output_capacity: usize,
) -> Result<(LiveCoordinatorHandle, BoundedReceiver<LivePipelineEvent>), LiveCoordinatorError>
where
    E: TranscriptionEngine + Send + 'static,
{
    if output_capacity == 0 {
        return Err(LiveCoordinatorError {
            code: "transcription_output_capacity_invalid",
        });
    }
    let (output, records) = bounded_queue(output_capacity);
    let cancelled = Arc::new(AtomicBool::new(false));
    let thread_cancelled = Arc::clone(&cancelled);
    let join = thread::Builder::new()
        .name("live-transcription-coordinator".to_owned())
        .spawn(move || run_coordinator(engine, updates, output, &thread_cancelled))
        .map_err(|_| LiveCoordinatorError {
            code: "transcription_coordinator_unavailable",
        })?;
    Ok((
        LiveCoordinatorHandle {
            cancelled,
            join: Some(join),
        },
        records,
    ))
}

fn run_coordinator<E: TranscriptionEngine>(
    engine: E,
    updates: BoundedReceiver<FinalizedAudioUpdate>,
    output: BoundedSender<LivePipelineEvent>,
    cancelled: &AtomicBool,
) -> LiveCoordinatorSummary {
    let mut pipeline = LiveFinalTranscriptionPipeline::new(engine);
    let mut updates_received = 0_u64;
    let mut sealed = [false; 2];
    let mut error_code = None;

    loop {
        if cancelled.load(Ordering::Acquire) {
            emit_all(&output, pipeline.cancel_pending());
            break;
        }
        match updates.receiver().recv_timeout(COORDINATOR_POLL) {
            Ok(update) => {
                updates_received = updates_received.saturating_add(1);
                let source = update.source;
                let terminal = update.terminal;
                let partial_result = update
                    .partial
                    .map_or(Ok(()), |partial| pipeline.submit_partial(partial));
                let submission = partial_result.and_then(|()| {
                    pipeline.submit_finalized_vad(
                        source,
                        update.utterances,
                        update.ordering_watermark_ms,
                    )
                });
                match submission {
                    Ok(events) => {
                        if !emit_all(&output, events) {
                            cancelled.store(true, Ordering::Release);
                            emit_all(&output, pipeline.cancel_pending());
                            break;
                        }
                    }
                    Err(_) => {
                        error_code = Some(PIPELINE_FAILURE_CODE);
                        emit_all(&output, pipeline.cancel_pending());
                        break;
                    }
                }
                if terminal {
                    if pipeline.state() == LivePipelineState::Running
                        && pipeline.begin_stop().is_err()
                    {
                        error_code = Some(PIPELINE_FAILURE_CODE);
                        emit_all(&output, pipeline.cancel_pending());
                        break;
                    }
                    let index = source_index(source);
                    if sealed[index] || pipeline.seal_source(source).is_err() {
                        error_code = Some(PIPELINE_FAILURE_CODE);
                        emit_all(&output, pipeline.cancel_pending());
                        break;
                    }
                    sealed[index] = true;
                }
                if !pump_ready(&mut pipeline, &output, cancelled) {
                    break;
                }
                if pipeline.state() == LivePipelineState::Stopped {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if cancelled.load(Ordering::Acquire) {
                    emit_all(&output, pipeline.cancel_pending());
                } else {
                    close_open_sources(&mut pipeline, &mut sealed);
                    pump_ready(&mut pipeline, &output, cancelled);
                }
                break;
            }
        }
    }

    LiveCoordinatorSummary {
        state: pipeline.state(),
        diagnostics: pipeline.diagnostics(),
        updates_received,
        terminal_sources: sealed.into_iter().filter(|value| *value).count() as u8,
        error_code,
    }
}

fn pump_ready<E: TranscriptionEngine>(
    pipeline: &mut LiveFinalTranscriptionPipeline<E>,
    output: &BoundedSender<LivePipelineEvent>,
    cancelled: &AtomicBool,
) -> bool {
    while let Some(event) = pipeline.pump_one(cancelled) {
        if output.send(event).is_err() {
            cancelled.store(true, Ordering::Release);
            return false;
        }
    }
    true
}

fn emit_all(
    output: &BoundedSender<LivePipelineEvent>,
    events: impl IntoIterator<Item = LivePipelineEvent>,
) -> bool {
    for event in events {
        if output.send(event).is_err() {
            return false;
        }
    }
    true
}

fn close_open_sources<E: TranscriptionEngine>(
    pipeline: &mut LiveFinalTranscriptionPipeline<E>,
    sealed: &mut [bool; 2],
) {
    if pipeline.state() == LivePipelineState::Running {
        let _ = pipeline.begin_stop();
    }
    for source in [AudioSource::Microphone, AudioSource::SystemOutput] {
        let index = source_index(source);
        if !sealed[index] && pipeline.seal_source(source).is_ok() {
            sealed[index] = true;
        }
    }
}

const fn source_index(source: AudioSource) -> usize {
    match source {
        AudioSource::Microphone => 0,
        AudioSource::SystemOutput => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        time::{Duration, Instant},
    };

    use crate::{
        audio::{DetectedUtterance, UtteranceEndReason},
        transcription::{
            TranscriptSegment, TranscriptionError, TranscriptionRequest, TranscriptionResult,
        },
    };

    use super::*;

    #[derive(Default)]
    struct FakeEngine {
        fail_microphone_once: bool,
    }

    impl TranscriptionEngine for FakeEngine {
        fn transcribe(
            &mut self,
            request: TranscriptionRequest<'_>,
        ) -> Result<TranscriptionResult, TranscriptionError> {
            if request.source == AudioSource::Microphone && self.fail_microphone_once {
                self.fail_microphone_once = false;
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

    fn utterance(source: AudioSource, start_ms: u64) -> DetectedUtterance {
        DetectedUtterance {
            source,
            start_ms,
            end_ms: start_ms + 1_000,
            samples: vec![0.1; 16_000],
            reason: UtteranceEndReason::TrailingSilence,
        }
    }

    fn update(
        source: AudioSource,
        starts: &[u64],
        watermark: Option<u64>,
        terminal: bool,
    ) -> FinalizedAudioUpdate {
        FinalizedAudioUpdate {
            source,
            utterances: starts
                .iter()
                .map(|start| utterance(source, *start))
                .collect(),
            partial: None,
            ordering_watermark_ms: watermark,
            terminal,
        }
    }

    fn drain(records: BoundedReceiver<LivePipelineEvent>) -> Vec<LivePipelineEvent> {
        records.receiver().iter().collect()
    }

    #[test]
    fn bounded_handoff_delivers_delayed_sources_chronologically_exactly_once() {
        let (sender, updates) = bounded_queue(4);
        let (handle, records) = spawn_live_coordinator(FakeEngine::default(), updates, 1).unwrap();
        let consumer = std::thread::spawn(move || drain(records));

        sender
            .send(update(
                AudioSource::Microphone,
                &[1_000],
                Some(3_000),
                false,
            ))
            .unwrap();
        sender
            .send(update(AudioSource::SystemOutput, &[], Some(200), false))
            .unwrap();
        sender
            .send(update(AudioSource::SystemOutput, &[200], Some(2_000), true))
            .unwrap();
        sender
            .send(update(AudioSource::Microphone, &[], Some(3_000), true))
            .unwrap();
        drop(sender);

        let summary = handle.join().unwrap();
        let events = consumer.join().unwrap();
        let finals = events
            .iter()
            .filter_map(|event| match event {
                LivePipelineEvent::Final { job_id, result } => {
                    Some((*job_id, result.source, result.utterance_start_ms))
                }
                LivePipelineEvent::Partial { .. } | LivePipelineEvent::Gap(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            finals,
            [
                (2, AudioSource::SystemOutput, 200),
                (1, AudioSource::Microphone, 1_000)
            ]
        );
        assert_eq!(
            finals
                .iter()
                .map(|value| value.0)
                .collect::<HashSet<_>>()
                .len(),
            2
        );
        assert_eq!(summary.state, LivePipelineState::Stopped);
        assert_eq!(summary.terminal_sources, 2);
        assert_eq!(summary.error_code, None);
    }

    #[test]
    fn one_source_inference_failure_is_an_explicit_gap_and_other_source_continues() {
        let (sender, updates) = bounded_queue(4);
        let (handle, records) = spawn_live_coordinator(
            FakeEngine {
                fail_microphone_once: true,
            },
            updates,
            4,
        )
        .unwrap();
        let consumer = std::thread::spawn(move || drain(records));
        sender
            .send(update(AudioSource::Microphone, &[0], Some(2_000), true))
            .unwrap();
        sender
            .send(update(
                AudioSource::SystemOutput,
                &[1_000],
                Some(2_000),
                true,
            ))
            .unwrap();
        drop(sender);

        let summary = handle.join().unwrap();
        let events = consumer.join().unwrap();
        assert!(events.iter().any(|event| matches!(
            event,
            LivePipelineEvent::Gap(gap)
                if gap.source == AudioSource::Microphone
                    && gap.code == "transcription_inference_failed"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            LivePipelineEvent::Final { result, .. }
                if result.source == AudioSource::SystemOutput
        )));
        assert_eq!(summary.diagnostics.inference_gaps, 1);
        assert_eq!(summary.diagnostics.final_results, 1);
    }

    #[test]
    fn cancellation_accounts_pending_final_without_waiting_for_source_frontier() {
        let (sender, updates) = bounded_queue(2);
        let (handle, records) = spawn_live_coordinator(FakeEngine::default(), updates, 2).unwrap();
        sender
            .send(update(AudioSource::Microphone, &[0], Some(2_000), false))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while sender.queued_len() != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(20));
        handle.cancel();
        drop(sender);
        let summary = handle.join().unwrap();
        let events = drain(records);
        assert_eq!(summary.state, LivePipelineState::Cancelled);
        assert!(events.iter().any(|event| matches!(
            event,
            LivePipelineEvent::Gap(gap)
                if gap.code == "transcription_pipeline_cancelled"
        )));
    }

    #[test]
    fn bounded_output_pressure_waits_without_losing_or_duplicating_finals() {
        let (sender, updates) = bounded_queue(4);
        let (handle, records) = spawn_live_coordinator(FakeEngine::default(), updates, 1).unwrap();
        sender
            .send(update(AudioSource::Microphone, &[0], Some(3_000), false))
            .unwrap();
        sender
            .send(update(
                AudioSource::SystemOutput,
                &[1_000],
                Some(3_000),
                true,
            ))
            .unwrap();
        sender
            .send(update(AudioSource::Microphone, &[], Some(3_000), true))
            .unwrap();
        drop(sender);

        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            records.receiver().len(),
            1,
            "the output sink must stay bounded"
        );
        let events = drain(records);
        let summary = handle.join().unwrap();
        assert_eq!(events.len(), 2);
        assert!(
            events
                .iter()
                .all(|event| matches!(event, LivePipelineEvent::Final { .. }))
        );
        assert_eq!(summary.diagnostics.finals_received, 2);
        assert_eq!(summary.diagnostics.final_results, 2);
        assert_eq!(summary.state, LivePipelineState::Stopped);
    }

    #[cfg(windows)]
    #[test]
    fn verified_catalog_model_ids_map_to_exact_runtime_kinds() {
        use std::path::PathBuf;

        use crate::models::VerifiedModelArtifact;

        let cases = [
            ("whisper-tiny-multilingual", WhisperModelKind::Tiny),
            ("whisper-base-multilingual", WhisperModelKind::Base),
            (
                "whisper-large-v3-turbo-q5_0-multilingual",
                WhisperModelKind::LargeV3TurboQ5_0,
            ),
        ];
        for (model_id, expected) in cases {
            let artifact = VerifiedModelArtifact {
                model_id: model_id.to_owned(),
                path: PathBuf::from("catalog-owned-model.bin"),
            };
            assert_eq!(whisper_model_kind(&artifact).unwrap(), expected);
        }

        let unknown = VerifiedModelArtifact {
            model_id: "caller-controlled-model".to_owned(),
            path: PathBuf::from("caller-controlled-model.bin"),
        };
        assert_eq!(
            whisper_model_kind(&unknown),
            Err(TranscriptionError::ModelUnavailable)
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires default Windows endpoints, a verified selected model, the external adapter, and fixture playback"]
    fn hardware_probe_live_dual_capture_to_verified_local_partial_and_final() {
        use std::{env, path::PathBuf};

        use crate::{
            audio::{AudioPrototypeConfig, RunningAudioPrototype},
            models::ModelService,
            persistence::SettingsService,
        };

        let app_data = PathBuf::from(
            env::var_os("KOKOROKOE_P3_014_APP_LOCAL_DATA")
                .expect("KOKOROKOE_P3_014_APP_LOCAL_DATA is required"),
        );
        let documents = PathBuf::from(
            env::var_os("KOKOROKOE_P3_014_DOCUMENTS")
                .expect("KOKOROKOE_P3_014_DOCUMENTS is required"),
        );
        let adapter = PathBuf::from(
            env::var_os("KOKOROKOE_P3_014_ADAPTER").expect("KOKOROKOE_P3_014_ADAPTER is required"),
        );
        let audio = PathBuf::from(
            env::var_os("KOKOROKOE_P3_014_AUDIO").expect("KOKOROKOE_P3_014_AUDIO is required"),
        );
        assert!(audio.is_file(), "the playback fixture must exist");

        let settings = SettingsService::open(app_data.clone(), documents).unwrap();
        let artifact = ModelService::open(&app_data, settings)
            .unwrap()
            .resolve_default_for_transcription()
            .unwrap();
        let engine = load_verified_cpu_engine(artifact, adapter, "auto", 8).unwrap();

        let (capture, updates) =
            RunningAudioPrototype::start_with_live_updates(AudioPrototypeConfig::default(), 64)
                .unwrap();
        let (coordinator, records) = spawn_live_coordinator(engine, updates, 16).unwrap();
        let sink = std::thread::spawn(move || {
            let mut final_count = 0_u64;
            let mut nonempty_count = 0_u64;
            let mut sources = HashSet::new();
            let mut gaps = 0_u64;
            let mut previous_start = None;
            let mut partial_ids = HashSet::new();
            let mut nonempty_partials = 0_u64;
            let mut final_ids = HashSet::new();
            for event in records.receiver().iter() {
                match event {
                    LivePipelineEvent::Partial { job_id, result } => {
                        partial_ids.insert(job_id);
                        nonempty_partials += u64::from(!result.text.trim().is_empty());
                    }
                    LivePipelineEvent::Final { job_id, result } => {
                        assert!(
                            previous_start.is_none_or(|value| value <= result.utterance_start_ms)
                        );
                        previous_start = Some(result.utterance_start_ms);
                        final_count += 1;
                        nonempty_count += u64::from(!result.text.trim().is_empty());
                        sources.insert(result.source);
                        final_ids.insert(job_id);
                    }
                    LivePipelineEvent::Gap(_) => gaps += 1,
                }
            }
            (
                final_count,
                nonempty_count,
                sources,
                gaps,
                partial_ids,
                nonempty_partials,
                final_ids,
            )
        });

        std::thread::sleep(Duration::from_secs(24));
        let status = capture.stop();
        let summary = coordinator.join().unwrap();
        let (final_count, nonempty_count, sources, gaps, partial_ids, nonempty_partials, final_ids) =
            sink.join().unwrap();

        assert!(status.microphone.packets_captured > 0);
        assert!(status.system_output.packets_captured > 0);
        assert!(status.microphone.normalized_samples_produced > 0);
        assert!(status.system_output.normalized_samples_produced > 0);
        assert_eq!(summary.state, LivePipelineState::Stopped);
        assert_eq!(summary.terminal_sources, 2);
        assert_eq!(summary.error_code, None);
        assert!(
            final_count > 0,
            "fixture playback must produce a local final"
        );
        assert_eq!(nonempty_count, final_count);
        assert!(
            nonempty_partials > 0,
            "fixture playback must produce a non-empty partial"
        );
        assert!(
            partial_ids.iter().any(|job_id| final_ids.contains(job_id)),
            "a provisional job id must reconcile to its final"
        );
        assert!(!sources.is_empty());
        assert_eq!(
            final_count + gaps,
            summary.diagnostics.finals_received,
            "every finalized utterance must be exactly accounted"
        );
        eprintln!(
            "P3-014 aggregate: microphone_packets={}, system_packets={}, partials={}, finals={}, gaps={}, result_sources={}",
            status.microphone.packets_captured,
            status.system_output.packets_captured,
            partial_ids.len(),
            final_count,
            gaps,
            sources.len()
        );
    }
}
