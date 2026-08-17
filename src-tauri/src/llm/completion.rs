use std::{
    io::{BufRead, BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};

use crate::{
    domain::{OpenRouterDataCollection, RequestId, Session},
    prompts::PromptEnvelope,
};

use super::{
    budget::{FinalUsage, UsageReconciliation},
    openrouter::OpenRouterService,
};

const REQUEST_BODY_LIMIT: usize = 512 * 1024;
const RESPONSE_BODY_LIMIT: usize = 2 * 1024 * 1024;
const SSE_EVENT_LIMIT: usize = 256 * 1024;
const SSE_EVENT_COUNT_LIMIT: usize = 4_096;
const OUTPUT_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionFinishReason {
    Stop,
    Length,
    ContentFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReservationDisposition {
    NotReserved,
    Released,
    Retained,
    Reconciled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletionResult {
    pub(crate) content: String,
    pub(crate) finish_reason: CompletionFinishReason,
    pub(crate) usage: UsageReconciliation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompletionError {
    pub(crate) code: &'static str,
    pub(crate) reservation: ReservationDisposition,
    pub(crate) retry_after: Option<Duration>,
}

impl CompletionError {
    pub(super) const fn new(code: &'static str, reservation: ReservationDisposition) -> Self {
        Self {
            code,
            reservation,
            retry_after: None,
        }
    }

    const fn with_retry_after(mut self, retry_after: Option<Duration>) -> Self {
        self.retry_after = retry_after;
        self
    }
}

#[derive(Serialize)]
struct CompletionRequest<'a> {
    model: &'a str,
    messages: [Message<'a>; 2],
    max_tokens: u32,
    stream: bool,
    response_format: ResponseFormat<'a>,
    provider: ProviderRouting<'static>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct ResponseFormat<'a> {
    r#type: &'static str,
    json_schema: JsonSchema<'a>,
}

#[derive(Serialize)]
struct JsonSchema<'a> {
    name: &'a str,
    strict: bool,
    schema: &'a serde_json::Value,
}

#[derive(Serialize)]
struct ProviderRouting<'a> {
    zdr: bool,
    data_collection: &'a str,
    require_parameters: bool,
}

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<ProviderUsage>,
    #[serde(default)]
    error: Option<ProviderError>,
}

#[derive(Deserialize)]
struct StreamChoice {
    index: u32,
    #[serde(default)]
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
struct ProviderUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Deserialize)]
struct ProviderError {
    #[serde(default)]
    metadata: Option<ProviderErrorMetadata>,
}

#[derive(Deserialize)]
struct ProviderErrorMetadata {
    #[serde(default)]
    error_type: Option<String>,
}

struct ParsedStream {
    content: String,
    finish_reason: CompletionFinishReason,
    usage: ProviderUsage,
}

struct StreamFailure {
    code: &'static str,
    started: bool,
    usage: Option<ProviderUsage>,
}

impl OpenRouterService {
    pub(crate) fn stream_completion<F>(
        &self,
        request_id: RequestId,
        session: &Session,
        envelope: &PromptEnvelope,
        cancellation: &Arc<AtomicBool>,
        mut on_delta: F,
    ) -> Result<CompletionResult, CompletionError>
    where
        F: FnMut(&str),
    {
        if cancellation.load(Ordering::Acquire) {
            return Err(CompletionError::new(
                "completion_cancelled",
                ReservationDisposition::NotReserved,
            ));
        }

        let model = self
            .cached_model_for_pricing(&envelope.model_id)
            .map_err(|code| {
                CompletionError::new(map_usage_code(code), ReservationDisposition::NotReserved)
            })?;
        if !model.supports_streaming
            || !model.supports_structured_outputs
            || !model.zero_data_retention_available
            || model.data_collection != OpenRouterDataCollection::Deny
        {
            return Err(CompletionError::new(
                "completion_model_unsupported",
                ReservationDisposition::NotReserved,
            ));
        }

        let body = build_request_body(envelope)?;
        self.reserve_prompt_usage(request_id, session, envelope)
            .map_err(|error| {
                CompletionError::new(
                    map_usage_code(error.code),
                    ReservationDisposition::NotReserved,
                )
            })?;

        if cancellation.load(Ordering::Acquire) {
            return Err(self.release_error(
                request_id,
                session,
                "completion_cancelled",
                ReservationDisposition::Released,
            ));
        }

        let response = match self.send_completion(body) {
            Ok(response) => response,
            Err(send_error) => {
                let code = map_openrouter_code(&send_error.error.code);
                if !send_error.definitely_not_started {
                    return Err(CompletionError::new(code, ReservationDisposition::Retained));
                }
                return Err(self
                    .release_error(request_id, session, code, ReservationDisposition::Released)
                    .with_retry_after(send_error.retry_after));
            }
        };
        let is_event_stream = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/event-stream"));
        if !is_event_stream {
            return Err(CompletionError::new(
                "completion_response_invalid",
                ReservationDisposition::Retained,
            ));
        }

        match parse_stream(response, &envelope.model_id, cancellation, &mut on_delta) {
            Ok(parsed) => {
                let usage = self
                    .reconcile_prompt_usage(FinalUsage {
                        request_id,
                        session_id: session.id,
                        input_tokens: u32::try_from(parsed.usage.prompt_tokens).map_err(|_| {
                            CompletionError::new(
                                "completion_usage_invalid",
                                ReservationDisposition::Retained,
                            )
                        })?,
                        output_tokens: u32::try_from(parsed.usage.completion_tokens).map_err(
                            |_| {
                                CompletionError::new(
                                    "completion_usage_invalid",
                                    ReservationDisposition::Retained,
                                )
                            },
                        )?,
                    })
                    .map_err(|error| {
                        CompletionError::new(
                            map_usage_code(error.code),
                            ReservationDisposition::Retained,
                        )
                    })?;
                Ok(CompletionResult {
                    content: parsed.content,
                    finish_reason: parsed.finish_reason,
                    usage,
                })
            }
            Err(failure) => {
                if let Some(usage) = failure.usage
                    && let (Ok(input_tokens), Ok(output_tokens)) = (
                        u32::try_from(usage.prompt_tokens),
                        u32::try_from(usage.completion_tokens),
                    )
                    && self
                        .reconcile_prompt_usage(FinalUsage {
                            request_id,
                            session_id: session.id,
                            input_tokens,
                            output_tokens,
                        })
                        .is_ok()
                {
                    return Err(CompletionError::new(
                        failure.code,
                        ReservationDisposition::Reconciled,
                    ));
                }
                if failure.started {
                    Err(CompletionError::new(
                        failure.code,
                        ReservationDisposition::Retained,
                    ))
                } else {
                    Err(self.release_error(
                        request_id,
                        session,
                        failure.code,
                        ReservationDisposition::Released,
                    ))
                }
            }
        }
    }

    fn release_error(
        &self,
        request_id: RequestId,
        session: &Session,
        code: &'static str,
        released: ReservationDisposition,
    ) -> CompletionError {
        match self.release_prompt_usage(request_id, session.id) {
            Ok(_) => CompletionError::new(code, released),
            Err(error) => {
                CompletionError::new(map_usage_code(error.code), ReservationDisposition::Retained)
            }
        }
    }
}

fn build_request_body(envelope: &PromptEnvelope) -> Result<Vec<u8>, CompletionError> {
    let schema =
        serde_json::from_str::<serde_json::Value>(&envelope.output_schema).map_err(|_| {
            CompletionError::new(
                "completion_prompt_invalid",
                ReservationDisposition::NotReserved,
            )
        })?;
    let system = format!(
        "{}\n\n{}",
        envelope.system_instructions, envelope.task_instructions
    );
    let request = CompletionRequest {
        model: &envelope.model_id,
        messages: [
            Message {
                role: "system",
                content: &system,
            },
            Message {
                role: "user",
                content: &envelope.untrusted_context,
            },
        ],
        max_tokens: envelope.maximum_output_tokens,
        stream: true,
        response_format: ResponseFormat {
            r#type: "json_schema",
            json_schema: JsonSchema {
                name: envelope.specification.output_schema_id,
                strict: true,
                schema: &schema,
            },
        },
        provider: ProviderRouting {
            zdr: true,
            data_collection: "deny",
            require_parameters: true,
        },
    };
    let body = serde_json::to_vec(&request).map_err(|_| {
        CompletionError::new(
            "completion_prompt_invalid",
            ReservationDisposition::NotReserved,
        )
    })?;
    if body.len() > REQUEST_BODY_LIMIT {
        return Err(CompletionError::new(
            "completion_request_too_large",
            ReservationDisposition::NotReserved,
        ));
    }
    Ok(body)
}

fn parse_stream<R: Read, F: FnMut(&str)>(
    response: R,
    expected_model: &str,
    cancellation: &Arc<AtomicBool>,
    on_delta: &mut F,
) -> Result<ParsedStream, StreamFailure> {
    let mut reader = BufReader::new(response.take(RESPONSE_BODY_LIMIT as u64 + 1));
    let mut line = Vec::new();
    let mut event_data = Vec::new();
    let mut total = 0_usize;
    let mut events = 0_usize;
    let mut output = String::new();
    let mut finish_reason = None;
    let mut usage = None;
    // A successful completion response means the request reached the provider;
    // without final usage, releasing its reservation would undercount a
    // potentially billable generation even if no SSE data byte reaches us.
    let started = true;

    loop {
        if cancellation.load(Ordering::Acquire) {
            return Err(StreamFailure {
                code: "completion_cancelled",
                started,
                usage,
            });
        }
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|_| StreamFailure {
                code: "completion_network_unavailable",
                started,
                usage,
            })?;
        if read == 0 {
            return Err(StreamFailure {
                code: "completion_stream_incomplete",
                started,
                usage,
            });
        }
        total = total.saturating_add(read);
        if total > RESPONSE_BODY_LIMIT || line.len() > SSE_EVENT_LIMIT {
            return Err(StreamFailure {
                code: "completion_response_too_large",
                started,
                usage,
            });
        }
        if line.ends_with(b"\n") {
            line.pop();
        }
        if line.ends_with(b"\r") {
            line.pop();
        }
        if line.is_empty() {
            if event_data.is_empty() {
                continue;
            }
            events += 1;
            if events > SSE_EVENT_COUNT_LIMIT || event_data.len() > SSE_EVENT_LIMIT {
                return Err(StreamFailure {
                    code: "completion_response_too_large",
                    started,
                    usage,
                });
            }
            let event = std::str::from_utf8(&event_data).map_err(|_| StreamFailure {
                code: "completion_response_invalid",
                started,
                usage,
            })?;
            if event == "[DONE]" {
                let finish_reason = finish_reason.ok_or(StreamFailure {
                    code: "completion_stream_incomplete",
                    started,
                    usage,
                })?;
                let usage = usage.ok_or(StreamFailure {
                    code: "completion_usage_missing",
                    started,
                    usage,
                })?;
                return Ok(ParsedStream {
                    content: output,
                    finish_reason,
                    usage,
                });
            }
            process_chunk(
                event,
                expected_model,
                &mut output,
                &mut finish_reason,
                &mut usage,
                on_delta,
                started,
            )?;
            event_data.clear();
            continue;
        }
        if line.starts_with(b":") {
            continue;
        }
        let Some(value) = line.strip_prefix(b"data:") else {
            continue;
        };
        let value = value.strip_prefix(b" ").unwrap_or(value);
        if !event_data.is_empty() {
            event_data.push(b'\n');
        }
        event_data.extend_from_slice(value);
        if event_data.len() > SSE_EVENT_LIMIT {
            return Err(StreamFailure {
                code: "completion_response_too_large",
                started,
                usage,
            });
        }
    }
}

fn process_chunk<F: FnMut(&str)>(
    event: &str,
    expected_model: &str,
    output: &mut String,
    finish_reason: &mut Option<CompletionFinishReason>,
    usage: &mut Option<ProviderUsage>,
    on_delta: &mut F,
    started: bool,
) -> Result<(), StreamFailure> {
    let chunk: StreamChunk = serde_json::from_str(event).map_err(|_| StreamFailure {
        code: "completion_response_invalid",
        started,
        usage: *usage,
    })?;
    if let Some(error) = chunk.error {
        return Err(StreamFailure {
            code: map_provider_error(error.metadata.and_then(|value| value.error_type).as_deref()),
            started,
            usage: *usage,
        });
    }
    if chunk
        .model
        .as_deref()
        .is_some_and(|model| model != expected_model)
    {
        return Err(StreamFailure {
            code: "completion_model_mismatch",
            started,
            usage: *usage,
        });
    }
    if chunk.choices.len() > 1 {
        return Err(StreamFailure {
            code: "completion_response_invalid",
            started,
            usage: *usage,
        });
    }
    if let Some(choice) = chunk.choices.into_iter().next() {
        if choice.index != 0 {
            return Err(StreamFailure {
                code: "completion_response_invalid",
                started,
                usage: *usage,
            });
        }
        if let Some(delta) = choice.delta.content {
            if finish_reason.is_some() || output.len().saturating_add(delta.len()) > OUTPUT_LIMIT {
                return Err(StreamFailure {
                    code: if finish_reason.is_some() {
                        "completion_response_invalid"
                    } else {
                        "completion_output_too_large"
                    },
                    started,
                    usage: *usage,
                });
            }
            output.push_str(&delta);
            on_delta(&delta);
        }
        if let Some(reason) = choice.finish_reason {
            if finish_reason.is_some() {
                return Err(StreamFailure {
                    code: "completion_response_invalid",
                    started,
                    usage: *usage,
                });
            }
            *finish_reason = Some(match reason.as_str() {
                "stop" => CompletionFinishReason::Stop,
                "length" => CompletionFinishReason::Length,
                "content_filter" => CompletionFinishReason::ContentFilter,
                _ => {
                    return Err(StreamFailure {
                        code: "completion_finish_reason_invalid",
                        started,
                        usage: *usage,
                    });
                }
            });
        }
    }
    if let Some(final_usage) = chunk.usage {
        if usage.is_some()
            || final_usage
                .prompt_tokens
                .checked_add(final_usage.completion_tokens)
                != Some(final_usage.total_tokens)
            || final_usage.prompt_tokens > u64::from(u32::MAX)
            || final_usage.completion_tokens > u64::from(u32::MAX)
        {
            return Err(StreamFailure {
                code: "completion_usage_invalid",
                started,
                usage: *usage,
            });
        }
        *usage = Some(final_usage);
    }
    Ok(())
}

fn map_provider_error(error_type: Option<&str>) -> &'static str {
    match error_type {
        Some("rate_limit_exceeded") | Some("rate_limit_error") => "completion_rate_limited",
        Some("provider_overloaded") => "completion_provider_overloaded",
        Some("provider_unavailable") | Some("server") => "completion_provider_unavailable",
        Some("timeout") => "completion_timeout",
        Some("authentication_error") => "completion_authentication_failed",
        Some("insufficient_credits") => "completion_payment_required",
        Some("context_length_exceeded") => "completion_context_rejected",
        _ => "completion_provider_failed",
    }
}

fn map_openrouter_code(code: &str) -> &'static str {
    match code {
        "openrouter_credential_missing" => "completion_credential_missing",
        "openrouter_credential_invalid" | "openrouter_authentication_failed" => {
            "completion_authentication_failed"
        }
        "openrouter_permission_denied" => "completion_permission_denied",
        "openrouter_payment_required" => "completion_payment_required",
        "openrouter_rate_limited" => "completion_rate_limited",
        "openrouter_timeout" => "completion_timeout",
        "openrouter_network_unavailable" => "completion_network_unavailable",
        "openrouter_request_rejected" => "completion_request_rejected",
        "openrouter_service_unavailable" => "completion_provider_unavailable",
        _ => "completion_provider_unavailable",
    }
}

fn map_usage_code(code: &str) -> &'static str {
    match code {
        "usage_catalog_unavailable" => "completion_catalog_unavailable",
        "usage_catalog_required" => "completion_catalog_required",
        "usage_model_not_available" => "completion_model_not_available",
        "usage_session_limit_exceeded" => "completion_budget_exceeded",
        "usage_request_duplicate" => "completion_request_duplicate",
        "usage_final_tokens_exceed_reservation" => "completion_usage_exceeds_reservation",
        "usage_reservation_not_found" => "completion_reservation_not_found",
        _ => "completion_budget_unavailable",
    }
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

    use crate::{
        domain::{OpenRouterModel, Session},
        prompts::{PromptEnvelope, PromptFallbackStrategy, PromptPurpose, prompt_specifications},
    };

    use super::*;

    const TEST_KEY: &str = "completion-secret-canary";

    struct ResponsePlan {
        status: u16,
        content_type: &'static str,
        chunks: Vec<Vec<u8>>,
    }

    struct CapturedRequest {
        head: String,
        body: Vec<u8>,
    }

    fn start_server(plans: Vec<ResponsePlan>) -> (String, mpsc::Receiver<CapturedRequest>) {
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
                    if read == 0 {
                        return;
                    }
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
                    if read == 0 {
                        return;
                    }
                    request.extend_from_slice(&buffer[..read]);
                }
                sender
                    .send(CapturedRequest {
                        head,
                        body: request[head_end..head_end + content_length].to_vec(),
                    })
                    .unwrap();

                let body_length = plan.chunks.iter().map(Vec::len).sum::<usize>();
                let reason = if plan.status == 200 { "OK" } else { "Error" };
                write!(
                    stream,
                    "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    plan.status, reason, plan.content_type, body_length
                )
                .unwrap();
                for chunk in plan.chunks {
                    if stream.write_all(&chunk).is_err() {
                        break;
                    }
                    stream.flush().ok();
                    thread::sleep(Duration::from_millis(5));
                }
            }
        });
        (format!("http://{address}"), receiver)
    }

    fn session() -> Session {
        serde_json::from_value(json!({
            "schemaVersion": 1,
            "id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "projectId": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "folderName": "2026-08-15-completion-test--cccccccc",
            "title": "Completion test",
            "objective": "Verify private streaming.",
            "sessionContext": "",
            "preset": {
                "id": "00000000-0000-4000-8000-000000000001",
                "version": 1,
                "name": "Technical interview",
                "assistantRole": "Track decisions.",
                "analysisObjectives": ["Track decisions"],
                "insightTypes": ["decision"],
                "responseTone": "Concise",
                "finalSummarySections": ["Decisions"],
                "highlightInstructions": [],
                "prohibitedBehaviors": []
            },
            "language": "en-GB",
            "microphone": {
                "endpointId": "microphone-endpoint",
                "friendlyName": "Microphone",
                "selection": {"kind": "fixed", "endpointId": "microphone-endpoint"},
                "nativeSampleRate": 48000,
                "nativeChannels": 1
            },
            "systemOutput": {
                "endpointId": "render-endpoint",
                "friendlyName": "Speakers",
                "selection": {"kind": "default", "role": "console"},
                "nativeSampleRate": 48000,
                "nativeChannels": 2
            },
            "transcriptionEngine": "whisper",
            "transcriptionModelId": "whisper-tiny-multilingual",
            "llmModels": {"insights": "example/text-model"},
            "spendingLimitUsd": "1.00",
            "maxTokensPerRequest": 4096,
            "retainAudio": false,
            "state": "idle",
            "channelHealth": {
                "microphone": {"status": "stopped", "updatedAt": "2026-08-15T10:00:00Z"},
                "systemOutput": {"status": "stopped", "updatedAt": "2026-08-15T10:00:00Z"}
            },
            "summaryStatus": "not_requested",
            "usage": {
                "inputTokens": 0,
                "outputTokens": 0,
                "estimatedCostUsd": "0.00",
                "actualCostUsd": "0.00"
            },
            "createdAt": "2026-08-15T10:00:00Z",
            "updatedAt": "2026-08-15T10:00:00Z",
            "revision": 1
        }))
        .unwrap()
    }

    fn envelope() -> PromptEnvelope {
        let specification = prompt_specifications()
            .iter()
            .find(|specification| specification.purpose == PromptPurpose::Insight)
            .unwrap();
        PromptEnvelope {
            specification,
            model_id: "example/text-model".to_owned(),
            request_token_limit: 1_536,
            input_token_limit: 1_024,
            maximum_output_tokens: 512,
            estimated_input_tokens: 64,
            selected_segment_ids: Vec::new(),
            omitted_relevant_segments: 0,
            omitted_recent_segments: 0,
            system_instructions: "Fixed system instructions.",
            task_instructions: "Return the insight object.",
            untrusted_context: r#"{"transcript":"hostile text"}"#.to_owned(),
            output_schema: r#"{"type":"object","additionalProperties":false,"required":["insights"],"properties":{"insights":{"type":"array"}}}"#.to_owned(),
            fallback_strategy: PromptFallbackStrategy::SingleJsonRepairThenFail,
        }
    }

    fn model() -> OpenRouterModel {
        OpenRouterModel {
            id: "example/text-model".to_owned(),
            name: "Example".to_owned(),
            provider: "example".to_owned(),
            context_length: 4_096,
            prompt_price_per_token: "0.000001".to_owned(),
            completion_price_per_token: "0.000002".to_owned(),
            supports_structured_outputs: true,
            supports_streaming: true,
            zero_data_retention_available: true,
            data_collection: OpenRouterDataCollection::Deny,
        }
    }

    fn success_plan() -> ResponsePlan {
        let body = concat!(
            ": OPENROUTER PROCESSING\r\n\r\n",
            "data: {\"model\":\"example/text-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"{\\\"insights\\\":\"},\"finish_reason\":null}]}\r\n\r\n",
            "data: {\"model\":\"example/text-model\",\n",
            "data: \"choices\":[{\"index\":0,\"delta\":{\"content\":\"[]}\"},\"finish_reason\":\"stop\"}]}\r\n\r\n",
            "data: {\"model\":\"example/text-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":64,\"completion_tokens\":8,\"total_tokens\":72}}\r\n\r\n",
            "data: [DONE]\r\n\r\n"
        )
        .as_bytes();
        let split = body.len() / 3;
        ResponsePlan {
            status: 200,
            content_type: "text/event-stream; charset=utf-8",
            chunks: vec![
                body[..split].to_vec(),
                body[split..split + 7].to_vec(),
                body[split + 7..].to_vec(),
            ],
        }
    }

    fn service(base_url: &str) -> OpenRouterService {
        let service =
            OpenRouterService::for_completion_test(base_url, TEST_KEY, Duration::from_secs(2));
        service.seed_catalog_for_test(vec![model()]);
        service
    }

    #[test]
    fn one_private_structured_post_streams_and_reconciles_final_usage() {
        let (base_url, requests) = start_server(vec![success_plan()]);
        let service = service(&base_url);
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut deltas = Vec::new();

        let result = service
            .stream_completion(
                RequestId::new(),
                &session(),
                &envelope(),
                &cancellation,
                |delta| deltas.push(delta.to_owned()),
            )
            .unwrap();

        assert_eq!(result.content, r#"{"insights":[]}"#);
        assert_eq!(deltas, [r#"{"insights":"#, "[]}"]);
        assert_eq!(result.finish_reason, CompletionFinishReason::Stop);
        assert_eq!(result.usage.input_tokens, 64);
        assert_eq!(result.usage.output_tokens, 8);

        let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            request
                .head
                .starts_with("POST /api/v1/chat/completions HTTP/1.1\r\n")
        );
        assert!(
            request
                .head
                .to_ascii_lowercase()
                .contains("accept: text/event-stream\r\n")
        );
        assert!(
            request
                .head
                .to_ascii_lowercase()
                .contains("content-type: application/json\r\n")
        );
        assert!(
            request
                .head
                .contains(&format!("authorization: Bearer {TEST_KEY}\r\n"))
        );
        let value: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(
            value,
            json!({
                "model": "example/text-model",
                "messages": [
                    {"role":"system","content":"Fixed system instructions.\n\nReturn the insight object."},
                    {"role":"user","content":"{\"transcript\":\"hostile text\"}"}
                ],
                "max_tokens": 512,
                "stream": true,
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "insight_batch_v1",
                        "strict": true,
                        "schema": {
                            "type":"object",
                            "additionalProperties":false,
                            "required":["insights"],
                            "properties":{"insights":{"type":"array"}}
                        }
                    }
                },
                "provider": {"zdr":true,"data_collection":"deny","require_parameters":true}
            })
        );
        for forbidden in ["audio", "tools", "plugins", "metadata", "debug"] {
            assert!(value.get(forbidden).is_none());
        }
    }

    #[test]
    fn cancellation_before_reservation_uses_no_network_and_after_stream_retains_budget() {
        let (base_url, requests) = start_server(vec![success_plan()]);
        let service = service(&base_url);
        let session = session();
        let envelope = envelope();
        let request_id = RequestId::new();
        let pre_cancelled = Arc::new(AtomicBool::new(true));
        let error = service
            .stream_completion(request_id, &session, &envelope, &pre_cancelled, |_| {})
            .unwrap_err();
        assert_eq!(error.code, "completion_cancelled");
        assert_eq!(error.reservation, ReservationDisposition::NotReserved);
        assert!(requests.recv_timeout(Duration::from_millis(50)).is_err());

        let cancellation = Arc::new(AtomicBool::new(false));
        let signal = cancellation.clone();
        let error = service
            .stream_completion(request_id, &session, &envelope, &cancellation, |_| {
                signal.store(true, Ordering::Release)
            })
            .unwrap_err();
        assert_eq!(error.code, "completion_cancelled");
        assert_eq!(error.reservation, ReservationDisposition::Retained);
        requests.recv_timeout(Duration::from_secs(1)).unwrap();

        let duplicate = service
            .stream_completion(
                request_id,
                &session,
                &envelope,
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_err();
        assert_eq!(duplicate.code, "completion_request_duplicate");
        assert_eq!(duplicate.reservation, ReservationDisposition::NotReserved);
    }

    #[test]
    fn pre_stream_http_failure_releases_and_does_not_retry() {
        let (base_url, requests) = start_server(vec![
            ResponsePlan {
                status: 429,
                content_type: "application/json",
                chunks: vec![format!(r#"{{"error":"{TEST_KEY}"}}"#).into_bytes()],
            },
            success_plan(),
        ]);
        let service = service(&base_url);
        let request_id = RequestId::new();
        let error = service
            .stream_completion(
                request_id,
                &session(),
                &envelope(),
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_err();
        assert_eq!(error.code, "completion_rate_limited");
        assert_eq!(error.reservation, ReservationDisposition::Released);
        requests.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(requests.recv_timeout(Duration::from_millis(50)).is_err());

        service
            .stream_completion(
                request_id,
                &session(),
                &envelope(),
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap();
        requests.recv_timeout(Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn in_stream_error_and_invalid_usage_keep_content_and_provider_details_private() {
        let provider_error = format!(
            "data: {{\"error\":{{\"message\":\"{TEST_KEY}\",\"metadata\":{{\"error_type\":\"rate_limit_exceeded\"}}}}}}\n\n"
        );
        let invalid_usage = concat!(
            "data: {\"model\":\"example/text-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"model\":\"example/text-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":1025,\"completion_tokens\":1,\"total_tokens\":1026}}\n\n",
            "data: [DONE]\n\n"
        );
        let (base_url, requests) = start_server(vec![
            ResponsePlan {
                status: 200,
                content_type: "text/event-stream",
                chunks: vec![provider_error.into_bytes()],
            },
            ResponsePlan {
                status: 200,
                content_type: "text/event-stream",
                chunks: vec![invalid_usage.as_bytes().to_vec()],
            },
        ]);
        let service = service(&base_url);

        let error = service
            .stream_completion(
                RequestId::new(),
                &session(),
                &envelope(),
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_err();
        assert_eq!(error.code, "completion_rate_limited");
        assert_eq!(error.reservation, ReservationDisposition::Retained);
        assert!(!format!("{error:?}").contains(TEST_KEY));

        let error = service
            .stream_completion(
                RequestId::new(),
                &session(),
                &envelope(),
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_err();
        assert_eq!(error.code, "completion_usage_exceeds_reservation");
        assert_eq!(error.reservation, ReservationDisposition::Retained);
        requests.recv_timeout(Duration::from_secs(1)).unwrap();
        requests.recv_timeout(Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn unsupported_models_and_oversized_requests_stop_before_reservation() {
        let service = OpenRouterService::for_completion_test(
            "http://127.0.0.1:9",
            TEST_KEY,
            Duration::from_millis(50),
        );
        let mut unsupported = model();
        unsupported.supports_structured_outputs = false;
        service.seed_catalog_for_test(vec![unsupported]);
        let error = service
            .stream_completion(
                RequestId::new(),
                &session(),
                &envelope(),
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_err();
        assert_eq!(error.code, "completion_model_unsupported");
        assert_eq!(error.reservation, ReservationDisposition::NotReserved);

        service.seed_catalog_for_test(vec![model()]);
        let mut oversized = envelope();
        oversized.untrusted_context = "x".repeat(REQUEST_BODY_LIMIT);
        let request_id = RequestId::new();
        let error = service
            .stream_completion(
                request_id,
                &session(),
                &oversized,
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_err();
        assert_eq!(error.code, "completion_request_too_large");
        assert_eq!(error.reservation, ReservationDisposition::NotReserved);
    }

    #[test]
    fn malformed_mismatched_and_oversized_streams_fail_closed() {
        let oversized_event = format!("data: {}\n\n", "x".repeat(SSE_EVENT_LIMIT + 1));
        let (base_url, requests) = start_server(vec![
            ResponsePlan {
                status: 200,
                content_type: "text/event-stream",
                chunks: vec![b"data: not-json\n\n".to_vec()],
            },
            ResponsePlan {
                status: 200,
                content_type: "text/event-stream",
                chunks: vec![b"data: {\"model\":\"other/model\",\"choices\":[]}\n\n".to_vec()],
            },
            ResponsePlan {
                status: 200,
                content_type: "text/event-stream",
                chunks: vec![oversized_event.into_bytes()],
            },
        ]);
        let service = service(&base_url);

        for expected in [
            "completion_response_invalid",
            "completion_model_mismatch",
            "completion_response_too_large",
        ] {
            let error = service
                .stream_completion(
                    RequestId::new(),
                    &session(),
                    &envelope(),
                    &Arc::new(AtomicBool::new(false)),
                    |_| {},
                )
                .unwrap_err();
            assert_eq!(error.code, expected);
            assert_eq!(error.reservation, ReservationDisposition::Retained);
            requests.recv_timeout(Duration::from_secs(1)).unwrap();
        }
    }

    #[test]
    fn ambiguous_network_failure_retains_the_conservative_reservation() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let service = service(&base_url);
        let request_id = RequestId::new();
        let error = service
            .stream_completion(
                request_id,
                &session(),
                &envelope(),
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_err();
        assert!(matches!(
            error.code,
            "completion_network_unavailable" | "completion_timeout"
        ));
        assert_eq!(error.reservation, ReservationDisposition::Retained);

        let duplicate = service
            .stream_completion(
                request_id,
                &session(),
                &envelope(),
                &Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_err();
        assert_eq!(duplicate.code, "completion_request_duplicate");
    }
}
