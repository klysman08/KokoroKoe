use std::sync::{Arc, atomic::AtomicBool};

use uuid::Uuid;

use crate::{
    domain::{
        AppError, AskManualQuestionRequest, ManualQuestionResponse, ManualQuestionUsage, RequestId,
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
pub(crate) struct ManualQuestionService {
    transcripts: TranscriptService,
    openrouter: OpenRouterService,
}

impl ManualQuestionService {
    pub(crate) fn new(transcripts: TranscriptService, openrouter: OpenRouterService) -> Self {
        Self {
            transcripts,
            openrouter,
        }
    }

    pub(crate) fn ask(
        &self,
        request: AskManualQuestionRequest,
    ) -> Result<ManualQuestionResponse, AppError> {
        request
            .validate()
            .map_err(AppError::manual_question_error)?;
        let context = self.transcripts.get_manual_question_context(
            request.project_id,
            request.session_id,
            request.selected_segment_id,
        )?;
        let envelope = build_prompt_context(PromptContextRequest {
            project: &context.project,
            session: &context.session,
            task: PromptTask::ManualQuestion {
                question: &request.question,
                selected_segment_id: request.selected_segment_id,
            },
            relevant_segments: std::slice::from_ref(&context.selected_segment),
            recent_segments: &context.neighboring_segments,
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
        let GeneratedResult::ManualAnswer(answer) = validated.result else {
            return Err(AppError::manual_question_error(
                "manual_question_response_invalid",
            ));
        };
        let related_segment_ids = answer
            .related_segment_ids
            .iter()
            .map(|value| Uuid::parse_str(value))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::manual_question_error("manual_question_response_invalid"))?;
        let final_usage = validated
            .repair_usage
            .as_ref()
            .unwrap_or(&validated.primary_usage);
        let response = ManualQuestionResponse {
            schema_version: 1,
            request_id: request.request_id,
            project_id: request.project_id,
            session_id: request.session_id,
            selected_segment_id: request.selected_segment_id,
            answer: answer.answer,
            related_segment_ids,
            limitations: answer.limitations,
            primary_attempts,
            repaired: validated.repaired,
            primary_usage: public_usage(&validated.primary_usage),
            repair_usage: validated.repair_usage.as_ref().map(public_usage),
            session_actual_cost_usd: final_usage.session_actual_cost_usd.clone(),
            available_budget_usd: final_usage.available_budget_usd.clone(),
        };
        response
            .validate()
            .map_err(AppError::manual_question_error)?;
        Ok(response)
    }
}

fn public_usage(usage: &UsageReconciliation) -> ManualQuestionUsage {
    ManualQuestionUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        actual_cost_usd: usage.actual_cost_usd.clone(),
    }
}

fn map_prompt_error(code: &str) -> AppError {
    let public = match code {
        "prompt_model_not_selected" => "manual_question_model_required",
        "prompt_context_budget_too_small" | "prompt_selected_segment_exceeds_budget" => {
            "manual_question_context_too_large"
        }
        "prompt_task_invalid" => "manual_question_invalid",
        _ => "manual_question_context_unavailable",
    };
    AppError::manual_question_error(public)
}

fn map_completion_error(error: CompletionError) -> AppError {
    let public = match error.code {
        "completion_credential_missing" | "completion_credential_invalid" => {
            "manual_question_credential_required"
        }
        "completion_authentication_failed" | "completion_permission_denied" => {
            "manual_question_authentication_failed"
        }
        "completion_payment_required" => "manual_question_payment_required",
        "completion_catalog_required" | "completion_model_not_available" => {
            "manual_question_catalog_required"
        }
        "completion_model_unsupported" => "manual_question_model_required",
        "completion_budget_exceeded" => "manual_question_budget_exceeded",
        "completion_rate_limited" => "manual_question_rate_limited",
        "completion_cancelled" => "manual_question_cancelled",
        "completion_provider_requirements_unavailable" => {
            "manual_question_provider_requirements_unavailable"
        }
        "completion_context_rejected" => "manual_question_context_too_large",
        "completion_request_rejected" => "manual_question_request_rejected",
        "completion_timeout" => "manual_question_timeout",
        "completion_network_unavailable" => "manual_question_network_unavailable",
        "completion_provider_unavailable"
        | "completion_provider_overloaded"
        | "completion_provider_failed" => "manual_question_provider_temporarily_unavailable",
        _ => "manual_question_provider_unavailable",
    };
    AppError::manual_question_error(public)
}

fn map_generated_error(error: GeneratedResultError) -> AppError {
    if let Some(completion) = error.completion_error {
        return map_completion_error(completion);
    }
    let public = match error.code {
        "generated_content_filtered" => "manual_question_content_filtered",
        _ => "manual_question_response_invalid",
    };
    AppError::manual_question_error(public)
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

    const TEST_KEY: &str = "manual-question-secret-canary";

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
            "name": "Question project",
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
        value["llmModels"] = json!({"manualQuestions":"example/text-model"});
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

    #[test]
    fn provider_requirement_failures_are_actionable_and_content_free() {
        let error = super::map_completion_error(CompletionError::new(
            "completion_provider_requirements_unavailable",
            crate::llm::completion::ReservationDisposition::Released,
        ));

        assert_eq!(
            error.code,
            "manual_question_provider_requirements_unavailable"
        );
        assert!(error.user_message.contains("choose another model"));
        assert_eq!(
            error.technical_detail.as_deref(),
            Some("manual_question_provider_requirements_unavailable")
        );

        let unavailable = super::map_completion_error(CompletionError::new(
            "completion_provider_unavailable",
            crate::llm::completion::ReservationDisposition::Released,
        ));
        assert_eq!(
            unavailable.code,
            "manual_question_provider_temporarily_unavailable"
        );
        assert!(unavailable.user_message.contains("choose another model"));
    }

    #[test]
    fn product_path_retries_repairs_and_never_sends_distant_transcript_text() {
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
        let segment_ids = (1..=11)
            .map(|index| Uuid::parse_str(&format!("60000000-0000-4000-8000-{index:012}")).unwrap())
            .collect::<Vec<_>>();
        for (index, segment_id) in segment_ids.iter().enumerate() {
            let text = match index {
                0 => "DISTANT-LEFT-SECRET-CANARY".to_owned(),
                10 => "DISTANT-RIGHT-SECRET-CANARY".to_owned(),
                _ => format!("verified nearby context {index}"),
            };
            journal
                .append(
                    &locator,
                    JournalAppend {
                        event_id: Uuid::new_v4(),
                        recorded_at: format!("2026-08-17T10:00:{index:02}Z"),
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

        let valid_answer = json!({
            "answer":"The selected context records the local-only decision.",
            "relatedSegmentIds":[segment_ids[5].to_string()],
            "limitations":["Only the bounded nearby context was supplied."]
        })
        .to_string();
        let (base_url, requests) = start_server(vec![
            ResponsePlan {
                status: 429,
                body: format!(r#"{{"error":{{"message":"{TEST_KEY}"}}}}"#),
            },
            completion_plan("{broken", 400, 20),
            completion_plan(&valid_answer, 100, 30),
        ]);
        let openrouter =
            OpenRouterService::for_completion_test(&base_url, TEST_KEY, Duration::from_secs(2));
        openrouter.seed_catalog_for_test(vec![model()]);
        let service =
            ManualQuestionService::new(TranscriptService::new(settings, app_data), openrouter);
        let request: AskManualQuestionRequest = serde_json::from_value(json!({
            "requestId":"70000000-0000-4000-8000-000000000001",
            "projectId":project.id,
            "sessionId":session.id,
            "selectedSegmentId":segment_ids[5],
            "question":"What decision was made here?"
        }))
        .unwrap();

        let response = service.ask(request).unwrap();

        assert_eq!(response.primary_attempts, 2);
        assert!(response.repaired);
        assert_eq!(response.related_segment_ids, [segment_ids[5]]);
        assert_eq!(response.primary_usage.input_tokens, 400);
        assert_eq!(response.repair_usage.unwrap().input_tokens, 100);
        let first_primary: serde_json::Value =
            serde_json::from_slice(&requests.recv_timeout(Duration::from_secs(1)).unwrap())
                .unwrap();
        let second_primary: serde_json::Value =
            serde_json::from_slice(&requests.recv_timeout(Duration::from_secs(1)).unwrap())
                .unwrap();
        let repair: serde_json::Value =
            serde_json::from_slice(&requests.recv_timeout(Duration::from_secs(1)).unwrap())
                .unwrap();
        for body in [&first_primary, &second_primary] {
            let serialized = body.to_string();
            assert!(serialized.contains("verified nearby context 1"));
            assert!(serialized.contains("verified nearby context 9"));
            assert!(!serialized.contains("DISTANT-LEFT-SECRET-CANARY"));
            assert!(!serialized.contains("DISTANT-RIGHT-SECRET-CANARY"));
            assert!(!serialized.contains("workspace-secret-path"));
            assert!(!serialized.contains("microphone-endpoint"));
            assert_eq!(body["provider"]["zdr"], true);
            assert_eq!(body["provider"]["data_collection"], "deny");
        }
        let repair_text = repair.to_string();
        assert!(repair_text.contains("{broken"));
        assert!(!repair_text.contains("verified nearby context"));
        assert!(!repair_text.contains(TEST_KEY));
    }
}
