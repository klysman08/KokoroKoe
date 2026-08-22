use std::sync::{Arc, atomic::AtomicBool};

use uuid::Uuid;

use crate::{
    domain::{
        AppError, GenerateRecentInsightsRequest, InsightUsage, RecentInsight,
        RecentInsightsResponse, RequestId,
    },
    persistence::TranscriptService,
    prompts::{PromptContextRequest, PromptTask, build_prompt_context},
};

use super::{
    budget::UsageReconciliation,
    completion::CompletionError,
    openrouter::OpenRouterService,
    results::{GeneratedResult, GeneratedResultError},
};

#[derive(Clone)]
pub(crate) struct InsightService {
    transcripts: TranscriptService,
    openrouter: OpenRouterService,
}

impl InsightService {
    pub(crate) fn new(transcripts: TranscriptService, openrouter: OpenRouterService) -> Self {
        Self {
            transcripts,
            openrouter,
        }
    }

    pub(crate) fn generate_recent(
        &self,
        request: GenerateRecentInsightsRequest,
    ) -> Result<RecentInsightsResponse, AppError> {
        request.validate().map_err(AppError::insight_error)?;
        let context = self
            .transcripts
            .get_recent_insight_context(request.project_id, request.session_id)?;
        if context.session.preset.insight_types.is_empty() {
            return Err(AppError::insight_error("insight_types_required"));
        }
        let requested_types = context.session.preset.insight_types.clone();
        let envelope = build_prompt_context(PromptContextRequest {
            project: &context.project,
            session: &context.session,
            task: PromptTask::Insight {
                requested_types: &requested_types,
            },
            relevant_segments: &context.recent_segments,
            recent_segments: &context.recent_segments,
        })
        .map_err(|error| map_prompt_error(error.code))?;
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
        let GeneratedResult::InsightBatch(batch) = validated.result else {
            return Err(AppError::insight_error("insight_response_invalid"));
        };
        let insights = batch
            .insights
            .into_iter()
            .map(|insight| {
                let related_segment_ids = insight
                    .related_segment_ids
                    .iter()
                    .map(|value| Uuid::parse_str(value))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| AppError::insight_error("insight_response_invalid"))?;
                Ok(RecentInsight {
                    r#type: insight.r#type,
                    title: insight.title,
                    content: insight.content,
                    rationale: insight.rationale,
                    related_segment_ids,
                    confidence: insight.confidence,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let final_usage = validated
            .repair_usage
            .as_ref()
            .unwrap_or(&validated.primary_usage);
        let response = RecentInsightsResponse {
            schema_version: 1,
            request_id: request.request_id,
            project_id: request.project_id,
            session_id: request.session_id,
            insights,
            primary_attempts,
            repaired: validated.repaired,
            primary_usage: public_usage(&validated.primary_usage),
            repair_usage: validated.repair_usage.as_ref().map(public_usage),
            session_actual_cost_usd: final_usage.session_actual_cost_usd.clone(),
            available_budget_usd: final_usage.available_budget_usd.clone(),
        };
        response.validate().map_err(AppError::insight_error)?;
        Ok(response)
    }
}

fn public_usage(usage: &UsageReconciliation) -> InsightUsage {
    InsightUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        actual_cost_usd: usage.actual_cost_usd.clone(),
    }
}

fn map_prompt_error(code: &str) -> AppError {
    let public = match code {
        "prompt_model_not_selected" => "insight_model_required",
        "prompt_context_budget_too_small" | "prompt_selected_segment_exceeds_budget" => {
            "insight_context_too_large"
        }
        "prompt_task_invalid" => "insight_types_required",
        _ => "insight_invalid",
    };
    AppError::insight_error(public)
}

fn map_completion_error(error: CompletionError) -> AppError {
    let public = match error.code {
        "completion_credential_missing" | "completion_credential_invalid" => {
            "insight_credential_required"
        }
        "completion_authentication_failed" | "completion_permission_denied" => {
            "insight_authentication_failed"
        }
        "completion_payment_required" => "insight_payment_required",
        "completion_catalog_required" | "completion_model_not_available" => {
            "insight_catalog_required"
        }
        "completion_model_unsupported" => "insight_model_required",
        "completion_budget_exceeded" => "insight_budget_exceeded",
        "completion_rate_limited" => "insight_rate_limited",
        "completion_cancelled" => "insight_cancelled",
        "completion_provider_requirements_unavailable" => {
            "insight_provider_requirements_unavailable"
        }
        "completion_context_rejected" => "insight_context_too_large",
        "completion_request_rejected" => "insight_request_rejected",
        "completion_timeout" => "insight_timeout",
        "completion_network_unavailable" => "insight_network_unavailable",
        "completion_provider_unavailable"
        | "completion_provider_overloaded"
        | "completion_provider_failed" => "insight_provider_temporarily_unavailable",
        _ => "insight_provider_unavailable",
    };
    AppError::insight_error(public)
}

fn map_generated_error(error: GeneratedResultError) -> AppError {
    if let Some(completion) = error.completion_error {
        return map_completion_error(completion);
    }
    let public = match error.code {
        "generated_content_filtered" => "insight_content_filtered",
        _ => "insight_response_invalid",
    };
    AppError::insight_error(public)
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
            OpenRouterModel,
        },
        persistence::{
            FinalizedTranscriptSegment, JournalAppend, JournalMutation, ProjectService,
            SessionJournal, SessionLocator, SessionService, SettingsService, TranscriptStore,
        },
    };

    use super::*;

    const TEST_KEY: &str = "insight-secret-canary";
    const OLDEST_SEGMENTS: usize = 6;
    const RECENT_SEGMENTS: usize = 12;

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
            "name": "Insight project",
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
        value["llmModels"] = json!({"insights":"example/text-model"});
        value["spendingLimitUsd"] = json!("1.00");
        value["maxTokensPerRequest"] = json!(4096);
        serde_json::from_value(value).unwrap()
    }

    fn model() -> OpenRouterModel {
        OpenRouterModel {
            id: "example/text-model".to_owned(),
            name: "Example".to_owned(),
            provider: "example".to_owned(),
            context_length: 8_192,
            prompt_price_per_token: "0.000001".to_owned(),
            completion_price_per_token: "0.000002".to_owned(),
            supports_structured_outputs: true,
            supports_streaming: true,
            zero_data_retention_available: true,
            data_collection: OpenRouterDataCollection::Deny,
        }
    }

    struct Fixture {
        service: InsightService,
        project_id: crate::domain::ProjectId,
        session_id: crate::domain::SessionId,
        segment_ids: Vec<Uuid>,
        requests: mpsc::Receiver<Vec<u8>>,
        _root: tempfile::TempDir,
    }

    /// Journals `OLDEST_SEGMENTS` distant segments followed by `RECENT_SEGMENTS`
    /// nearby ones, so the bounded recent window is provably exercised.
    fn fixture(plans: Vec<ResponsePlan>) -> Fixture {
        let root = tempdir().unwrap();
        let workspace = root.path().join("workspace-secret-path");
        let app_data = root.path().join("app-data");
        std::fs::create_dir_all(&workspace).unwrap();
        let settings = settings(&workspace, &app_data);
        let project = ProjectService::new(settings.clone(), app_data.clone())
            .create_project(project_input())
            .unwrap();
        let session = SessionService::new(settings.clone(), app_data.clone())
            .create_session(project.id, session_input())
            .unwrap();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let journal = SessionJournal::open(&workspace).unwrap();
        let total = OLDEST_SEGMENTS + RECENT_SEGMENTS;
        let segment_ids = (1..=total)
            .map(|index| Uuid::parse_str(&format!("70000000-0000-4000-8000-{index:012}")).unwrap())
            .collect::<Vec<_>>();
        for (index, segment_id) in segment_ids.iter().enumerate() {
            let text = if index < OLDEST_SEGMENTS {
                format!("DISTANT-SECRET-CANARY-{index}")
            } else {
                format!("verified recent context {index}")
            };
            journal
                .append(
                    &locator,
                    JournalAppend {
                        event_id: Uuid::new_v4(),
                        recorded_at: format!("2026-08-21T10:00:{index:02}Z"),
                        mutation: JournalMutation::FinalizedTranscriptSegment(
                            FinalizedTranscriptSegment {
                                id: *segment_id,
                                source: AudioSource::Microphone,
                                start_ms: index as u64 * 1_000,
                                end_ms: index as u64 * 1_000 + 800,
                                text,
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

        let (base_url, requests) = start_server(plans);
        let openrouter =
            OpenRouterService::for_completion_test(&base_url, TEST_KEY, Duration::from_secs(2));
        openrouter.seed_catalog_for_test(vec![model()]);
        Fixture {
            service: InsightService::new(TranscriptService::new(settings, app_data), openrouter),
            project_id: project.id,
            session_id: session.id,
            segment_ids,
            requests,
            _root: root,
        }
    }

    fn request(
        project_id: crate::domain::ProjectId,
        session_id: crate::domain::SessionId,
    ) -> GenerateRecentInsightsRequest {
        serde_json::from_value(json!({
            "requestId":"80000000-0000-4000-8000-000000000001",
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

        assert_eq!(error.code, "insight_provider_requirements_unavailable");
        assert!(error.user_message.contains("choose another model"));
        assert_eq!(
            error.technical_detail.as_deref(),
            Some("insight_provider_requirements_unavailable")
        );

        let unavailable = super::map_completion_error(CompletionError::new(
            "completion_provider_unavailable",
            crate::llm::completion::ReservationDisposition::Released,
        ));
        assert_eq!(unavailable.code, "insight_provider_temporarily_unavailable");
        assert!(unavailable.user_message.contains("choose another model"));
    }

    /// A started Session materializes an empty transcript before any final
    /// arrives; insights must refuse that state without contacting OpenRouter.
    #[test]
    fn a_session_without_finalized_transcript_is_rejected_before_any_request() {
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
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        TranscriptStore::open(&workspace)
            .unwrap()
            .materialize(&locator, None)
            .unwrap();
        let openrouter = OpenRouterService::for_completion_test(
            "http://127.0.0.1:1",
            TEST_KEY,
            Duration::from_secs(2),
        );
        openrouter.seed_catalog_for_test(vec![model()]);
        let service = InsightService::new(TranscriptService::new(settings, app_data), openrouter);

        let error = service
            .generate_recent(request(project.id, session.id))
            .unwrap_err();

        assert_eq!(error.code, "insight_transcript_empty");
    }

    #[test]
    fn product_path_retries_repairs_and_sends_only_the_bounded_recent_window() {
        let expected_segment = Uuid::parse_str(&format!(
            "70000000-0000-4000-8000-{:012}",
            OLDEST_SEGMENTS + 2
        ))
        .unwrap();
        let valid_batch = json!({
            "insights":[{
                "type":"decision",
                "title":"Transcript stays local",
                "content":"The recent context records the local-only decision.",
                "relatedSegmentIds":[expected_segment.to_string()],
                "confidence":0.5
            }]
        })
        .to_string();
        let fixture = fixture(vec![
            ResponsePlan {
                status: 429,
                body: format!(r#"{{"error":{{"message":"{TEST_KEY}"}}}}"#),
            },
            completion_plan("{broken", 400, 20),
            completion_plan(&valid_batch, 100, 30),
        ]);

        let response = fixture
            .service
            .generate_recent(request(fixture.project_id, fixture.session_id))
            .unwrap();

        assert_eq!(response.primary_attempts, 2);
        assert!(response.repaired);
        assert_eq!(response.insights.len(), 1);
        assert_eq!(response.insights[0].related_segment_ids, [expected_segment]);
        assert_eq!(response.primary_usage.input_tokens, 400);
        assert_eq!(response.repair_usage.unwrap().input_tokens, 100);

        let first_primary: serde_json::Value = serde_json::from_slice(
            &fixture
                .requests
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
        )
        .unwrap();
        let second_primary: serde_json::Value = serde_json::from_slice(
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
        for body in [&first_primary, &second_primary] {
            let serialized = body.to_string();
            assert!(serialized.contains(&format!("verified recent context {OLDEST_SEGMENTS}")));
            assert!(!serialized.contains("DISTANT-SECRET-CANARY"));
            assert!(!serialized.contains("workspace-secret-path"));
            assert!(!serialized.contains(TEST_KEY));
            assert_eq!(body["provider"]["zdr"], true);
            assert_eq!(body["provider"]["data_collection"], "deny");
        }
        for id in &fixture.segment_ids[..OLDEST_SEGMENTS] {
            assert!(!first_primary.to_string().contains(&id.to_string()));
        }
        let repair_text = repair.to_string();
        assert!(repair_text.contains("{broken"));
        assert!(!repair_text.contains("verified recent context"));
        assert!(!repair_text.contains(TEST_KEY));
    }
}
