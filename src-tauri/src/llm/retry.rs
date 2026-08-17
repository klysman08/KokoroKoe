use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    domain::{RequestId, Session},
    prompts::PromptEnvelope,
};

use super::{
    completion::{CompletionError, CompletionResult, ReservationDisposition},
    openrouter::OpenRouterService,
};

const MAX_ATTEMPTS: u8 = 3;
const FALLBACK_BACKOFF: [Duration; 2] = [Duration::from_millis(250), Duration::from_secs(1)];
const CANCELLATION_POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetriedCompletion {
    pub(crate) completion: CompletionResult,
    pub(crate) attempts: u8,
}

trait RetryRuntime {
    fn next_request_id(&self) -> RequestId;
    fn wait(&self, duration: Duration, cancellation: &Arc<AtomicBool>) -> bool;
}

struct SystemRetryRuntime;

impl RetryRuntime for SystemRetryRuntime {
    fn next_request_id(&self) -> RequestId {
        RequestId::new()
    }

    fn wait(&self, duration: Duration, cancellation: &Arc<AtomicBool>) -> bool {
        let deadline = Instant::now() + duration;
        loop {
            if cancellation.load(Ordering::Acquire) {
                return false;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return true;
            }
            thread::sleep(remaining.min(CANCELLATION_POLL));
        }
    }
}

impl OpenRouterService {
    pub(crate) fn complete_with_retry(
        &self,
        request_id: RequestId,
        session: &Session,
        envelope: &PromptEnvelope,
        cancellation: &Arc<AtomicBool>,
    ) -> Result<RetriedCompletion, CompletionError> {
        self.complete_with_retry_using(
            request_id,
            session,
            envelope,
            cancellation,
            &SystemRetryRuntime,
        )
    }

    fn complete_with_retry_using<R: RetryRuntime>(
        &self,
        request_id: RequestId,
        session: &Session,
        envelope: &PromptEnvelope,
        cancellation: &Arc<AtomicBool>,
        runtime: &R,
    ) -> Result<RetriedCompletion, CompletionError> {
        let mut attempt_request_id = request_id;
        let mut used_request_ids = Vec::with_capacity(usize::from(MAX_ATTEMPTS));

        for attempt in 1..=MAX_ATTEMPTS {
            used_request_ids.push(attempt_request_id);
            match self.stream_completion(
                attempt_request_id,
                session,
                envelope,
                cancellation,
                |_| {},
            ) {
                Ok(completion) => {
                    return Ok(RetriedCompletion {
                        completion,
                        attempts: attempt,
                    });
                }
                Err(error) => {
                    if attempt == MAX_ATTEMPTS || !should_retry(error) {
                        return Err(error);
                    }
                    let delay = error
                        .retry_after
                        .unwrap_or(FALLBACK_BACKOFF[usize::from(attempt - 1)]);
                    if !runtime.wait(delay, cancellation) {
                        return Err(CompletionError::new(
                            "completion_cancelled",
                            ReservationDisposition::NotReserved,
                        ));
                    }
                    let next_request_id = runtime.next_request_id();
                    if used_request_ids.contains(&next_request_id) {
                        return Err(CompletionError::new(
                            "completion_request_duplicate",
                            ReservationDisposition::NotReserved,
                        ));
                    }
                    attempt_request_id = next_request_id;
                }
            }
        }

        unreachable!("the bounded retry loop always returns")
    }
}

fn should_retry(error: CompletionError) -> bool {
    matches!(
        error.reservation,
        ReservationDisposition::Released | ReservationDisposition::Reconciled
    ) && matches!(
        error.code,
        "completion_rate_limited"
            | "completion_provider_overloaded"
            | "completion_provider_unavailable"
            | "completion_timeout"
            | "completion_network_unavailable"
    )
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        io::{Read as _, Write as _},
        net::TcpListener,
        sync::{Mutex, mpsc},
    };

    use serde_json::json;

    use crate::{
        domain::{OpenRouterDataCollection, OpenRouterModel},
        prompts::{PromptFallbackStrategy, PromptPurpose, prompt_specifications},
    };

    use super::*;

    const TEST_KEY: &str = "retry-secret-canary";

    struct ResponsePlan {
        status: u16,
        headers: Vec<(&'static str, &'static str)>,
        body: &'static str,
    }

    struct FakeRuntime {
        ids: Mutex<VecDeque<RequestId>>,
        waits: Mutex<Vec<Duration>>,
        cancel_on_wait: bool,
    }

    impl FakeRuntime {
        fn new(ids: Vec<RequestId>, cancel_on_wait: bool) -> Self {
            Self {
                ids: Mutex::new(ids.into()),
                waits: Mutex::new(Vec::new()),
                cancel_on_wait,
            }
        }

        fn waits(&self) -> Vec<Duration> {
            self.waits.lock().unwrap().clone()
        }
    }

    impl RetryRuntime for FakeRuntime {
        fn next_request_id(&self) -> RequestId {
            self.ids
                .lock()
                .unwrap()
                .pop_front()
                .expect("test runtime should provide every retry ID")
        }

        fn wait(&self, duration: Duration, cancellation: &Arc<AtomicBool>) -> bool {
            self.waits.lock().unwrap().push(duration);
            if self.cancel_on_wait {
                cancellation.store(true, Ordering::Release);
                false
            } else {
                true
            }
        }
    }

    fn start_server(plans: Vec<ResponsePlan>) -> (String, mpsc::Receiver<()>) {
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
                sender.send(()).unwrap();

                let reason = if plan.status == 200 { "OK" } else { "Error" };
                write!(stream, "HTTP/1.1 {} {}\r\n", plan.status, reason).unwrap();
                for (name, value) in plan.headers {
                    write!(stream, "{name}: {value}\r\n").unwrap();
                }
                write!(
                    stream,
                    "Content-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    plan.body.len(),
                    plan.body
                )
                .unwrap();
            }
        });
        (format!("http://{address}"), receiver)
    }

    fn request_id(value: &str) -> RequestId {
        serde_json::from_value(json!(value)).unwrap()
    }

    fn session() -> Session {
        serde_json::from_value(json!({
            "schemaVersion": 1,
            "id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "projectId": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "folderName": "2026-08-17-retry-test--cccccccc",
            "title": "Retry test",
            "objective": "Verify bounded transient retries.",
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
                "microphone": {"status": "stopped", "updatedAt": "2026-08-17T10:00:00Z"},
                "systemOutput": {"status": "stopped", "updatedAt": "2026-08-17T10:00:00Z"}
            },
            "summaryStatus": "not_requested",
            "usage": {
                "inputTokens": 0,
                "outputTokens": 0,
                "estimatedCostUsd": "0.00",
                "actualCostUsd": "0.00"
            },
            "createdAt": "2026-08-17T10:00:00Z",
            "updatedAt": "2026-08-17T10:00:00Z",
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
            prompt_price_per_token: "0.00001".to_owned(),
            completion_price_per_token: "0.00002".to_owned(),
            supports_structured_outputs: true,
            supports_streaming: true,
            zero_data_retention_available: true,
            data_collection: OpenRouterDataCollection::Deny,
        }
    }

    fn service(base_url: &str) -> OpenRouterService {
        let service =
            OpenRouterService::for_completion_test(base_url, TEST_KEY, Duration::from_secs(2));
        service.seed_catalog_for_test(vec![model()]);
        service
    }

    fn success_plan() -> ResponsePlan {
        ResponsePlan {
            status: 200,
            headers: Vec::new(),
            body: concat!(
                "data: {\"model\":\"example/text-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"{\\\"insights\\\":[]}\"},\"finish_reason\":\"stop\"}]}\n\n",
                "data: {\"model\":\"example/text-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":64,\"completion_tokens\":8,\"total_tokens\":72}}\n\n",
                "data: [DONE]\n\n"
            ),
        }
    }

    fn error_plan(status: u16, retry_after: Option<&'static str>) -> ResponsePlan {
        ResponsePlan {
            status,
            headers: retry_after
                .map(|value| vec![("Retry-After", value)])
                .unwrap_or_default(),
            body: r#"{"error":{"code":429,"message":"secret provider detail"}}"#,
        }
    }

    #[test]
    fn retry_after_and_fallback_use_fresh_ids_before_success() {
        let (base_url, requests) = start_server(vec![
            error_plan(429, Some("999")),
            error_plan(503, Some("not-seconds")),
            success_plan(),
        ]);
        let service = service(&base_url);
        let initial = request_id("10000000-0000-4000-8000-000000000001");
        let second = request_id("10000000-0000-4000-8000-000000000002");
        let third = request_id("10000000-0000-4000-8000-000000000003");
        let runtime = FakeRuntime::new(vec![second, third], false);
        let cancellation = Arc::new(AtomicBool::new(false));

        let result = service
            .complete_with_retry_using(initial, &session(), &envelope(), &cancellation, &runtime)
            .unwrap();

        assert_eq!(result.attempts, 3);
        assert_eq!(result.completion.usage.request_id, third);
        assert_eq!(result.completion.content, r#"{"insights":[]}"#);
        assert_eq!(
            runtime.waits(),
            [Duration::from_secs(30), Duration::from_secs(1)]
        );
        for _ in 0..3 {
            requests.recv_timeout(Duration::from_secs(1)).unwrap();
        }
    }

    #[test]
    fn three_transient_failures_exhaust_without_a_fourth_attempt() {
        let (base_url, requests) = start_server(vec![
            error_plan(429, None),
            error_plan(429, None),
            error_plan(429, None),
        ]);
        let service = service(&base_url);
        let runtime = FakeRuntime::new(
            vec![
                request_id("20000000-0000-4000-8000-000000000002"),
                request_id("20000000-0000-4000-8000-000000000003"),
            ],
            false,
        );
        let cancellation = Arc::new(AtomicBool::new(false));

        let error = service
            .complete_with_retry_using(
                request_id("20000000-0000-4000-8000-000000000001"),
                &session(),
                &envelope(),
                &cancellation,
                &runtime,
            )
            .unwrap_err();

        assert_eq!(error.code, "completion_rate_limited");
        assert_eq!(error.reservation, ReservationDisposition::Released);
        assert_eq!(runtime.waits(), FALLBACK_BACKOFF);
        for _ in 0..3 {
            requests.recv_timeout(Duration::from_secs(1)).unwrap();
        }
    }

    #[test]
    fn cancellation_during_backoff_stops_before_another_reservation() {
        let (base_url, requests) = start_server(vec![error_plan(429, None)]);
        let service = service(&base_url);
        let runtime = FakeRuntime::new(
            vec![request_id("30000000-0000-4000-8000-000000000002")],
            true,
        );
        let cancellation = Arc::new(AtomicBool::new(false));

        let error = service
            .complete_with_retry_using(
                request_id("30000000-0000-4000-8000-000000000001"),
                &session(),
                &envelope(),
                &cancellation,
                &runtime,
            )
            .unwrap_err();

        assert_eq!(error.code, "completion_cancelled");
        assert_eq!(error.reservation, ReservationDisposition::NotReserved);
        assert_eq!(runtime.waits(), [Duration::from_millis(250)]);
        requests.recv_timeout(Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn reconciled_transient_usage_can_block_the_next_attempt_at_budget_admission() {
        let reconciled_overload = ResponsePlan {
            status: 200,
            headers: Vec::new(),
            body: concat!(
                "data: {\"model\":\"example/text-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":1000,\"completion_tokens\":500,\"total_tokens\":1500}}\n\n",
                "data: {\"error\":{\"metadata\":{\"error_type\":\"provider_overloaded\"}}}\n\n"
            ),
        };
        let (base_url, requests) = start_server(vec![reconciled_overload]);
        let service = service(&base_url);
        let runtime = FakeRuntime::new(
            vec![request_id("40000000-0000-4000-8000-000000000002")],
            false,
        );
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut budget_session = session();
        budget_session.spending_limit_usd = "0.03".to_owned();

        let error = service
            .complete_with_retry_using(
                request_id("40000000-0000-4000-8000-000000000001"),
                &budget_session,
                &envelope(),
                &cancellation,
                &runtime,
            )
            .unwrap_err();

        assert_eq!(error.code, "completion_budget_exceeded");
        assert_eq!(error.reservation, ReservationDisposition::NotReserved);
        assert_eq!(runtime.waits(), [Duration::from_millis(250)]);
        requests.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(requests.recv_timeout(Duration::from_millis(50)).is_err());
    }

    #[test]
    fn nonretryable_retained_and_duplicate_id_failures_stop_immediately() {
        assert!(!should_retry(CompletionError::new(
            "completion_authentication_failed",
            ReservationDisposition::Released,
        )));
        assert!(!should_retry(CompletionError::new(
            "completion_rate_limited",
            ReservationDisposition::Retained,
        )));

        let (base_url, requests) = start_server(vec![error_plan(429, None)]);
        let service = service(&base_url);
        let initial = request_id("50000000-0000-4000-8000-000000000001");
        let runtime = FakeRuntime::new(vec![initial], false);
        let cancellation = Arc::new(AtomicBool::new(false));
        let error = service
            .complete_with_retry_using(initial, &session(), &envelope(), &cancellation, &runtime)
            .unwrap_err();

        assert_eq!(error.code, "completion_request_duplicate");
        assert_eq!(error.reservation, ReservationDisposition::NotReserved);
        requests.recv_timeout(Duration::from_secs(1)).unwrap();
    }
}
