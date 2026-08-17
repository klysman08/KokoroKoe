use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde::Serialize;

use crate::domain::{
    AppError, Project, ProjectId, Session, SessionId, TranscriptPage, TranscriptPageRequest,
    TranscriptSearchHit, TranscriptSearchPageView, TranscriptSearchQuery, TranscriptSegmentStatus,
    TranscriptSegmentView,
};

use super::{
    ProjectCatalog, SessionCatalog, SessionLocator, SettingsService, TranscriptSearchCatalog,
    TranscriptSearchRequest, TranscriptStore,
};

const READ_CURSOR_PREFIX: &str = "r1:";
const MAX_READ_CURSOR_OFFSET: usize = 100_000;
const MANUAL_QUESTION_NEIGHBORS_PER_SIDE: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManualQuestionContext {
    pub(crate) project: Project,
    pub(crate) session: Session,
    pub(crate) selected_segment: TranscriptSegmentView,
    pub(crate) neighboring_segments: Vec<TranscriptSegmentView>,
}

#[derive(Clone)]
pub(crate) struct TranscriptService {
    settings: SettingsService,
    app_data_directory: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

impl TranscriptService {
    pub(crate) fn new(settings: SettingsService, app_data_directory: PathBuf) -> Self {
        Self {
            settings,
            app_data_directory,
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn get_transcript_page(
        &self,
        request: TranscriptPageRequest,
    ) -> Result<TranscriptPage, AppError> {
        request.validate().map_err(AppError::transcript_error)?;
        let cursor = ReadCursor::decode(&request).map_err(AppError::transcript_error)?;
        self.with_workspace(|workspace| {
            let projects = ProjectCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::transcript_error(error.code))?;
            let sessions = SessionCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::transcript_error(error.code))?;
            let project = projects
                .read_project(request.project_id)
                .map_err(|error| AppError::transcript_error(error.code))?
                .project;
            let session = sessions
                .read_session(request.project_id, request.session_id)
                .map_err(|error| AppError::transcript_error(error.code))?
                .session;
            let locator = SessionLocator::from_records(&project, &session)
                .map_err(|error| AppError::transcript_error(error.code))?;
            let snapshot = TranscriptStore::open(workspace)
                .and_then(|store| store.read_transcript(&locator))
                .map_err(|error| AppError::transcript_error(error.code))?;
            let fingerprint = encode_hex(&snapshot.fingerprint.as_bytes());
            if cursor
                .fingerprint
                .as_ref()
                .is_some_and(|expected| expected != &fingerprint)
            {
                return Err(AppError::transcript_error("transcript_page_stale"));
            }
            if cursor.offset > snapshot.segments.len() || cursor.offset > MAX_READ_CURSOR_OFFSET {
                return Err(AppError::transcript_error("transcript_page_cursor_invalid"));
            }
            let end = cursor
                .offset
                .saturating_add(usize::from(request.limit))
                .min(snapshot.segments.len());
            let items = snapshot.segments[cursor.offset..end]
                .iter()
                .cloned()
                .map(|segment| TranscriptSegmentView {
                    id: segment.id,
                    project_id: request.project_id,
                    session_id: request.session_id,
                    source: segment.source,
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    text: segment.text,
                    status: TranscriptSegmentStatus::Final,
                    language: segment.language,
                })
                .collect::<Vec<_>>();
            let next_cursor = (end < snapshot.segments.len()).then(|| {
                format!(
                    "{READ_CURSOR_PREFIX}{}:{}:{fingerprint}:{end}",
                    id_text(request.project_id),
                    id_text(request.session_id)
                )
            });
            let page = TranscriptPage { items, next_cursor };
            page.validate().map_err(AppError::transcript_error)?;
            Ok(page)
        })
    }

    pub(crate) fn get_manual_question_context(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        selected_segment_id: uuid::Uuid,
    ) -> Result<ManualQuestionContext, AppError> {
        if project_id.as_uuid().is_nil()
            || session_id.as_uuid().is_nil()
            || selected_segment_id.is_nil()
        {
            return Err(AppError::manual_question_error("manual_question_invalid"));
        }
        self.with_workspace(|workspace| {
            let project = ProjectCatalog::open(workspace, self.app_data_directory.clone())
                .and_then(|catalog| catalog.read_project(project_id))
                .map_err(|error| AppError::transcript_error(error.code))?
                .project;
            let session = SessionCatalog::open(workspace, self.app_data_directory.clone())
                .and_then(|catalog| catalog.read_session(project_id, session_id))
                .map_err(|error| AppError::transcript_error(error.code))?
                .session;
            let locator = SessionLocator::from_records(&project, &session)
                .map_err(|error| AppError::transcript_error(error.code))?;
            let snapshot = TranscriptStore::open(workspace)
                .and_then(|store| store.read_transcript(&locator))
                .map_err(|error| AppError::transcript_error(error.code))?;
            let selected_index = snapshot
                .segments
                .iter()
                .position(|segment| segment.id == selected_segment_id)
                .ok_or_else(|| {
                    AppError::manual_question_error("manual_question_segment_not_found")
                })?;
            let start = selected_index.saturating_sub(MANUAL_QUESTION_NEIGHBORS_PER_SIDE);
            let end = selected_index
                .saturating_add(MANUAL_QUESTION_NEIGHBORS_PER_SIDE + 1)
                .min(snapshot.segments.len());
            let neighboring_segments = snapshot.segments[start..end]
                .iter()
                .cloned()
                .map(|segment| TranscriptSegmentView {
                    id: segment.id,
                    project_id,
                    session_id,
                    source: segment.source,
                    start_ms: segment.start_ms,
                    end_ms: segment.end_ms,
                    text: segment.text,
                    status: TranscriptSegmentStatus::Final,
                    language: segment.language,
                })
                .collect::<Vec<_>>();
            let selected_segment = neighboring_segments[selected_index - start].clone();
            Ok(ManualQuestionContext {
                project,
                session,
                selected_segment,
                neighboring_segments,
            })
        })
    }

    pub(crate) fn search_transcript(
        &self,
        request: TranscriptSearchQuery,
    ) -> Result<TranscriptSearchPageView, AppError> {
        request.validate().map_err(AppError::transcript_error)?;
        self.with_workspace(|workspace| {
            let projects = ProjectCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::transcript_error(error.code))?;
            projects
                .read_project(request.project_id)
                .map_err(|error| AppError::transcript_error(error.code))?;
            if let Some(session_id) = request.session_id {
                SessionCatalog::open(workspace, self.app_data_directory.clone())
                    .and_then(|catalog| catalog.read_session(request.project_id, session_id))
                    .map_err(|error| AppError::transcript_error(error.code))?;
            }
            let page = TranscriptSearchCatalog::open(workspace, self.app_data_directory.clone())
                .and_then(|catalog| {
                    catalog.search(&TranscriptSearchRequest {
                        project_id: request.project_id,
                        session_id: request.session_id,
                        query: request.query,
                        cursor: request.cursor,
                        limit: request.limit,
                    })
                })
                .map_err(|error| AppError::transcript_error(error.code))?;
            let page = TranscriptSearchPageView {
                items: page
                    .items
                    .into_iter()
                    .map(|item| TranscriptSearchHit {
                        id: item.segment_id,
                        project_id: item.project_id,
                        session_id: item.session_id,
                        source: item.source,
                        start_ms: item.start_ms,
                        end_ms: item.end_ms,
                        language: item.language,
                        snippet: item.snippet,
                    })
                    .collect(),
                next_cursor: page.next_cursor,
            };
            page.validate().map_err(AppError::transcript_error)?;
            Ok(page)
        })
    }

    fn with_workspace<T>(
        &self,
        operation: impl FnOnce(&std::path::Path) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| AppError::transcript_worker_failed())?;
        self.settings.with_workspace_operation(operation)
    }
}

struct ReadCursor {
    fingerprint: Option<String>,
    offset: usize,
}

impl ReadCursor {
    fn decode(request: &TranscriptPageRequest) -> Result<Self, &'static str> {
        let Some(cursor) = request.cursor.as_deref() else {
            return Ok(Self {
                fingerprint: None,
                offset: 0,
            });
        };
        let value = cursor
            .strip_prefix(READ_CURSOR_PREFIX)
            .ok_or("transcript_page_cursor_invalid")?;
        let mut parts = value.split(':');
        let project_id = parts.next().ok_or("transcript_page_cursor_invalid")?;
        let session_id = parts.next().ok_or("transcript_page_cursor_invalid")?;
        let fingerprint = parts.next().ok_or("transcript_page_cursor_invalid")?;
        let offset = parts
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value <= MAX_READ_CURSOR_OFFSET)
            .ok_or("transcript_page_cursor_invalid")?;
        if parts.next().is_some()
            || project_id != id_text(request.project_id)
            || session_id != id_text(request.session_id)
            || fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
        {
            return Err("transcript_page_cursor_invalid");
        }
        Ok(Self {
            fingerprint: Some(fingerprint.to_owned()),
            offset,
        })
    }
}

fn id_text(id: impl Serialize) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::{
        domain::{
            CreateProjectInput, CreateSessionSnapshotInput, TranscriptPageRequest,
            TranscriptSearchQuery,
        },
        persistence::{
            ProjectService, SessionJournal, SessionService, SettingsService, TranscriptStore,
        },
    };

    use super::TranscriptService;

    fn settings(workspace: &std::path::Path, app_data: &std::path::Path) -> SettingsService {
        let documents = workspace.parent().unwrap().join("documents");
        std::fs::create_dir_all(&documents).unwrap();
        let settings = SettingsService::open(app_data.to_path_buf(), documents).unwrap();
        settings.choose_workspace(workspace).unwrap();
        settings
    }

    fn project_input() -> CreateProjectInput {
        serde_json::from_value(serde_json::json!({
            "name": "Transcript project", "description": "", "globalContext": "",
            "participants": [], "tags": [],
            "defaultPresetId": "11111111-1111-4111-8111-111111111111",
            "defaultTranscriptionModelId": "whisper-tiny", "preferredLlmModels": {}
        }))
        .unwrap()
    }

    fn session_input() -> CreateSessionSnapshotInput {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/session-management-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["createRequest"]["value"].clone()).unwrap()
    }

    #[test]
    fn authoritative_pages_and_scoped_search_are_bounded() {
        let root = tempdir().unwrap();
        let workspace = root.path().join("workspace");
        let app_data = root.path().join("app-data");
        std::fs::create_dir_all(&workspace).unwrap();
        let settings = settings(&workspace, &app_data);
        let project = ProjectService::new(settings.clone(), app_data.clone())
            .create_project(project_input())
            .unwrap();
        let session = SessionService::new(settings.clone(), app_data.clone())
            .create_session(project.id, session_input())
            .unwrap();
        let locator = super::SessionLocator::from_records(&project, &session).unwrap();
        let journal = SessionJournal::open(&workspace).unwrap();
        for (index, text) in ["authoritative first", "authoritative second"]
            .into_iter()
            .enumerate()
        {
            journal
                .append(
                    &locator,
                    crate::persistence::JournalAppend {
                        event_id: uuid::Uuid::new_v4(),
                        recorded_at: format!("2026-08-12T10:00:0{index}Z"),
                        mutation: crate::persistence::JournalMutation::FinalizedTranscriptSegment(
                            crate::persistence::FinalizedTranscriptSegment {
                                id: uuid::Uuid::new_v4(),
                                source: crate::audio::AudioSource::Microphone,
                                start_ms: index as u64 * 1000,
                                end_ms: index as u64 * 1000 + 800,
                                text: text.to_owned(),
                                language: "en-US".to_owned(),
                            },
                        ),
                    },
                )
                .unwrap();
        }
        let transcript_store = TranscriptStore::open(&workspace).unwrap();
        let initial_snapshot = transcript_store.materialize(&locator, None).unwrap();
        let service = TranscriptService::new(settings, app_data);
        let first = service
            .get_transcript_page(TranscriptPageRequest {
                project_id: project.id,
                session_id: session.id,
                cursor: None,
                limit: 1,
            })
            .unwrap();
        assert_eq!(first.items[0].text, "authoritative first");
        let original_cursor = first.next_cursor.clone();
        let second = service
            .get_transcript_page(TranscriptPageRequest {
                project_id: project.id,
                session_id: session.id,
                cursor: original_cursor.clone(),
                limit: 1,
            })
            .unwrap();
        assert_eq!(second.items[0].text, "authoritative second");
        let search = service
            .search_transcript(TranscriptSearchQuery {
                project_id: project.id,
                session_id: Some(session.id),
                query: "second".to_owned(),
                cursor: None,
                limit: 20,
            })
            .unwrap();
        assert_eq!(search.items.len(), 1);
        assert!(search.items[0].snippet.contains("second"));

        journal
            .append(
                &locator,
                crate::persistence::JournalAppend {
                    event_id: uuid::Uuid::new_v4(),
                    recorded_at: "2026-08-12T10:00:03Z".to_owned(),
                    mutation: crate::persistence::JournalMutation::FinalizedTranscriptSegment(
                        crate::persistence::FinalizedTranscriptSegment {
                            id: uuid::Uuid::new_v4(),
                            source: crate::audio::AudioSource::SystemOutput,
                            start_ms: 2_000,
                            end_ms: 2_800,
                            text: "new durable segment".to_owned(),
                            language: "en-US".to_owned(),
                        },
                    ),
                },
            )
            .unwrap();
        transcript_store
            .materialize(&locator, Some(initial_snapshot.fingerprint))
            .unwrap();
        let stale = service
            .get_transcript_page(TranscriptPageRequest {
                project_id: project.id,
                session_id: session.id,
                cursor: original_cursor,
                limit: 1,
            })
            .unwrap_err();
        assert_eq!(stale.code, "transcript_page_stale");
    }

    #[test]
    fn manual_question_context_keeps_only_four_verified_neighbors_per_side() {
        let root = tempdir().unwrap();
        let workspace = root.path().join("workspace");
        let app_data = root.path().join("app-data");
        std::fs::create_dir_all(&workspace).unwrap();
        let settings = settings(&workspace, &app_data);
        let project = ProjectService::new(settings.clone(), app_data.clone())
            .create_project(project_input())
            .unwrap();
        let session = SessionService::new(settings.clone(), app_data.clone())
            .create_session(project.id, session_input())
            .unwrap();
        let locator = super::SessionLocator::from_records(&project, &session).unwrap();
        let journal = SessionJournal::open(&workspace).unwrap();
        let segment_ids = (1..=11)
            .map(|index| {
                uuid::Uuid::parse_str(&format!("50000000-0000-4000-8000-{index:012}")).unwrap()
            })
            .collect::<Vec<_>>();
        for (index, segment_id) in segment_ids.iter().enumerate() {
            journal
                .append(
                    &locator,
                    crate::persistence::JournalAppend {
                        event_id: uuid::Uuid::new_v4(),
                        recorded_at: format!("2026-08-12T10:00:{index:02}Z"),
                        mutation: crate::persistence::JournalMutation::FinalizedTranscriptSegment(
                            crate::persistence::FinalizedTranscriptSegment {
                                id: *segment_id,
                                source: crate::audio::AudioSource::Microphone,
                                start_ms: index as u64 * 1_000,
                                end_ms: index as u64 * 1_000 + 800,
                                text: format!("verified segment {index}"),
                                language: "en-US".to_owned(),
                            },
                        ),
                    },
                )
                .unwrap();
        }
        TranscriptStore::open(&workspace)
            .unwrap()
            .materialize(&locator, None)
            .unwrap();
        let service = TranscriptService::new(settings, app_data);

        let context = service
            .get_manual_question_context(project.id, session.id, segment_ids[5])
            .unwrap();

        assert_eq!(context.project.id, project.id);
        assert_eq!(context.session.id, session.id);
        assert_eq!(context.selected_segment.id, segment_ids[5]);
        assert_eq!(
            context
                .neighboring_segments
                .iter()
                .map(|segment| segment.id)
                .collect::<Vec<_>>(),
            segment_ids[1..10]
        );
        assert!(
            context
                .neighboring_segments
                .iter()
                .all(|segment| segment.text.starts_with("verified segment"))
        );
        let missing = service
            .get_manual_question_context(project.id, session.id, uuid::Uuid::new_v4())
            .unwrap_err();
        assert_eq!(missing.code, "manual_question_segment_not_found");
    }
}
