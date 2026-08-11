use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use uuid::Uuid;

use crate::{
    audio::{AudioPrototypeConfig, RunningAudioPrototype},
    domain::{
        AppError, LiveEventEnvelope, LiveSegmentStatus, LiveTranscriptSegment,
        LiveTranscriptionInput, LiveTranscriptionRunState, LiveTranscriptionStatus, RequestId,
        TranscriptionFinalPayload, TranscriptionGapPayload, TranscriptionPartialPayload,
        now_rfc3339,
    },
    models::ModelService,
};

use super::{LivePipelineEvent, load_verified_cpu_engine, spawn_live_coordinator};

const AUDIO_HANDOFF_CAPACITY: usize = 64;
const EVENT_HANDOFF_CAPACITY: usize = 64;
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub(crate) enum ProductTranscriptionEvent {
    Partial(LiveEventEnvelope<TranscriptionPartialPayload>),
    Final(LiveEventEnvelope<TranscriptionFinalPayload>),
    Gap(LiveEventEnvelope<TranscriptionGapPayload>),
}

type EventSink = Arc<dyn Fn(ProductTranscriptionEvent) + Send + Sync>;

struct ActiveRun {
    request_id: RequestId,
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

#[derive(Default)]
struct ProductState {
    status: LiveTranscriptionStatus,
    active: Option<ActiveRun>,
}

#[derive(Clone)]
pub(crate) struct LiveTranscriptionService {
    models: ModelService,
    adapter_path: PathBuf,
    state: Arc<Mutex<ProductState>>,
}

impl LiveTranscriptionService {
    pub(crate) fn new(models: ModelService, adapter_path: PathBuf) -> Self {
        Self {
            models,
            adapter_path,
            state: Arc::new(Mutex::new(ProductState::default())),
        }
    }

    pub(crate) fn status(&self) -> Result<LiveTranscriptionStatus, AppError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| {
                AppError::live_transcription_error("live_transcription_state_unavailable")
            })?
            .status
            .clone())
    }

    pub(crate) fn start(
        &self,
        input: LiveTranscriptionInput,
        request_id: RequestId,
        emit: EventSink,
    ) -> Result<LiveTranscriptionStatus, AppError> {
        input
            .validate()
            .map_err(AppError::live_transcription_error)?;
        reserve_start(&self.state, request_id)?;

        match self.start_runtime(input, request_id, emit) {
            Ok(active) => {
                let mut state = self.state.lock().map_err(|_| {
                    AppError::live_transcription_error("live_transcription_state_unavailable")
                })?;
                state.status.state = LiveTranscriptionRunState::Running;
                state.active = Some(active);
                state
                    .status
                    .validate()
                    .map_err(AppError::live_transcription_error)?;
                Ok(state.status.clone())
            }
            Err(error) => {
                record_terminal_state(&self.state, request_id, Some(error.clone()));
                Err(error)
            }
        }
    }

    pub(crate) fn stop(&self, request_id: RequestId) -> Result<LiveTranscriptionStatus, AppError> {
        let active = {
            let mut state = self.state.lock().map_err(|_| {
                AppError::live_transcription_error("live_transcription_state_unavailable")
            })?;
            if state.active.is_none()
                && state.status.request_id == Some(request_id)
                && matches!(
                    state.status.state,
                    LiveTranscriptionRunState::Stopped | LiveTranscriptionRunState::Failed
                )
            {
                return Ok(state.status.clone());
            }
            let active = state.active.take().ok_or_else(|| {
                AppError::live_transcription_error("live_transcription_not_running")
            })?;
            if active.request_id != request_id {
                state.active = Some(active);
                return Err(AppError::live_transcription_error(
                    "live_transcription_not_running",
                ));
            }
            state.status.state = LiveTranscriptionRunState::Stopping;
            active.stop.store(true, Ordering::Release);
            active
        };

        if active.join.join().is_err() {
            let error = AppError::live_transcription_error("live_transcription_worker_failed");
            record_terminal_state(&self.state, request_id, Some(error.clone()));
            return Err(error);
        }
        self.status()
    }

    fn start_runtime(
        &self,
        input: LiveTranscriptionInput,
        request_id: RequestId,
        emit: EventSink,
    ) -> Result<ActiveRun, AppError> {
        if !self.adapter_path.is_file() {
            return Err(AppError::live_transcription_error(
                "live_transcription_runtime_unavailable",
            ));
        }
        let artifact = self.models.resolve_default_for_transcription()?;
        let threads = thread::available_parallelism()
            .map_or(1, usize::from)
            .clamp(1, 8);
        let engine = load_verified_cpu_engine(artifact, self.adapter_path.clone(), threads)
            .map_err(|error| AppError::live_transcription_error(error.code()))?;
        let config = AudioPrototypeConfig {
            microphone: input.microphone_selection,
            system_output: input.system_output_selection,
            queue_capacity_packets_per_source: AUDIO_HANDOFF_CAPACITY,
        };
        let (capture, updates) =
            RunningAudioPrototype::start_with_live_updates(config, AUDIO_HANDOFF_CAPACITY)
                .map_err(|error| AppError::live_transcription_error(error.code))?;
        let (coordinator, records) =
            spawn_live_coordinator(engine, updates, EVENT_HANDOFF_CAPACITY)
                .map_err(|error| AppError::live_transcription_error(error.code))?;
        let event_sink = thread::Builder::new()
            .name("product-transcription-events".to_owned())
            .spawn(move || bridge_events(request_id, records, &emit))
            .map_err(|_| AppError::live_transcription_error("live_transcription_worker_failed"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let state = Arc::clone(&self.state);
        let join = thread::Builder::new()
            .name("product-live-transcription".to_owned())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    thread::sleep(STOP_POLL_INTERVAL);
                }
                let _capture_status = capture.stop();
                let coordinator_result = coordinator.join();
                let sink_result = event_sink.join();
                let coordinator_failed = match coordinator_result {
                    Ok(summary) => summary.error_code.is_some(),
                    Err(_) => true,
                };
                let error = if coordinator_failed || sink_result.is_err() {
                    Some(AppError::live_transcription_error(
                        "live_transcription_worker_failed",
                    ))
                } else {
                    None
                };
                record_terminal_state(&state, request_id, error);
            })
            .map_err(|_| AppError::live_transcription_error("live_transcription_worker_failed"))?;
        Ok(ActiveRun {
            request_id,
            stop,
            join,
        })
    }
}

fn reserve_start(state: &Arc<Mutex<ProductState>>, request_id: RequestId) -> Result<(), AppError> {
    let mut state = state
        .lock()
        .map_err(|_| AppError::live_transcription_error("live_transcription_state_unavailable"))?;
    if state.active.is_some()
        || matches!(
            state.status.state,
            LiveTranscriptionRunState::Starting
                | LiveTranscriptionRunState::Running
                | LiveTranscriptionRunState::Stopping
        )
    {
        return Err(AppError::live_transcription_error(
            "live_transcription_already_running",
        ));
    }
    if state.status.request_id == Some(request_id) {
        return Err(AppError::live_transcription_error(
            "live_transcription_request_conflict",
        ));
    }
    state.status = LiveTranscriptionStatus::starting(request_id)?;
    Ok(())
}

fn record_terminal_state(
    state: &Arc<Mutex<ProductState>>,
    request_id: RequestId,
    error: Option<AppError>,
) {
    if let Ok(mut state) = state.lock()
        && state.status.request_id == Some(request_id)
    {
        state.status.state = if error.is_some() {
            LiveTranscriptionRunState::Failed
        } else {
            LiveTranscriptionRunState::Stopped
        };
        state.status.stopped_at =
            Some(now_rfc3339().unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()));
        state.status.error = error;
    }
}

fn bridge_events(
    request_id: RequestId,
    records: crate::audio::BoundedReceiver<LivePipelineEvent>,
    emit: &EventSink,
) {
    let mut segment_ids = HashMap::new();
    let mut partials = HashSet::new();
    let mut sequence = 0_u64;
    for event in records.receiver().iter() {
        sequence = sequence.saturating_add(1);
        let product_event = match event {
            LivePipelineEvent::Partial { job_id, result } => {
                let id = *segment_ids.entry(job_id).or_insert_with(Uuid::new_v4);
                partials.insert(job_id);
                let payload = TranscriptionPartialPayload {
                    segment: segment_from_result(id, LiveSegmentStatus::Partial, result),
                };
                LiveEventEnvelope::new(request_id, sequence, payload)
                    .ok()
                    .map(ProductTranscriptionEvent::Partial)
            }
            LivePipelineEvent::Final { job_id, result } => {
                let id = *segment_ids.entry(job_id).or_insert_with(Uuid::new_v4);
                let replaces_partial_id = partials.remove(&job_id).then_some(id);
                let payload = TranscriptionFinalPayload {
                    segment: segment_from_result(id, LiveSegmentStatus::Final, result),
                    replaces_partial_id,
                };
                LiveEventEnvelope::new(request_id, sequence, payload)
                    .ok()
                    .map(ProductTranscriptionEvent::Final)
            }
            LivePipelineEvent::Gap(gap) => {
                let payload = TranscriptionGapPayload {
                    source: gap.source,
                    start_ms: gap.start_ms,
                    end_ms: gap.end_ms,
                    code: gap.code.to_owned(),
                };
                LiveEventEnvelope::new(request_id, sequence, payload)
                    .ok()
                    .map(ProductTranscriptionEvent::Gap)
            }
        };
        if let Some(event) = product_event {
            emit(event);
        }
    }
}

fn segment_from_result(
    id: Uuid,
    status: LiveSegmentStatus,
    result: super::TranscriptionResult,
) -> LiveTranscriptSegment {
    LiveTranscriptSegment {
        id,
        source: result.source,
        start_ms: result.utterance_start_ms,
        end_ms: result.utterance_end_ms,
        text: result.text,
        status,
        language: result.language,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use crate::{
        audio::{AudioSource, DeviceRole, DeviceSelection, bounded_queue},
        domain::{
            LiveTranscriptionInput, LiveTranscriptionRunState, LiveTranscriptionStatus, RequestId,
        },
        models::ModelService,
        persistence::SettingsService,
        transcription::{TranscriptSegment, TranscriptionResult, scheduler::TranscriptionGap},
    };

    use super::{
        EventSink, LivePipelineEvent, LiveTranscriptionService, ProductState,
        ProductTranscriptionEvent, bridge_events, record_terminal_state, reserve_start,
    };

    fn result(source: AudioSource, start_ms: u64, end_ms: u64, text: &str) -> TranscriptionResult {
        TranscriptionResult {
            source,
            utterance_start_ms: start_ms,
            utterance_end_ms: end_ms,
            language: "en".to_owned(),
            text: text.to_owned(),
            segments: vec![TranscriptSegment {
                start_ms,
                end_ms,
                text: text.to_owned(),
            }],
        }
    }

    #[test]
    fn product_bridge_orders_events_and_preserves_partial_replacement_identity() {
        let request_id = RequestId::new();
        let (sender, receiver) = bounded_queue(4);
        sender
            .send(LivePipelineEvent::Partial {
                job_id: 7,
                result: result(AudioSource::Microphone, 1_000, 1_500, "partial"),
            })
            .unwrap();
        sender
            .send(LivePipelineEvent::Final {
                job_id: 7,
                result: result(AudioSource::Microphone, 1_000, 2_000, "final"),
            })
            .unwrap();
        sender
            .send(LivePipelineEvent::Gap(TranscriptionGap {
                job_id: 8,
                source: AudioSource::SystemOutput,
                start_ms: 2_500,
                end_ms: 3_000,
                code: "transcription_inference_failed",
            }))
            .unwrap();
        drop(sender);

        let received = Arc::new(Mutex::new(Vec::new()));
        let sink_received = Arc::clone(&received);
        let sink: EventSink = Arc::new(move |event| sink_received.lock().unwrap().push(event));
        bridge_events(request_id, receiver, &sink);

        let events = received.lock().unwrap();
        assert_eq!(events.len(), 3);
        let (partial_id, final_id, replacement_id) = match (&events[0], &events[1]) {
            (
                ProductTranscriptionEvent::Partial(partial),
                ProductTranscriptionEvent::Final(final_event),
            ) => {
                assert_eq!(partial.session_sequence, 1);
                assert_eq!(final_event.session_sequence, 2);
                assert_eq!(partial.request_id, request_id);
                (
                    partial.payload.segment.id,
                    final_event.payload.segment.id,
                    final_event.payload.replaces_partial_id,
                )
            }
            _ => panic!("expected partial followed by final"),
        };
        assert_eq!(partial_id, final_id);
        assert_eq!(replacement_id, Some(partial_id));
        match &events[2] {
            ProductTranscriptionEvent::Gap(gap) => {
                assert_eq!(gap.session_sequence, 3);
                assert_eq!(gap.request_id, request_id);
                assert_eq!(gap.payload.source, AudioSource::SystemOutput);
            }
            _ => panic!("expected gap"),
        }
    }

    #[test]
    fn terminal_transition_is_request_scoped_and_valid() {
        let request_id = RequestId::new();
        let other_request_id = RequestId::new();
        let state = Arc::new(Mutex::new(ProductState {
            status: LiveTranscriptionStatus::starting(request_id).unwrap(),
            active: None,
        }));

        record_terminal_state(&state, other_request_id, None);
        assert_eq!(
            state.lock().unwrap().status.state,
            LiveTranscriptionRunState::Starting
        );
        record_terminal_state(&state, request_id, None);
        let status = state.lock().unwrap().status.clone();
        assert_eq!(status.state, LiveTranscriptionRunState::Stopped);
        assert!(status.validate().is_ok());
    }

    #[test]
    fn start_reservation_is_single_flight_and_rejects_reused_requests() {
        let request_id = RequestId::new();
        let state = Arc::new(Mutex::new(ProductState::default()));
        reserve_start(&state, request_id).unwrap();
        let error = reserve_start(&state, RequestId::new()).unwrap_err();
        assert_eq!(error.code, "live_transcription_already_running");

        record_terminal_state(&state, request_id, None);
        let error = reserve_start(&state, request_id).unwrap_err();
        assert_eq!(error.code, "live_transcription_request_conflict");
    }

    #[derive(Default)]
    struct ProductProbeAggregate {
        partials: u64,
        nonempty_partials: u64,
        finals: u64,
        nonempty_finals: u64,
        gaps: u64,
        partial_ids: HashSet<uuid::Uuid>,
        nonempty_partial_ids: HashSet<uuid::Uuid>,
        matching_finals: u64,
    }

    #[test]
    #[ignore = "requires default Windows endpoints, a verified selected model, the external adapter, and fixture playback"]
    fn hardware_probe_product_service_emits_matching_partial_and_final() {
        use std::{env, path::PathBuf};

        let app_data = PathBuf::from(
            env::var_os("KOKOROKOE_P3_015_APP_LOCAL_DATA")
                .expect("KOKOROKOE_P3_015_APP_LOCAL_DATA is required"),
        );
        let documents = PathBuf::from(
            env::var_os("KOKOROKOE_P3_015_DOCUMENTS")
                .expect("KOKOROKOE_P3_015_DOCUMENTS is required"),
        );
        let adapter = PathBuf::from(
            env::var_os("KOKOROKOE_P3_015_ADAPTER").expect("KOKOROKOE_P3_015_ADAPTER is required"),
        );
        let audio = PathBuf::from(
            env::var_os("KOKOROKOE_P3_015_AUDIO").expect("KOKOROKOE_P3_015_AUDIO is required"),
        );
        assert!(audio.is_file(), "the playback fixture must exist");

        let settings = SettingsService::open(app_data.clone(), documents).unwrap();
        let service = LiveTranscriptionService::new(
            ModelService::open(&app_data, settings).unwrap(),
            adapter,
        );
        let aggregate = Arc::new(Mutex::new(ProductProbeAggregate::default()));
        let sink_aggregate = Arc::clone(&aggregate);
        let sink: EventSink = Arc::new(move |event| {
            let mut aggregate = sink_aggregate.lock().unwrap();
            match event {
                ProductTranscriptionEvent::Partial(event) => {
                    aggregate.partials += 1;
                    if !event.payload.segment.text.trim().is_empty() {
                        aggregate.nonempty_partials += 1;
                        aggregate
                            .nonempty_partial_ids
                            .insert(event.payload.segment.id);
                    }
                    aggregate.partial_ids.insert(event.payload.segment.id);
                }
                ProductTranscriptionEvent::Final(event) => {
                    aggregate.finals += 1;
                    aggregate.nonempty_finals +=
                        u64::from(!event.payload.segment.text.trim().is_empty());
                    aggregate.matching_finals += u64::from(
                        !event.payload.segment.text.trim().is_empty()
                            && aggregate
                                .nonempty_partial_ids
                                .contains(&event.payload.segment.id)
                            && event.payload.replaces_partial_id == Some(event.payload.segment.id)
                            && aggregate.partial_ids.contains(&event.payload.segment.id),
                    );
                }
                ProductTranscriptionEvent::Gap(_) => aggregate.gaps += 1,
            }
        });
        let request_id = RequestId::new();
        let input = LiveTranscriptionInput {
            acknowledged_capture_consent: true,
            microphone_selection: DeviceSelection::Default {
                role: DeviceRole::Communications,
            },
            system_output_selection: DeviceSelection::Default {
                role: DeviceRole::Console,
            },
        };
        let running = service
            .start(input.clone(), request_id, Arc::clone(&sink))
            .unwrap();
        assert_eq!(running.state, LiveTranscriptionRunState::Running);
        let conflict = service.start(input, RequestId::new(), sink).unwrap_err();
        assert_eq!(conflict.code, "live_transcription_already_running");

        std::thread::sleep(Duration::from_secs(24));
        let stopped = service.stop(request_id).unwrap();
        assert_eq!(stopped.state, LiveTranscriptionRunState::Stopped);
        assert_eq!(service.stop(request_id).unwrap(), stopped);

        let aggregate = aggregate.lock().unwrap();
        assert!(aggregate.partials > 0);
        assert!(aggregate.nonempty_partials > 0);
        assert!(aggregate.finals > 0);
        assert!(aggregate.nonempty_finals > 0);
        assert!(aggregate.matching_finals > 0);
        eprintln!(
            "P3-015 aggregate: partials={}, finals={}, matching_finals={}, gaps={}",
            aggregate.partials, aggregate.finals, aggregate.matching_finals, aggregate.gaps
        );
    }
}
