use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Serialize;
use uuid::Uuid;

use crate::{
    domain::{
        AppError, LiveTranscriptionInput, ProjectId, RequestId, Session, SessionId, SessionState,
        now_rfc3339,
    },
    transcription::{EventSink, LiveTranscriptionService, ProductTranscriptionEvent},
};

use super::{
    FinalizedTranscriptSegment, JournalAppend, JournalMutation, LifecycleChange,
    PersistedSessionWriter, ProjectCatalog, SessionCatalog, SessionJournal, SessionLocator,
    SessionStore, SettingsService,
};

const PERSISTENCE_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PersistenceState {
    Clean,
    Deferred,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PersistenceStatus {
    pub(crate) schema_version: u8,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) journal_sequence: u64,
    pub(crate) snapshot_sequence: u64,
    pub(crate) state: PersistenceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) code: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub(crate) enum PersistedSessionEvent {
    Partial {
        project_id: ProjectId,
        session_id: SessionId,
        event: crate::domain::LiveEventEnvelope<crate::domain::TranscriptionPartialPayload>,
    },
    Final {
        project_id: ProjectId,
        session_id: SessionId,
        event: crate::domain::LiveEventEnvelope<crate::domain::TranscriptionFinalPayload>,
    },
    Gap {
        project_id: ProjectId,
        session_id: SessionId,
        event: crate::domain::LiveEventEnvelope<crate::domain::TranscriptionGapPayload>,
    },
    Persistence(PersistenceStatus),
}

pub(crate) type PersistedEventSink = Arc<dyn Fn(PersistedSessionEvent) + Send + Sync>;

trait SessionTranscriptionRuntime: Send + Sync {
    fn start(
        &self,
        input: LiveTranscriptionInput,
        request_id: RequestId,
        model_id: &str,
        emit: EventSink,
    ) -> Result<(), AppError>;

    fn stop(&self, request_id: RequestId) -> Result<(), AppError>;
}

impl SessionTranscriptionRuntime for LiveTranscriptionService {
    fn start(
        &self,
        input: LiveTranscriptionInput,
        request_id: RequestId,
        model_id: &str,
        emit: EventSink,
    ) -> Result<(), AppError> {
        LiveTranscriptionService::start_with_model(self, input, request_id, Some(model_id), emit)
            .map(|_| ())
    }

    fn stop(&self, request_id: RequestId) -> Result<(), AppError> {
        LiveTranscriptionService::stop(self, request_id).map(|_| ())
    }
}

struct ProjectionPump {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl ProjectionPump {
    fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for ProjectionPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct ActiveSessionRun {
    project_id: ProjectId,
    session_id: SessionId,
    request_id: RequestId,
    writer: Arc<Mutex<PersistedSessionWriter>>,
    epoch: Instant,
    journal_sequence: Arc<AtomicU64>,
    pump: Option<ProjectionPump>,
}

#[derive(Clone)]
pub(crate) struct PersistedSessionLifecycleService {
    settings: SettingsService,
    app_data_directory: PathBuf,
    runtime: Arc<dyn SessionTranscriptionRuntime>,
    operation_lock: Arc<Mutex<()>>,
    active: Arc<Mutex<Option<ActiveSessionRun>>>,
}

impl PersistedSessionLifecycleService {
    pub(crate) fn new(
        settings: SettingsService,
        app_data_directory: PathBuf,
        runtime: LiveTranscriptionService,
    ) -> Self {
        Self::with_runtime(settings, app_data_directory, Arc::new(runtime))
    }

    fn with_runtime(
        settings: SettingsService,
        app_data_directory: PathBuf,
        runtime: Arc<dyn SessionTranscriptionRuntime>,
    ) -> Self {
        Self {
            settings,
            app_data_directory,
            runtime,
            operation_lock: Arc::new(Mutex::new(())),
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn recover_interrupted_sessions(&self) -> Result<u32, AppError> {
        let _operation = self.lock_operation()?;
        self.settings.with_workspace_operation(|workspace| {
            let sessions = SessionStore::open(workspace)
                .map_err(|error| AppError::session_error(error.code))?;
            let discovered = sessions
                .discover_sessions()
                .map_err(|error| AppError::session_error(error.code))?;
            let mut recovered = 0_u32;
            for candidate in discovered.sessions {
                let current = candidate.snapshot;
                let locator =
                    SessionLocator::from_discovered(&current.session, candidate.project_folder);
                let journal = SessionJournal::open(workspace)
                    .map_err(|error| AppError::session_error(error.code))?;
                let replay = journal
                    .replay(&locator)
                    .map_err(|error| AppError::session_error(error.code))?;
                if replay.state == SessionState::Idle
                    || (replay.state == current.session.state
                        && matches!(
                            replay.state,
                            SessionState::Paused | SessionState::Completed | SessionState::Failed
                        ))
                {
                    continue;
                }
                let target = match replay.state {
                    SessionState::Transcribing => SessionState::Paused,
                    SessionState::Preparing | SessionState::Capturing | SessionState::Stopping => {
                        SessionState::Failed
                    }
                    SessionState::Paused | SessionState::Completed | SessionState::Failed => {
                        replay.state
                    }
                    SessionState::Idle | SessionState::ProcessingSummary => continue,
                };
                let timestamp = now_rfc3339()?;
                let mut writer = PersistedSessionWriter::open(
                    workspace,
                    &self.app_data_directory,
                    locator.clone(),
                    0,
                )
                .map_err(|error| AppError::session_error(error.code))?;
                if let Some(change) = recovery_change(replay.state, target) {
                    writer
                        .append(lifecycle_append(change.0, change.1, &timestamp), 0)
                        .map_err(|error| AppError::session_error(error.code))?;
                }
                let recovered_session = current
                    .session
                    .apply_runtime_state(target, timestamp, Some("recovery_required"))
                    .map_err(AppError::session_error)?;
                sessions
                    .update_session(
                        &locator,
                        &recovered_session,
                        current.session.revision,
                        current.fingerprint,
                    )
                    .map_err(|error| AppError::session_error(error.code))?;
                recovered = recovered.saturating_add(1);
            }
            if recovered > 0 {
                SessionCatalog::open(workspace, self.app_data_directory.clone())
                    .and_then(|catalog| catalog.rebuild_index().map(|_| ()))
                    .map_err(|error| AppError::session_error(error.code))?;
            }
            Ok(recovered)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
        request_id: RequestId,
        acknowledged_capture_consent: bool,
        emit: PersistedEventSink,
    ) -> Result<Session, AppError> {
        self.start_or_resume(
            project_id,
            session_id,
            expected_revision,
            request_id,
            acknowledged_capture_consent,
            SessionState::Idle,
            emit,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resume_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
        request_id: RequestId,
        acknowledged_capture_consent: bool,
        emit: PersistedEventSink,
    ) -> Result<Session, AppError> {
        self.start_or_resume(
            project_id,
            session_id,
            expected_revision,
            request_id,
            acknowledged_capture_consent,
            SessionState::Paused,
            emit,
        )
    }

    pub(crate) fn pause_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
    ) -> Result<Session, AppError> {
        let _operation = self.lock_operation()?;
        self.validate_boundary_session(
            project_id,
            session_id,
            expected_revision,
            SessionState::Transcribing,
        )?;
        let mut active = self.take_active(project_id, session_id)?;
        active.pump.take().expect("active run owns its pump").stop();
        if let Err(error) = self.runtime.stop(active.request_id) {
            return self.finish_failed(active, expected_revision, error);
        }
        self.finish_boundary(active, expected_revision, SessionState::Paused)
    }

    pub(crate) fn stop_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
    ) -> Result<Session, AppError> {
        let _operation = self.lock_operation()?;
        let has_active = {
            let active = self
                .active
                .lock()
                .map_err(|_| AppError::session_error("session_lifecycle_state_unavailable"))?;
            match active.as_ref() {
                Some(active)
                    if active.project_id == project_id && active.session_id == session_id =>
                {
                    true
                }
                Some(_) => {
                    return Err(AppError::session_error("session_not_running"));
                }
                None => false,
            }
        };
        if has_active {
            self.validate_boundary_session(
                project_id,
                session_id,
                expected_revision,
                SessionState::Transcribing,
            )?;
            let mut active = self.take_active(project_id, session_id)?;
            active.pump.take().expect("active run owns its pump").stop();
            if let Err(error) = self.runtime.stop(active.request_id) {
                return self.finish_failed(active, expected_revision, error);
            }
            self.finish_boundary(active, expected_revision, SessionState::Completed)
        } else {
            self.finish_paused(project_id, session_id, expected_revision)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn start_or_resume(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
        request_id: RequestId,
        acknowledged_capture_consent: bool,
        expected_state: SessionState,
        emit: PersistedEventSink,
    ) -> Result<Session, AppError> {
        if !acknowledged_capture_consent {
            return Err(AppError::live_transcription_error(
                "live_transcription_consent_required",
            ));
        }
        let _operation = self.lock_operation()?;
        if self
            .active
            .lock()
            .map_err(|_| AppError::session_error("session_lifecycle_state_unavailable"))?
            .is_some()
        {
            return Err(AppError::session_error("session_already_running"));
        }
        self.settings.with_workspace_operation(|workspace| {
            let projects = ProjectCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::session_error(error.code))?;
            let sessions = SessionCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::session_error(error.code))?;
            let project = projects
                .read_project(project_id)
                .map_err(|error| AppError::session_error(error.code))?
                .project;
            let current = sessions
                .read_session(project_id, session_id)
                .map_err(|error| AppError::session_error(error.code))?;
            if current.session.revision != expected_revision {
                return Err(AppError::session_error("session_revision_conflict"));
            }
            if current.session.state != expected_state {
                return Err(AppError::session_error("session_lifecycle_invalid"));
            }
            let locator = SessionLocator::from_records(&project, &current.session)
                .map_err(|error| AppError::session_error(error.code))?;
            let epoch = Instant::now();
            let writer = Arc::new(Mutex::new(
                PersistedSessionWriter::open(workspace, &self.app_data_directory, locator, 0)
                    .map_err(|error| AppError::session_error(error.code))?,
            ));
            let journal_sequence = Arc::new(AtomicU64::new(0));
            append_boundary(
                &writer,
                expected_state,
                SessionState::Preparing,
                epoch,
                &journal_sequence,
                &emit,
                project_id,
                session_id,
            )?;
            let runtime_emit = persisted_runtime_sink(
                project_id,
                session_id,
                Arc::clone(&writer),
                epoch,
                Arc::clone(&journal_sequence),
                Arc::clone(&emit),
            );
            let input = LiveTranscriptionInput {
                acknowledged_capture_consent: true,
                microphone_selection: current.session.microphone.selection.clone(),
                system_output_selection: current.session.system_output.selection.clone(),
            };
            if let Err(error) = self.runtime.start(
                input,
                request_id,
                &current.session.transcription_model_id,
                runtime_emit,
            ) {
                let timestamp = now_rfc3339()?;
                let _ = append_boundary_at(
                    &writer,
                    SessionState::Preparing,
                    SessionState::Failed,
                    epoch,
                    &timestamp,
                    &journal_sequence,
                    &emit,
                    project_id,
                    session_id,
                );
                let failed = current
                    .session
                    .apply_runtime_state(SessionState::Failed, timestamp, Some(&error.code))
                    .map_err(AppError::session_error)?;
                let _ = sessions.update_session(
                    &project,
                    &failed,
                    current.session.revision,
                    current.fingerprint,
                );
                return Err(error);
            }
            let mut durable_state = SessionState::Preparing;
            let post_start: Result<(Session, ProjectionPump), AppError> = (|| {
                append_boundary(
                    &writer,
                    durable_state,
                    SessionState::Capturing,
                    epoch,
                    &journal_sequence,
                    &emit,
                    project_id,
                    session_id,
                )?;
                durable_state = SessionState::Capturing;
                append_boundary(
                    &writer,
                    durable_state,
                    SessionState::Transcribing,
                    epoch,
                    &journal_sequence,
                    &emit,
                    project_id,
                    session_id,
                )?;
                durable_state = SessionState::Transcribing;
                let timestamp = now_rfc3339()?;
                let running = current
                    .session
                    .apply_runtime_state(SessionState::Transcribing, timestamp, None)
                    .map_err(AppError::session_error)?;
                let running = sessions
                    .update_session(
                        &project,
                        &running,
                        current.session.revision,
                        current.fingerprint,
                    )
                    .map_err(|error| AppError::session_error(error.code))?;
                let pump = spawn_projection_pump(
                    project_id,
                    session_id,
                    Arc::clone(&writer),
                    epoch,
                    Arc::clone(&journal_sequence),
                    Arc::clone(&emit),
                )?;
                Ok((running, pump))
            })();
            let (running, pump) = match post_start {
                Ok(result) => result,
                Err(error) => {
                    let _ = self.runtime.stop(request_id);
                    if let Ok(timestamp) = now_rfc3339() {
                        let _ = append_boundary_at(
                            &writer,
                            durable_state,
                            SessionState::Failed,
                            epoch,
                            &timestamp,
                            &journal_sequence,
                            &emit,
                            project_id,
                            session_id,
                        );
                        if let Ok(failed) = current.session.apply_runtime_state(
                            SessionState::Failed,
                            timestamp,
                            Some(&error.code),
                        ) {
                            let _ = sessions.update_session(
                                &project,
                                &failed,
                                current.session.revision,
                                current.fingerprint,
                            );
                        }
                    }
                    return Err(error);
                }
            };
            self.active
                .lock()
                .map_err(|_| AppError::session_error("session_lifecycle_state_unavailable"))?
                .replace(ActiveSessionRun {
                    project_id,
                    session_id,
                    request_id,
                    writer,
                    epoch,
                    journal_sequence,
                    pump: Some(pump),
                });
            Ok(running)
        })
    }

    fn finish_boundary(
        &self,
        active: ActiveSessionRun,
        expected_revision: u64,
        target: SessionState,
    ) -> Result<Session, AppError> {
        self.settings.with_workspace_operation(|workspace| {
            let (project, current, sessions) = read_lifecycle_session(
                workspace,
                &self.app_data_directory,
                active.project_id,
                active.session_id,
                expected_revision,
            )?;
            if current.session.state != SessionState::Transcribing {
                return Err(AppError::session_error("session_lifecycle_invalid"));
            }
            if target == SessionState::Completed {
                append_boundary(
                    &active.writer,
                    SessionState::Transcribing,
                    SessionState::Stopping,
                    active.epoch,
                    &active.journal_sequence,
                    &noop_sink(),
                    active.project_id,
                    active.session_id,
                )?;
                append_boundary(
                    &active.writer,
                    SessionState::Stopping,
                    SessionState::Completed,
                    active.epoch,
                    &active.journal_sequence,
                    &noop_sink(),
                    active.project_id,
                    active.session_id,
                )?;
            } else {
                append_boundary(
                    &active.writer,
                    SessionState::Transcribing,
                    target,
                    active.epoch,
                    &active.journal_sequence,
                    &noop_sink(),
                    active.project_id,
                    active.session_id,
                )?;
            }
            let updated = current
                .session
                .apply_runtime_state(target, now_rfc3339()?, None)
                .map_err(AppError::session_error)?;
            sessions
                .update_session(
                    &project,
                    &updated,
                    current.session.revision,
                    current.fingerprint,
                )
                .map_err(|error| AppError::session_error(error.code))
        })
    }

    fn finish_paused(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
    ) -> Result<Session, AppError> {
        self.settings.with_workspace_operation(|workspace| {
            let (project, current, sessions) = read_lifecycle_session(
                workspace,
                &self.app_data_directory,
                project_id,
                session_id,
                expected_revision,
            )?;
            if current.session.state != SessionState::Paused {
                return Err(AppError::session_error("session_not_running"));
            }
            let locator = SessionLocator::from_records(&project, &current.session)
                .map_err(|error| AppError::session_error(error.code))?;
            let mut writer =
                PersistedSessionWriter::open(workspace, &self.app_data_directory, locator, 0)
                    .map_err(|error| AppError::session_error(error.code))?;
            let timestamp = now_rfc3339()?;
            writer
                .append(
                    lifecycle_append(SessionState::Paused, SessionState::Stopping, &timestamp),
                    0,
                )
                .and_then(|_| {
                    writer.append(
                        lifecycle_append(
                            SessionState::Stopping,
                            SessionState::Completed,
                            &timestamp,
                        ),
                        0,
                    )
                })
                .map_err(|error| AppError::session_error(error.code))?;
            let updated = current
                .session
                .apply_runtime_state(SessionState::Completed, timestamp, None)
                .map_err(AppError::session_error)?;
            sessions
                .update_session(
                    &project,
                    &updated,
                    current.session.revision,
                    current.fingerprint,
                )
                .map_err(|error| AppError::session_error(error.code))
        })
    }

    fn finish_failed(
        &self,
        active: ActiveSessionRun,
        expected_revision: u64,
        runtime_error: AppError,
    ) -> Result<Session, AppError> {
        let result = self.settings.with_workspace_operation(|workspace| {
            let (project, current, sessions) = read_lifecycle_session(
                workspace,
                &self.app_data_directory,
                active.project_id,
                active.session_id,
                expected_revision,
            )?;
            append_boundary(
                &active.writer,
                SessionState::Transcribing,
                SessionState::Failed,
                active.epoch,
                &active.journal_sequence,
                &noop_sink(),
                active.project_id,
                active.session_id,
            )?;
            let failed = current
                .session
                .apply_runtime_state(
                    SessionState::Failed,
                    now_rfc3339()?,
                    Some(&runtime_error.code),
                )
                .map_err(AppError::session_error)?;
            sessions
                .update_session(
                    &project,
                    &failed,
                    current.session.revision,
                    current.fingerprint,
                )
                .map_err(|error| AppError::session_error(error.code))
        });
        result.and(Err(runtime_error))
    }

    fn take_active(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<ActiveSessionRun, AppError> {
        let mut state = self
            .active
            .lock()
            .map_err(|_| AppError::session_error("session_lifecycle_state_unavailable"))?;
        let active = state
            .take()
            .ok_or_else(|| AppError::session_error("session_not_running"))?;
        if active.project_id != project_id || active.session_id != session_id {
            state.replace(active);
            return Err(AppError::session_error("session_not_running"));
        }
        Ok(active)
    }

    fn validate_boundary_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
        expected_state: SessionState,
    ) -> Result<(), AppError> {
        self.settings.with_workspace_operation(|workspace| {
            let (_, current, _) = read_lifecycle_session(
                workspace,
                &self.app_data_directory,
                project_id,
                session_id,
                expected_revision,
            )?;
            if current.session.state != expected_state {
                return Err(AppError::session_error("session_lifecycle_invalid"));
            }
            Ok(())
        })
    }

    fn lock_operation(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.operation_lock
            .lock()
            .map_err(|_| AppError::session_worker_failed())
    }
}

fn read_lifecycle_session(
    workspace: &std::path::Path,
    app_data_directory: &std::path::Path,
    project_id: ProjectId,
    session_id: SessionId,
    expected_revision: u64,
) -> Result<
    (
        crate::domain::Project,
        super::SessionSnapshot,
        SessionCatalog,
    ),
    AppError,
> {
    let projects = ProjectCatalog::open(workspace, app_data_directory.to_path_buf())
        .map_err(|error| AppError::session_error(error.code))?;
    let sessions = SessionCatalog::open(workspace, app_data_directory.to_path_buf())
        .map_err(|error| AppError::session_error(error.code))?;
    let project = projects
        .read_project(project_id)
        .map_err(|error| AppError::session_error(error.code))?
        .project;
    let current = sessions
        .read_session(project_id, session_id)
        .map_err(|error| AppError::session_error(error.code))?;
    if current.session.revision != expected_revision {
        return Err(AppError::session_error("session_revision_conflict"));
    }
    Ok((project, current, sessions))
}

fn recovery_change(
    current: SessionState,
    target: SessionState,
) -> Option<(SessionState, SessionState)> {
    matches!(
        (current, target),
        (SessionState::Transcribing, SessionState::Paused)
            | (SessionState::Preparing, SessionState::Failed)
            | (SessionState::Capturing, SessionState::Failed)
            | (SessionState::Stopping, SessionState::Failed)
    )
    .then_some((current, target))
}

#[allow(clippy::too_many_arguments)]
fn append_boundary(
    writer: &Arc<Mutex<PersistedSessionWriter>>,
    previous: SessionState,
    current: SessionState,
    epoch: Instant,
    journal_sequence: &Arc<AtomicU64>,
    emit: &PersistedEventSink,
    project_id: ProjectId,
    session_id: SessionId,
) -> Result<(), AppError> {
    let timestamp = now_rfc3339()?;
    append_boundary_at(
        writer,
        previous,
        current,
        epoch,
        &timestamp,
        journal_sequence,
        emit,
        project_id,
        session_id,
    )
}

#[allow(clippy::too_many_arguments)]
fn append_boundary_at(
    writer: &Arc<Mutex<PersistedSessionWriter>>,
    previous: SessionState,
    current: SessionState,
    epoch: Instant,
    timestamp: &str,
    journal_sequence: &Arc<AtomicU64>,
    emit: &PersistedEventSink,
    project_id: ProjectId,
    session_id: SessionId,
) -> Result<(), AppError> {
    let mut writer = writer
        .lock()
        .map_err(|_| AppError::session_error("session_writer_state_unavailable"))?;
    let receipt = writer
        .append(
            lifecycle_append(previous, current, timestamp),
            elapsed_ms(epoch),
        )
        .map_err(|error| AppError::session_error(error.code))?;
    journal_sequence.store(receipt.sequence, Ordering::Release);
    emit(PersistedSessionEvent::Persistence(persistence_status(
        project_id,
        session_id,
        receipt.sequence,
        &receipt.projection,
    )));
    Ok(())
}

fn lifecycle_append(
    previous: SessionState,
    current: SessionState,
    timestamp: &str,
) -> JournalAppend {
    JournalAppend {
        event_id: Uuid::new_v4(),
        recorded_at: timestamp.to_owned(),
        mutation: JournalMutation::LifecycleChange(LifecycleChange {
            previous,
            current,
            occurred_at: timestamp.to_owned(),
        }),
    }
}

fn persisted_runtime_sink(
    project_id: ProjectId,
    session_id: SessionId,
    writer: Arc<Mutex<PersistedSessionWriter>>,
    epoch: Instant,
    journal_sequence: Arc<AtomicU64>,
    emit: PersistedEventSink,
) -> EventSink {
    Arc::new(move |event| match event {
        ProductTranscriptionEvent::Partial(event) => emit(PersistedSessionEvent::Partial {
            project_id,
            session_id,
            event,
        }),
        ProductTranscriptionEvent::Gap(event) => emit(PersistedSessionEvent::Gap {
            project_id,
            session_id,
            event,
        }),
        ProductTranscriptionEvent::Final(event) => {
            let timestamp = match now_rfc3339() {
                Ok(timestamp) => timestamp,
                Err(_) => {
                    emit(PersistedSessionEvent::Persistence(PersistenceStatus {
                        schema_version: 1,
                        project_id,
                        session_id,
                        journal_sequence: journal_sequence.load(Ordering::Acquire),
                        snapshot_sequence: 0,
                        state: PersistenceState::Failed,
                        code: Some("session_writer_clock_unavailable"),
                    }));
                    return;
                }
            };
            let segment = &event.payload.segment;
            let append = JournalAppend {
                event_id: segment.id,
                recorded_at: timestamp,
                mutation: JournalMutation::FinalizedTranscriptSegment(FinalizedTranscriptSegment {
                    id: segment.id,
                    source: segment.source,
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    text: segment.text.clone(),
                    language: segment.language.clone(),
                }),
            };
            let result = writer
                .lock()
                .map_err(|_| "session_writer_state_unavailable")
                .and_then(|mut writer| {
                    writer
                        .append(append, elapsed_ms(epoch))
                        .map_err(|error| error.code)
                });
            match result {
                Ok(receipt) => {
                    journal_sequence.store(receipt.sequence, Ordering::Release);
                    emit(PersistedSessionEvent::Persistence(persistence_status(
                        project_id,
                        session_id,
                        receipt.sequence,
                        &receipt.projection,
                    )));
                    emit(PersistedSessionEvent::Final {
                        project_id,
                        session_id,
                        event,
                    });
                }
                Err(code) => emit(PersistedSessionEvent::Persistence(PersistenceStatus {
                    schema_version: 1,
                    project_id,
                    session_id,
                    journal_sequence: journal_sequence.load(Ordering::Acquire),
                    snapshot_sequence: 0,
                    state: PersistenceState::Failed,
                    code: Some(code),
                })),
            }
        }
    })
}

fn spawn_projection_pump(
    project_id: ProjectId,
    session_id: SessionId,
    writer: Arc<Mutex<PersistedSessionWriter>>,
    epoch: Instant,
    journal_sequence: Arc<AtomicU64>,
    emit: PersistedEventSink,
) -> Result<ProjectionPump, AppError> {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let join = thread::Builder::new()
        .name("persisted-session-projection".to_owned())
        .spawn(move || {
            let mut last = None;
            while !thread_stop.load(Ordering::Acquire) {
                thread::sleep(PERSISTENCE_POLL_INTERVAL);
                let projection = writer
                    .lock()
                    .map_err(|_| "session_writer_state_unavailable")
                    .and_then(|mut writer| {
                        writer.poll(elapsed_ms(epoch)).map_err(|error| error.code)
                    });
                match projection {
                    Ok(projection) if last.as_ref() != Some(&projection) => {
                        emit(PersistedSessionEvent::Persistence(persistence_status(
                            project_id,
                            session_id,
                            journal_sequence.load(Ordering::Acquire),
                            &projection,
                        )));
                        last = Some(projection);
                    }
                    Ok(_) => {}
                    Err(code) => {
                        emit(PersistedSessionEvent::Persistence(PersistenceStatus {
                            schema_version: 1,
                            project_id,
                            session_id,
                            journal_sequence: journal_sequence.load(Ordering::Acquire),
                            snapshot_sequence: 0,
                            state: PersistenceState::Failed,
                            code: Some(code),
                        }));
                        break;
                    }
                }
            }
        })
        .map_err(|_| AppError::session_error("session_writer_worker_failed"))?;
    Ok(ProjectionPump {
        stop,
        join: Some(join),
    })
}

fn persistence_status(
    project_id: ProjectId,
    session_id: SessionId,
    journal_sequence: u64,
    projection: &super::SessionWriterProjection,
) -> PersistenceStatus {
    use super::SessionWriterProjection::{Current, Deferred, IndexPending, SnapshotPending};
    let (snapshot_sequence, state, code) = match projection {
        Current {
            checkpoint_sequence,
            ..
        } => (*checkpoint_sequence, PersistenceState::Clean, None),
        Deferred {
            checkpoint_sequence,
            ..
        } => (*checkpoint_sequence, PersistenceState::Deferred, None),
        SnapshotPending {
            checkpoint_sequence,
            code,
            ..
        } => (
            *checkpoint_sequence,
            if *code == "transcript_snapshot_conflict" {
                PersistenceState::Conflict
            } else {
                PersistenceState::Failed
            },
            Some(*code),
        ),
        IndexPending {
            checkpoint_sequence,
            code,
        } => (*checkpoint_sequence, PersistenceState::Failed, Some(*code)),
    };
    PersistenceStatus {
        schema_version: 1,
        project_id,
        session_id,
        journal_sequence,
        snapshot_sequence,
        state,
        code,
    }
}

fn elapsed_ms(epoch: Instant) -> u64 {
    u64::try_from(epoch.elapsed().as_millis()).unwrap_or(9_007_199_254_740_991)
}

fn noop_sink() -> PersistedEventSink {
    Arc::new(|_| {})
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::{
        audio::AudioSource,
        domain::{
            CreateProjectInput, CreateSessionSnapshotInput, LiveEventEnvelope, LiveSegmentStatus,
            LiveTranscriptSegment, TranscriptionFinalPayload,
        },
        persistence::{ProjectService, SessionService, TranscriptStore},
    };

    use super::*;

    #[derive(Default)]
    struct FakeRuntime {
        starts: AtomicUsize,
        stops: AtomicUsize,
        model_ids: Mutex<Vec<String>>,
    }

    impl SessionTranscriptionRuntime for FakeRuntime {
        fn start(
            &self,
            _input: LiveTranscriptionInput,
            request_id: RequestId,
            model_id: &str,
            emit: EventSink,
        ) -> Result<(), AppError> {
            self.model_ids.lock().unwrap().push(model_id.to_owned());
            let index = self.starts.fetch_add(1, Ordering::SeqCst) as u64;
            let segment_id = Uuid::from_u128(100 + u128::from(index));
            let event = LiveEventEnvelope::new(
                request_id,
                1,
                TranscriptionFinalPayload {
                    segment: LiveTranscriptSegment {
                        id: segment_id,
                        source: AudioSource::Microphone,
                        start_ms: index * 1_000,
                        end_ms: index * 1_000 + 500,
                        text: format!("persisted final {index}"),
                        status: LiveSegmentStatus::Final,
                        language: "en-GB".to_owned(),
                    },
                    replaces_partial_id: None,
                },
            )
            .unwrap();
            emit(ProductTranscriptionEvent::Final(event));
            Ok(())
        }

        fn stop(&self, _request_id: RequestId) -> Result<(), AppError> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct Fixture {
        settings: SettingsService,
        app_data: tempfile::TempDir,
        _documents: tempfile::TempDir,
        project: crate::domain::Project,
        session: Session,
    }

    impl Fixture {
        fn new() -> Self {
            let app_data = tempfile::tempdir().unwrap();
            let documents = tempfile::tempdir().unwrap();
            let settings = SettingsService::open(
                app_data.path().to_path_buf(),
                documents.path().to_path_buf(),
            )
            .unwrap();
            let projects = ProjectService::new(settings.clone(), app_data.path().to_path_buf());
            let sessions = SessionService::new(settings.clone(), app_data.path().to_path_buf());
            let fixture: serde_json::Value = serde_json::from_str(include_str!(
                "../../../fixtures/contracts/project-management-v1.json"
            ))
            .unwrap();
            let input: CreateProjectInput =
                serde_json::from_value(fixture["createInput"].clone()).unwrap();
            let project = projects.create_project(input).unwrap();
            let fixture: serde_json::Value = serde_json::from_str(include_str!(
                "../../../fixtures/contracts/session-management-v1.json"
            ))
            .unwrap();
            let input: CreateSessionSnapshotInput =
                serde_json::from_value(fixture["createRequest"]["value"].clone()).unwrap();
            let session = sessions.create_session(project.id, input).unwrap();
            Self {
                settings,
                app_data,
                _documents: documents,
                project,
                session,
            }
        }

        fn service(
            &self,
            runtime: Arc<dyn SessionTranscriptionRuntime>,
        ) -> PersistedSessionLifecycleService {
            PersistedSessionLifecycleService::with_runtime(
                self.settings.clone(),
                self.app_data.path().to_path_buf(),
                runtime,
            )
        }

        fn current(&self) -> Session {
            SessionService::new(self.settings.clone(), self.app_data.path().to_path_buf())
                .get_session(self.project.id, self.session.id)
                .unwrap()
        }
    }

    fn request_id(value: &str) -> RequestId {
        serde_json::from_value(serde_json::json!(value)).unwrap()
    }

    #[test]
    fn start_pause_resume_stop_persists_finals_and_metadata_in_order() {
        let fixture = Fixture::new();
        let runtime = Arc::new(FakeRuntime::default());
        let service = fixture.service(runtime.clone());
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink: PersistedEventSink = {
            let events = Arc::clone(&events);
            Arc::new(move |event| events.lock().unwrap().push(event))
        };

        let running = service
            .start_session(
                fixture.project.id,
                fixture.session.id,
                fixture.session.revision,
                request_id("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
                true,
                Arc::clone(&sink),
            )
            .unwrap();
        assert_eq!(running.state, SessionState::Transcribing);
        assert_eq!(running.revision, 2);
        let stale_pause = service
            .pause_session(fixture.project.id, fixture.session.id, running.revision - 1)
            .unwrap_err();
        assert_eq!(stale_pause.code, "session_revision_conflict");
        assert_eq!(runtime.stops.load(Ordering::SeqCst), 0);
        let paused = service
            .pause_session(fixture.project.id, fixture.session.id, running.revision)
            .unwrap();
        assert_eq!(paused.state, SessionState::Paused);
        let resumed = service
            .resume_session(
                fixture.project.id,
                fixture.session.id,
                paused.revision,
                request_id("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
                true,
                Arc::clone(&sink),
            )
            .unwrap();
        assert_eq!(resumed.state, SessionState::Transcribing);
        let completed = service
            .stop_session(fixture.project.id, fixture.session.id, resumed.revision)
            .unwrap();
        assert_eq!(completed.state, SessionState::Completed);
        assert!(completed.ended_at.is_some());
        assert_eq!(runtime.starts.load(Ordering::SeqCst), 2);
        assert_eq!(runtime.stops.load(Ordering::SeqCst), 2);
        assert_eq!(
            runtime.model_ids.lock().unwrap().as_slice(),
            ["whisper-tiny-multilingual", "whisper-tiny-multilingual"]
        );
        assert_eq!(fixture.current(), completed);

        fixture
            .settings
            .with_workspace_operation(|workspace| {
                let locator = SessionLocator::from_records(&fixture.project, &completed).unwrap();
                let replay = SessionJournal::open(workspace)
                    .unwrap()
                    .replay(&locator)
                    .unwrap();
                assert_eq!(replay.state, SessionState::Completed);
                assert_eq!(replay.finalized_segments.len(), 2);
                let transcript = TranscriptStore::open(workspace)
                    .unwrap()
                    .read_transcript(&locator)
                    .unwrap();
                assert_eq!(transcript.segment_count, 2);
                Ok(())
            })
            .unwrap();
        let events = events.lock().unwrap();
        for final_index in events.iter().enumerate().filter_map(|(index, event)| {
            matches!(event, PersistedSessionEvent::Final { .. }).then_some(index)
        }) {
            assert!(final_index > 0);
            assert!(matches!(
                &events[final_index - 1],
                PersistedSessionEvent::Persistence(status) if status.journal_sequence > 0
            ));
        }
    }

    #[test]
    fn rust_accepts_the_shared_lifecycle_requests_and_events() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/session-lifecycle-v1.json"
        ))
        .unwrap();
        for request in ["startRequest", "resumeRequest"] {
            let value = &fixture[request];
            let _: ProjectId = serde_json::from_value(value["projectId"].clone()).unwrap();
            let _: SessionId = serde_json::from_value(value["sessionId"].clone()).unwrap();
            let _: RequestId = serde_json::from_value(value["requestId"].clone()).unwrap();
            assert_eq!(value["acknowledgedCaptureConsent"], true);
        }
        let _: crate::domain::LiveEventEnvelope<crate::domain::TranscriptionPartialPayload> =
            serde_json::from_value(fixture["partialEvent"]["event"].clone()).unwrap();
        let _: crate::domain::LiveEventEnvelope<crate::domain::TranscriptionFinalPayload> =
            serde_json::from_value(fixture["finalEvent"]["event"].clone()).unwrap();
        let _: crate::domain::LiveEventEnvelope<crate::domain::TranscriptionGapPayload> =
            serde_json::from_value(fixture["gapEvent"]["event"].clone()).unwrap();
        assert_eq!(fixture["persistenceStatus"]["schemaVersion"], 1);
        assert!(
            fixture["persistenceStatus"]["snapshotSequence"].as_u64()
                <= fixture["persistenceStatus"]["journalSequence"].as_u64()
        );
    }

    #[test]
    fn consent_and_revision_are_rejected_before_runtime_start() {
        let fixture = Fixture::new();
        let runtime = Arc::new(FakeRuntime::default());
        let service = fixture.service(runtime.clone());
        let error = service
            .start_session(
                fixture.project.id,
                fixture.session.id,
                fixture.session.revision,
                request_id("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
                false,
                Arc::new(|_| {}),
            )
            .unwrap_err();
        assert_eq!(error.code, "live_transcription_consent_required");
        let error = service
            .start_session(
                fixture.project.id,
                fixture.session.id,
                fixture.session.revision + 1,
                request_id("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
                true,
                Arc::new(|_| {}),
            )
            .unwrap_err();
        assert_eq!(error.code, "session_revision_conflict");
        assert_eq!(runtime.starts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn startup_recovery_reconciles_a_journal_ahead_of_idle_markdown() {
        let fixture = Fixture::new();
        fixture
            .settings
            .with_workspace_operation(|workspace| {
                let locator =
                    SessionLocator::from_records(&fixture.project, &fixture.session).unwrap();
                let mut writer =
                    PersistedSessionWriter::open(workspace, fixture.app_data.path(), locator, 0)
                        .unwrap();
                for (previous, current) in [
                    (SessionState::Idle, SessionState::Preparing),
                    (SessionState::Preparing, SessionState::Capturing),
                    (SessionState::Capturing, SessionState::Transcribing),
                ] {
                    writer
                        .append(
                            lifecycle_append(previous, current, "2026-08-12T10:00:00Z"),
                            0,
                        )
                        .unwrap();
                }
                Ok(())
            })
            .unwrap();
        let service = fixture.service(Arc::new(FakeRuntime::default()));
        assert_eq!(service.recover_interrupted_sessions().unwrap(), 1);
        let recovered = fixture.current();
        assert_eq!(recovered.state, SessionState::Paused);
        assert_eq!(
            recovered.channel_health.microphone.detail_code.as_deref(),
            Some("recovery_required")
        );
    }
}
