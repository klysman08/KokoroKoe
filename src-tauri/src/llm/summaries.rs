use std::sync::{Arc, atomic::AtomicBool};

use crate::{
    domain::{
        AppError, GenerateSessionSummaryRequest, GenerateSessionSummaryResponse,
        GetSessionSummaryRequest, RequestId, SessionState, SessionSummaryContent,
        SessionSummaryStatus, SummaryActionItem, SummaryUsage, now_rfc3339,
    },
    persistence::{SettingsService, SummaryPublication, SummaryStore, TranscriptService},
    prompts::{PromptContextRequest, PromptTask, SummaryKind, build_prompt_context},
};

use super::{
    budget::UsageReconciliation,
    completion::CompletionError,
    openrouter::OpenRouterService,
    results::{GeneratedResult, GeneratedResultError, SessionSummary},
};

#[derive(Clone)]
pub(crate) struct SummaryService {
    settings: SettingsService,
    transcripts: TranscriptService,
    openrouter: OpenRouterService,
}

impl SummaryService {
    pub(crate) fn new(
        settings: SettingsService,
        transcripts: TranscriptService,
        openrouter: OpenRouterService,
    ) -> Self {
        Self {
            settings,
            transcripts,
            openrouter,
        }
    }

    /// Reads the Session's saved `summary.md`, if it has one.
    pub(crate) fn get_summary(
        &self,
        request: GetSessionSummaryRequest,
    ) -> Result<SessionSummaryStatus, AppError> {
        request.validate().map_err(AppError::summary_error)?;
        let context = match self
            .transcripts
            .get_session_summary_context(request.project_id, request.session_id)
        {
            Ok(context) => Some(context),
            // A Session with no finalized transcript can never have a summary.
            Err(error) if error.code == "summary_transcript_empty" => None,
            Err(error) => return Err(error),
        };
        let document = match context {
            Some(context) => self.settings.with_workspace_operation(|workspace| {
                SummaryStore::open(workspace)
                    .and_then(|store| {
                        store.read_summary(&context.locator, request.project_id, request.session_id)
                    })
                    .map_err(|error| AppError::summary_error(error.code))
            })?,
            None => None,
        };
        let status = SessionSummaryStatus {
            schema_version: 1,
            project_id: request.project_id,
            session_id: request.session_id,
            document,
        };
        status.validate().map_err(AppError::summary_error)?;
        Ok(status)
    }

    /// Generates the final summary and publishes it as the Session's
    /// `summary.md`, replacing any previous document.
    pub(crate) fn generate(
        &self,
        request: GenerateSessionSummaryRequest,
    ) -> Result<GenerateSessionSummaryResponse, AppError> {
        request.validate().map_err(AppError::summary_error)?;
        let context = self
            .transcripts
            .get_session_summary_context(request.project_id, request.session_id)?;
        if !matches!(
            context.session.state,
            SessionState::Completed | SessionState::Failed
        ) {
            return Err(AppError::summary_error("summary_session_not_finished"));
        }
        let envelope = build_prompt_context(PromptContextRequest {
            project: &context.project,
            session: &context.session,
            task: PromptTask::Summary {
                kind: SummaryKind::Final,
            },
            relevant_segments: &context.sampled_segments,
            recent_segments: &context.closing_segments,
        })
        .map_err(|error| map_prompt_error(error.code))?;
        let segments_included = u32::try_from(envelope.selected_segment_ids.len())
            .map_err(|_| AppError::summary_error("summary_response_invalid"))?;
        let segments_considered = u32::try_from(context.total_segments)
            .map_err(|_| AppError::summary_error("summary_response_invalid"))?;
        let model_id = envelope.model_id.clone();

        let cancellation = Arc::new(AtomicBool::new(false));
        let primary = self
            .openrouter
            .complete_with_retry(
                request.request_id,
                &context.session,
                &envelope,
                &cancellation,
            )
            .map_err(map_completion_error)?;
        let primary_attempts = primary.attempts;
        let validated = self
            .openrouter
            .validate_or_repair_completion(
                primary.completion,
                RequestId::new(),
                &context.session,
                &envelope,
                &cancellation,
            )
            .map_err(map_generated_error)?;
        let GeneratedResult::SessionSummary(summary) = validated.result else {
            return Err(AppError::summary_error("summary_response_invalid"));
        };

        let generated_at = now_rfc3339()?;
        let content = summary_content(summary);
        let document = self.settings.with_workspace_operation(|workspace| {
            SummaryStore::open(workspace)
                .and_then(|store| {
                    store.write_summary(
                        &context.locator,
                        SummaryPublication {
                            session: &context.session,
                            content: &content,
                            generated_at: &generated_at,
                            model_id: &model_id,
                            segments_considered,
                            segments_included,
                        },
                    )
                })
                .map_err(|error| AppError::summary_error(error.code))
        })?;

        let final_usage = validated
            .repair_usage
            .as_ref()
            .unwrap_or(&validated.primary_usage);
        let response = GenerateSessionSummaryResponse {
            schema_version: 1,
            request_id: request.request_id,
            document,
            primary_attempts,
            repaired: validated.repaired,
            primary_usage: public_usage(&validated.primary_usage),
            repair_usage: validated.repair_usage.as_ref().map(public_usage),
            session_actual_cost_usd: final_usage.session_actual_cost_usd.clone(),
            available_budget_usd: final_usage.available_budget_usd.clone(),
        };
        response.validate().map_err(AppError::summary_error)?;
        Ok(response)
    }
}

/// Drops the generated segment references: `summary.md` is portable prose, so
/// it carries no internal identifiers.
fn summary_content(summary: SessionSummary) -> SessionSummaryContent {
    SessionSummaryContent {
        executive_summary: summary.executive_summary,
        main_topics: summary.main_topics,
        decisions: summary.decisions,
        action_items: summary
            .action_items
            .into_iter()
            .map(|item| SummaryActionItem {
                text: item.text,
                owner: item.owner,
                deadline: item.deadline,
            })
            .collect(),
        risks: summary.risks,
        open_questions: summary.open_questions,
        next_steps: summary.next_steps,
    }
}

fn public_usage(usage: &UsageReconciliation) -> SummaryUsage {
    SummaryUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        actual_cost_usd: usage.actual_cost_usd.clone(),
    }
}

fn map_prompt_error(code: &str) -> AppError {
    let public = match code {
        "prompt_model_not_selected" => "summary_model_required",
        "prompt_context_budget_too_small"
        | "prompt_context_budget_exceeded"
        | "prompt_selected_segment_exceeds_budget" => "summary_context_too_large",
        _ => "summary_invalid",
    };
    AppError::summary_error(public)
}

fn map_completion_error(error: CompletionError) -> AppError {
    let public = match error.code {
        "completion_credential_missing" | "completion_credential_invalid" => {
            "summary_credential_required"
        }
        "completion_authentication_failed" | "completion_permission_denied" => {
            "summary_authentication_failed"
        }
        "completion_payment_required" => "summary_payment_required",
        "completion_catalog_required" | "completion_model_not_available" => {
            "summary_catalog_required"
        }
        "completion_model_unsupported" => "summary_model_required",
        "completion_budget_exceeded" => "summary_budget_exceeded",
        "completion_rate_limited" => "summary_rate_limited",
        "completion_cancelled" => "summary_cancelled",
        "completion_provider_requirements_unavailable" => {
            "summary_provider_requirements_unavailable"
        }
        "completion_context_rejected" => "summary_context_too_large",
        "completion_request_rejected" => "summary_request_rejected",
        "completion_timeout" => "summary_timeout",
        "completion_network_unavailable" => "summary_network_unavailable",
        "completion_provider_unavailable"
        | "completion_provider_overloaded"
        | "completion_provider_failed" => "summary_provider_temporarily_unavailable",
        _ => "summary_provider_unavailable",
    };
    AppError::summary_error(public)
}

fn map_generated_error(error: GeneratedResultError) -> AppError {
    if let Some(completion) = error.completion_error {
        return map_completion_error(completion);
    }
    let public = match error.code {
        "generated_content_filtered" => "summary_content_filtered",
        _ => "summary_response_invalid",
    };
    AppError::summary_error(public)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Duration,
    };

    use serde_json::json;
    use tempfile::tempdir;

    use crate::{
        audio::AudioSource,
        domain::{
            CreateProjectInput, CreateSessionSnapshotInput, OpenRouterDataCollection,
            OpenRouterModel, ProjectId, SessionId,
        },
        persistence::{
            FinalizedTranscriptSegment, JournalAppend, JournalMutation, ProjectService,
            SessionJournal, SessionLocator, SessionService, SessionStore, TranscriptStore,
        },
    };

    use super::*;

    const TEST_KEY: &str = "summary-secret-canary";
    const TOTAL_SEGMENTS: usize = 120;

    struct ResponsePlan {
        status: u16,
        body: String,
    }

    fn start_server(plans: Vec<ResponsePlan>) -> (String, mpsc::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for plan in plans {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = stream.read(&mut buffer).unwrap();
                    request.extend_from_slice(&buffer[..read]);
                }
                let head_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                let head = String::from_utf8(request[..head_end].to_vec()).unwrap();
                let content_length = head
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap();
                while request.len() - head_end < content_length {
                    let read = stream.read(&mut buffer).unwrap();
                    request.extend_from_slice(&buffer[..read]);
                }
                sender
                    .send(request[head_end..head_end + content_length].to_vec())
                    .unwrap();
                let content_type = if plan.status == 200 {
                    "text/event-stream"
                } else {
                    "application/json"
                };
                write!(
                    stream,
                    "HTTP/1.1 {} Test\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    plan.status,
                    content_type,
                    plan.body.len(),
                    plan.body
                )
                .unwrap();
            }
        });
        (format!("http://{address}"), receiver)
    }

    fn completion_plan(content: &str, input_tokens: u32, output_tokens: u32) -> ResponsePlan {
        let content = serde_json::to_string(content).unwrap();
        ResponsePlan {
            status: 200,
            body: format!(
                concat!(
                    "data: {{\"model\":\"example/text-model\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":{}}},\"finish_reason\":\"stop\"}}]}}\n\n",
                    "data: {{\"model\":\"example/text-model\",\"choices\":[],\"usage\":{{\"prompt_tokens\":{},\"completion_tokens\":{},\"total_tokens\":{}}}}}\n\n",
                    "data: [DONE]\n\n"
                ),
                content,
                input_tokens,
                output_tokens,
                input_tokens + output_tokens
            ),
        }
    }

    fn settings(workspace: &std::path::Path, app_data: &std::path::Path) -> SettingsService {
        let documents = workspace.parent().unwrap().join("documents");
        std::fs::create_dir_all(&documents).unwrap();
        let settings = SettingsService::open(app_data.to_path_buf(), documents).unwrap();
        settings.choose_workspace(workspace).unwrap();
        settings
    }

    fn project_input() -> CreateProjectInput {
        serde_json::from_value(json!({
            "name": "Summary project",
            "description": "",
            "globalContext": "Keep meeting evidence minimal.",
            "participants": [],
            "tags": [],
            "defaultPresetId": "11111111-1111-4111-8111-111111111111",
            "defaultTranscriptionModelId": "whisper-tiny",
            "preferredLlmModels": {}
        }))
        .unwrap()
    }

    fn session_input() -> CreateSessionSnapshotInput {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/session-management-v1.json"
        ))
        .unwrap();
        let mut value = fixture["createRequest"]["value"].clone();
        value["llmModels"] = json!({"summaries":"example/text-model"});
        value["spendingLimitUsd"] = json!("1.00");
        value["maxTokensPerRequest"] = json!(16384);
        serde_json::from_value(value).unwrap()
    }

    fn model() -> OpenRouterModel {
        OpenRouterModel {
            id: "example/text-model".to_owned(),
            name: "Example".to_owned(),
            provider: "example".to_owned(),
            context_length: 32_768,
            prompt_price_per_token: "0.000001".to_owned(),
            completion_price_per_token: "0.000002".to_owned(),
            supports_structured_outputs: true,
            supports_streaming: true,
            zero_data_retention_available: true,
            data_collection: OpenRouterDataCollection::Deny,
        }
    }

    fn valid_summary() -> String {
        json!({
            "executiveSummary":"The team kept the authoritative transcript local.",
            "mainTopics":["Transcript storage"],
            "decisions":["Keep the transcript local."],
            "actionItems":[{
                "text":"Confirm the retention window.",
                "owner":"Product lead",
                "relatedSegmentIds":[]
            }],
            "risks":[],
            "openQuestions":["How long may audio be kept?"],
            "nextSteps":["Bring the proposal to review."]
        })
        .to_string()
    }

    struct Fixture {
        service: SummaryService,
        project_id: ProjectId,
        session_id: SessionId,
        requests: mpsc::Receiver<Vec<u8>>,
        _root: tempfile::TempDir,
    }

    fn fixture(plans: Vec<ResponsePlan>, stop_session: bool) -> Fixture {
        let root = tempdir().unwrap();
        let workspace = root.path().join("workspace-secret-path");
        let app_data = root.path().join("app-data");
        std::fs::create_dir_all(&workspace).unwrap();
        let settings = settings(&workspace, &app_data);
        let project = ProjectService::new(settings.clone(), app_data.clone())
            .create_project(project_input())
            .unwrap();
        let sessions = SessionService::new(settings.clone(), app_data.clone());
        let session = sessions
            .create_session(project.id, session_input())
            .unwrap();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let journal = SessionJournal::open(&workspace).unwrap();
        for index in 0..TOTAL_SEGMENTS {
            journal
                .append(
                    &locator,
                    JournalAppend {
                        event_id: uuid::Uuid::new_v4(),
                        recorded_at: format!("2026-08-21T10:{:02}:{:02}Z", index / 60, index % 60),
                        mutation: JournalMutation::FinalizedTranscriptSegment(
                            FinalizedTranscriptSegment {
                                id: uuid::Uuid::parse_str(&format!(
                                    "90000000-0000-4000-8000-{index:012}"
                                ))
                                .unwrap(),
                                source: AudioSource::Microphone,
                                start_ms: index as u64 * 1_000,
                                end_ms: index as u64 * 1_000 + 800,
                                text: format!("meeting statement {index}"),
                                language: "en-GB".to_owned(),
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
        if stop_session {
            complete_session(&workspace, &locator);
        }

        let (base_url, requests) = start_server(plans);
        let openrouter =
            OpenRouterService::for_completion_test(&base_url, TEST_KEY, Duration::from_secs(2));
        openrouter.seed_catalog_for_test(vec![model()]);
        Fixture {
            service: SummaryService::new(
                settings.clone(),
                TranscriptService::new(settings, app_data),
                openrouter,
            ),
            project_id: project.id,
            session_id: session.id,
            requests,
            _root: root,
        }
    }

    /// Drives the stored Session snapshot to `completed` through the same store
    /// path the lifecycle boundary uses, without starting the capture runtime a
    /// unit test cannot own.
    fn complete_session(workspace: &std::path::Path, locator: &SessionLocator) {
        let store = SessionStore::open(workspace).unwrap();
        let snapshot = store.read_session(locator).unwrap();
        let updated = snapshot
            .session
            .apply_runtime_state(SessionState::Completed, now_rfc3339().unwrap(), None)
            .unwrap();
        store
            .update_session(
                locator,
                &updated,
                snapshot.session.revision,
                snapshot.fingerprint,
            )
            .unwrap();
    }

    fn generate_request(
        project_id: ProjectId,
        session_id: SessionId,
    ) -> GenerateSessionSummaryRequest {
        serde_json::from_value(json!({
            "requestId":"a0000000-0000-4000-8000-000000000001",
            "projectId":project_id,
            "sessionId":session_id
        }))
        .unwrap()
    }

    fn get_request(project_id: ProjectId, session_id: SessionId) -> GetSessionSummaryRequest {
        serde_json::from_value(json!({
            "projectId":project_id,
            "sessionId":session_id
        }))
        .unwrap()
    }

    #[test]
    fn provider_requirement_failures_are_actionable_and_content_free() {
        let error = super::map_completion_error(CompletionError::new(
            "completion_provider_requirements_unavailable",
            crate::llm::completion::ReservationDisposition::Released,
        ));

        assert_eq!(error.code, "summary_provider_requirements_unavailable");
        assert!(error.user_message.contains("choose another model"));
        assert_eq!(
            error.technical_detail.as_deref(),
            Some("summary_provider_requirements_unavailable")
        );
    }

    #[test]
    fn a_running_session_is_refused_before_any_request() {
        let fixture = fixture(Vec::new(), false);

        let error = fixture
            .service
            .generate(generate_request(fixture.project_id, fixture.session_id))
            .unwrap_err();

        assert_eq!(error.code, "summary_session_not_finished");
    }

    #[test]
    fn an_absent_summary_reads_as_an_empty_status() {
        let fixture = fixture(Vec::new(), true);

        let status = fixture
            .service
            .get_summary(get_request(fixture.project_id, fixture.session_id))
            .unwrap();

        assert!(status.document.is_none());
        assert_eq!(status.project_id, fixture.project_id);
    }

    #[test]
    fn generation_publishes_a_readable_document_and_states_honest_coverage() {
        let fixture = fixture(
            vec![
                completion_plan("{broken", 900, 40),
                completion_plan(&valid_summary(), 200, 80),
            ],
            true,
        );

        let response = fixture
            .service
            .generate(generate_request(fixture.project_id, fixture.session_id))
            .unwrap();

        assert!(response.repaired);
        assert_eq!(response.primary_attempts, 1);
        assert_eq!(response.document.segments_considered, TOTAL_SEGMENTS as u32);
        assert!(response.document.segments_included > 0);
        assert!(response.document.segments_included < TOTAL_SEGMENTS as u32);
        assert!(response.document.markdown.contains("## Decisions"));
        assert!(response.document.markdown.contains("## Open questions"));
        assert!(response.document.markdown.contains("Owner: Product lead"));
        assert!(response.document.markdown.contains("_None recorded._"));

        let status = fixture
            .service
            .get_summary(get_request(fixture.project_id, fixture.session_id))
            .unwrap();
        assert_eq!(status.document.unwrap(), response.document);

        let primary: serde_json::Value = serde_json::from_slice(
            &fixture
                .requests
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
        )
        .unwrap();
        let repair: serde_json::Value = serde_json::from_slice(
            &fixture
                .requests
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
        )
        .unwrap();
        let serialized = primary.to_string();
        assert!(serialized.contains("meeting statement 0"));
        assert!(serialized.contains(&format!("meeting statement {}", TOTAL_SEGMENTS - 1)));
        assert!(!serialized.contains("workspace-secret-path"));
        assert!(!serialized.contains(TEST_KEY));
        assert!(!serialized.contains("microphone-endpoint"));
        assert_eq!(primary["provider"]["zdr"], true);
        assert_eq!(primary["provider"]["data_collection"], "deny");
        let repair_text = repair.to_string();
        assert!(repair_text.contains("{broken"));
        assert!(!repair_text.contains("meeting statement"));
    }
}
