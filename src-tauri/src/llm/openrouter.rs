use std::{
    io::Read,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use reqwest::{
    StatusCode,
    blocking::{Client, Response},
    header::{ACCEPT, AUTHORIZATION, HeaderValue, USER_AGENT},
};
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    domain::{
        AppError, CredentialValidation, LlmRoleModels, OpenRouterApiKey, OpenRouterDataCollection,
        OpenRouterModel,
    },
    security::CredentialService,
};

use super::budget::UsageBudgetService;

const OPENROUTER_BASE_URL: &str = "https://openrouter.ai";
const VALIDATION_RESPONSE_LIMIT: usize = 64 * 1024;
const MODEL_RESPONSE_LIMIT: usize = 2 * 1024 * 1024;
const MODEL_LIMIT: usize = 500;
const CATALOG_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> Result<String, AppError>;
}

struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> Result<String, AppError> {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|_| AppError::openrouter_error("openrouter_clock_unavailable"))
    }
}

#[derive(Clone)]
pub(crate) struct OpenRouterService {
    credentials: CredentialService,
    client: Client,
    base_url: String,
    clock: Arc<dyn Clock>,
    catalog: Arc<Mutex<Option<CachedCatalog>>>,
    pub(super) usage_budget: UsageBudgetService,
}

#[derive(Clone)]
struct CachedCatalog {
    stored_at: Instant,
    models: Vec<OpenRouterModel>,
}

impl OpenRouterService {
    pub(crate) fn open(credentials: CredentialService) -> Result<Self, AppError> {
        Self::with_configuration(
            credentials,
            OPENROUTER_BASE_URL,
            Duration::from_secs(5),
            Duration::from_secs(20),
            Arc::new(SystemClock),
        )
    }

    fn with_configuration(
        credentials: CredentialService,
        base_url: &str,
        connect_timeout: Duration,
        request_timeout: Duration,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AppError> {
        let client = Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AppError::openrouter_error("openrouter_client_unavailable"))?;
        Ok(Self {
            credentials,
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            clock,
            catalog: Arc::new(Mutex::new(None)),
            usage_budget: UsageBudgetService::default(),
        })
    }

    pub(crate) fn validate_credential(&self) -> Result<CredentialValidation, AppError> {
        let api_key = self.credentials.load_api_key()?;
        let response = self.send_get("/api/v1/key", &api_key)?;
        let body = read_success_body(response, VALIDATION_RESPONSE_LIMIT)?;
        let envelope: KeyEnvelope = serde_json::from_slice(&body)
            .map_err(|_| AppError::openrouter_error("openrouter_response_invalid"))?;
        if !envelope.data.is_object() {
            return Err(AppError::openrouter_error("openrouter_response_invalid"));
        }
        let validated_at = self.clock.now_rfc3339()?;
        let result = CredentialValidation::valid_at(validated_at.clone())
            .map_err(AppError::openrouter_error)?;
        self.credentials.record_validation(&validated_at)?;
        Ok(result)
    }

    pub(crate) fn list_models(
        &self,
        force_refresh: bool,
    ) -> Result<Vec<OpenRouterModel>, AppError> {
        if !force_refresh {
            let cache = self
                .catalog
                .lock()
                .map_err(|_| AppError::openrouter_error("openrouter_catalog_unavailable"))?;
            if let Some(cached) = cache.as_ref()
                && cached.stored_at.elapsed() < CATALOG_CACHE_TTL
            {
                return Ok(cached.models.clone());
            }
        }

        let api_key = self.credentials.load_api_key()?;
        let path = format!(
            "/api/v1/models?limit={MODEL_LIMIT}&input_modalities=text&output_modalities=text&zdr=true"
        );
        let response = self.send_get(&path, &api_key)?;
        let body = read_success_body(response, MODEL_RESPONSE_LIMIT)?;
        let envelope: ModelEnvelope = serde_json::from_slice(&body)
            .map_err(|_| AppError::openrouter_error("openrouter_model_catalog_invalid"))?;
        if envelope.data.len() > MODEL_LIMIT {
            return Err(AppError::openrouter_error(
                "openrouter_model_catalog_invalid",
            ));
        }

        let mut models = envelope
            .data
            .into_iter()
            .map(OpenRouterModel::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        models.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        for pair in models.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(AppError::openrouter_error(
                    "openrouter_model_catalog_invalid",
                ));
            }
        }
        let mut cache = self
            .catalog
            .lock()
            .map_err(|_| AppError::openrouter_error("openrouter_catalog_unavailable"))?;
        *cache = Some(CachedCatalog {
            stored_at: Instant::now(),
            models: models.clone(),
        });
        Ok(models)
    }

    pub(crate) fn validate_cached_model_selection(
        &self,
        selection: &LlmRoleModels,
    ) -> Result<(), AppError> {
        let selected = selection.selected_ids().collect::<Vec<_>>();
        if selected.is_empty() {
            return Ok(());
        }
        let cache = self
            .catalog
            .lock()
            .map_err(|_| AppError::openrouter_error("openrouter_catalog_unavailable"))?;
        let catalog = cache
            .as_ref()
            .filter(|cached| cached.stored_at.elapsed() < CATALOG_CACHE_TTL)
            .ok_or_else(|| AppError::openrouter_error("openrouter_catalog_required"))?;
        if selected
            .iter()
            .all(|id| catalog.models.iter().any(|model| model.id == *id))
        {
            Ok(())
        } else {
            Err(AppError::openrouter_error("openrouter_model_not_available"))
        }
    }

    pub(super) fn cached_model_for_pricing(
        &self,
        model_id: &str,
    ) -> Result<OpenRouterModel, &'static str> {
        let cache = self
            .catalog
            .lock()
            .map_err(|_| "usage_catalog_unavailable")?;
        let catalog = cache
            .as_ref()
            .filter(|cached| cached.stored_at.elapsed() < CATALOG_CACHE_TTL)
            .ok_or("usage_catalog_required")?;
        catalog
            .models
            .iter()
            .find(|model| model.id == model_id)
            .cloned()
            .ok_or("usage_model_not_available")
    }

    #[cfg(test)]
    pub(super) fn seed_catalog_for_test(&self, models: Vec<OpenRouterModel>) {
        *self.catalog.lock().unwrap() = Some(CachedCatalog {
            stored_at: Instant::now(),
            models,
        });
    }

    fn send_get(&self, path: &str, api_key: &OpenRouterApiKey) -> Result<Response, AppError> {
        let mut authorization = Vec::with_capacity(7 + api_key.expose().len());
        authorization.extend_from_slice(b"Bearer ");
        authorization.extend_from_slice(api_key.expose());
        let mut authorization_header = HeaderValue::from_bytes(&authorization)
            .map_err(|_| AppError::openrouter_error("openrouter_credential_invalid"))?;
        authorization.fill(0);
        authorization_header.set_sensitive(true);

        self.client
            .get(format!("{}{path}", self.base_url))
            .header(AUTHORIZATION, authorization_header)
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, "KokoroKoe/0.1.0")
            .send()
            .map_err(map_transport_error)
            .and_then(check_status)
    }
}

fn map_transport_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() {
        AppError::openrouter_error("openrouter_timeout")
    } else {
        AppError::openrouter_error("openrouter_network_unavailable")
    }
}

fn check_status(response: Response) -> Result<Response, AppError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let code = match response.status() {
        StatusCode::UNAUTHORIZED => "openrouter_authentication_failed",
        StatusCode::PAYMENT_REQUIRED => "openrouter_payment_required",
        StatusCode::FORBIDDEN => "openrouter_permission_denied",
        StatusCode::REQUEST_TIMEOUT => "openrouter_timeout",
        StatusCode::TOO_MANY_REQUESTS => "openrouter_rate_limited",
        status if status.is_server_error() => "openrouter_service_unavailable",
        _ => "openrouter_request_rejected",
    };
    Err(AppError::openrouter_error(code))
}

fn read_success_body(mut response: Response, limit: usize) -> Result<Vec<u8>, AppError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AppError::openrouter_error("openrouter_response_too_large"));
    }
    let mut body = Vec::new();
    response
        .by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|_| AppError::openrouter_error("openrouter_network_unavailable"))?;
    if body.len() > limit {
        return Err(AppError::openrouter_error("openrouter_response_too_large"));
    }
    Ok(body)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyEnvelope {
    data: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelEnvelope {
    data: Vec<RemoteModel>,
}

#[derive(Deserialize)]
struct RemoteModel {
    id: String,
    name: String,
    context_length: u64,
    architecture: RemoteArchitecture,
    pricing: RemotePricing,
    #[serde(default)]
    supported_parameters: Vec<String>,
}

#[derive(Deserialize)]
struct RemoteArchitecture {
    input_modalities: Vec<String>,
    output_modalities: Vec<String>,
}

#[derive(Deserialize)]
struct RemotePricing {
    prompt: String,
    completion: String,
}

impl TryFrom<RemoteModel> for OpenRouterModel {
    type Error = AppError;

    fn try_from(remote: RemoteModel) -> Result<Self, Self::Error> {
        if !remote
            .architecture
            .input_modalities
            .iter()
            .any(|value| value == "text")
            || !remote
                .architecture
                .output_modalities
                .iter()
                .any(|value| value == "text")
        {
            return Err(AppError::openrouter_error(
                "openrouter_model_catalog_invalid",
            ));
        }
        let provider = remote
            .id
            .split_once('/')
            .map(|(provider, _)| provider.to_owned())
            .ok_or_else(|| AppError::openrouter_error("openrouter_model_catalog_invalid"))?;
        let supports_structured_outputs = remote
            .supported_parameters
            .iter()
            .any(|value| value == "structured_outputs" || value == "response_format");
        let model = Self {
            id: remote.id,
            name: remote.name,
            provider,
            context_length: remote.context_length,
            prompt_price_per_token: remote.pricing.prompt,
            completion_price_per_token: remote.pricing.completion,
            supports_structured_outputs,
            supports_streaming: true,
            zero_data_retention_available: true,
            data_collection: OpenRouterDataCollection::Deny,
        };
        model.validate().map_err(AppError::openrouter_error)?;
        Ok(model)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::TcpListener,
        sync::mpsc,
        thread,
    };

    use super::*;

    const TEST_KEY: &str = "secret-canary-openrouter-1234";
    const VALIDATED_AT: &str = "2026-08-13T12:34:56Z";

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_rfc3339(&self) -> Result<String, AppError> {
            Ok(VALIDATED_AT.to_owned())
        }
    }

    struct MockResponse {
        status: u16,
        body: String,
        delay: Duration,
        content_length: Option<usize>,
    }

    fn start_server(responses: Vec<MockResponse>) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n")
                    && request.len() < 16 * 1024
                {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                }
                let _ = sender.send(String::from_utf8(request).unwrap());
                thread::sleep(response.delay);
                let reason = if response.status == 200 {
                    "OK"
                } else {
                    "Error"
                };
                let length = response.content_length.unwrap_or(response.body.len());
                let wire = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.status, reason, length, response.body
                );
                let _ = stream.write_all(wire.as_bytes());
            }
        });
        (format!("http://{address}"), receiver)
    }

    fn make_service(base_url: &str, timeout: Duration) -> (OpenRouterService, CredentialService) {
        let credentials = CredentialService::in_memory(Some(TEST_KEY));
        let service = OpenRouterService::with_configuration(
            credentials.clone(),
            base_url,
            timeout,
            timeout,
            Arc::new(FixedClock),
        )
        .unwrap();
        (service, credentials)
    }

    #[test]
    fn validation_is_get_only_secret_safe_and_records_only_timestamp() {
        let (base_url, requests) = start_server(vec![MockResponse {
            status: 200,
            body: r#"{"data":{"label":"provider-owned-label","usage":99}}"#.into(),
            delay: Duration::ZERO,
            content_length: None,
        }]);
        let (service, credentials) = make_service(&base_url, Duration::from_secs(2));

        let validation = service.validate_credential().unwrap();
        assert_eq!(validation.validated_at, VALIDATED_AT);
        assert_eq!(validation.usage_usd, None);
        assert_eq!(
            credentials.status().unwrap().validated_at.as_deref(),
            Some(VALIDATED_AT)
        );
        let serialized = serde_json::to_string(&validation).unwrap();
        assert!(!serialized.contains(TEST_KEY));
        assert!(!serialized.contains("provider-owned-label"));

        let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(request.starts_with("GET /api/v1/key HTTP/1.1\r\n"));
        assert!(request.contains(&format!("authorization: Bearer {TEST_KEY}\r\n")));
        assert!(request.ends_with("\r\n\r\n"));
    }

    #[test]
    fn provider_statuses_and_raw_bodies_map_to_fixed_secret_free_errors() {
        for (status, expected) in [
            (401, "openrouter_authentication_failed"),
            (402, "openrouter_payment_required"),
            (403, "openrouter_permission_denied"),
            (429, "openrouter_rate_limited"),
            (503, "openrouter_service_unavailable"),
        ] {
            let (base_url, _) = start_server(vec![MockResponse {
                status,
                body: format!(r#"{{"error":{{"message":"{TEST_KEY}"}}}}"#),
                delay: Duration::ZERO,
                content_length: None,
            }]);
            let (service, _) = make_service(&base_url, Duration::from_secs(2));
            let error = service.validate_credential().unwrap_err();
            assert_eq!(error.code, expected);
            assert!(!serde_json::to_string(&error).unwrap().contains(TEST_KEY));
        }
    }

    #[test]
    fn model_catalog_is_zdr_text_only_bounded_sorted_and_cached() {
        let body = r#"{
          "data": [
            {
              "id": "zeta/second",
              "name": "Zulu",
              "context_length": 4096,
              "architecture": {"input_modalities":["text"],"output_modalities":["text"]},
              "pricing": {"prompt":"0.000002","completion":"0.000004"},
              "supported_parameters": ["temperature"]
            },
            {
              "id": "alpha/first",
              "name": "Alpha",
              "context_length": 131072,
              "architecture": {"input_modalities":["text","image"],"output_modalities":["text"]},
              "pricing": {"prompt":"0.000001","completion":"0.000003"},
              "supported_parameters": ["response_format"]
            }
          ]
        }"#;
        let (base_url, requests) = start_server(
            [body, body]
                .into_iter()
                .map(|body| MockResponse {
                    status: 200,
                    body: body.into(),
                    delay: Duration::ZERO,
                    content_length: None,
                })
                .collect(),
        );
        let (service, _) = make_service(&base_url, Duration::from_secs(2));

        let models = service.list_models(false).unwrap();
        assert_eq!(
            models.iter().map(|model| &model.id).collect::<Vec<_>>(),
            ["alpha/first", "zeta/second"]
        );
        assert!(models[0].supports_structured_outputs);
        assert!(models.iter().all(|model| model.supports_streaming));
        assert!(
            models
                .iter()
                .all(|model| model.zero_data_retention_available)
        );
        assert!(
            models
                .iter()
                .all(|model| model.data_collection == OpenRouterDataCollection::Deny)
        );
        assert_eq!(service.list_models(false).unwrap(), models);
        assert_eq!(
            service.cached_model_for_pricing("alpha/first").unwrap(),
            models[0]
        );
        assert_eq!(service.list_models(true).unwrap(), models);

        let selected = LlmRoleModels {
            insights: Some("alpha/first".to_owned()),
            summaries: Some("zeta/second".to_owned()),
            manual_questions: None,
        };
        service.validate_cached_model_selection(&selected).unwrap();
        let unknown = LlmRoleModels {
            insights: Some("missing/model".to_owned()),
            summaries: None,
            manual_questions: None,
        };
        assert_eq!(
            service
                .validate_cached_model_selection(&unknown)
                .unwrap_err()
                .code,
            "openrouter_model_not_available"
        );

        for _ in 0..2 {
            let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
            assert!(request.starts_with("GET /api/v1/models?limit=500&input_modalities=text&output_modalities=text&zdr=true HTTP/1.1\r\n"));
            assert!(!request.contains("audio"));
        }
        service.catalog.lock().unwrap().as_mut().unwrap().stored_at =
            Instant::now() - CATALOG_CACHE_TTL;
        assert_eq!(
            service.cached_model_for_pricing("alpha/first").unwrap_err(),
            "usage_catalog_required"
        );
    }

    #[test]
    fn model_catalog_rejects_non_text_and_duplicate_provider_records() {
        for body in [
            r#"{"data":[{"id":"audio/only","name":"Audio","context_length":4096,"architecture":{"input_modalities":["audio"],"output_modalities":["text"]},"pricing":{"prompt":"0","completion":"0"},"supported_parameters":[]}]}"#,
            r#"{"data":[{"id":"same/model","name":"A","context_length":4096,"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"pricing":{"prompt":"0","completion":"0"},"supported_parameters":[]},{"id":"same/model","name":"B","context_length":4096,"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"pricing":{"prompt":"0","completion":"0"},"supported_parameters":[]}]}"#,
        ] {
            let (base_url, _) = start_server(vec![MockResponse {
                status: 200,
                body: body.into(),
                delay: Duration::ZERO,
                content_length: None,
            }]);
            let (service, _) = make_service(&base_url, Duration::from_secs(2));
            assert_eq!(
                service.list_models(true).unwrap_err().code,
                "openrouter_model_catalog_invalid"
            );
        }
    }

    #[test]
    fn malformed_oversized_and_timeout_responses_fail_closed() {
        let (base_url, _) = start_server(vec![MockResponse {
            status: 200,
            body: "not-json".into(),
            delay: Duration::ZERO,
            content_length: None,
        }]);
        let (service, _) = make_service(&base_url, Duration::from_secs(2));
        assert_eq!(
            service.validate_credential().unwrap_err().code,
            "openrouter_response_invalid"
        );

        let (base_url, _) = start_server(vec![MockResponse {
            status: 200,
            body: String::new(),
            delay: Duration::ZERO,
            content_length: Some(VALIDATION_RESPONSE_LIMIT + 1),
        }]);
        let (service, _) = make_service(&base_url, Duration::from_secs(2));
        assert_eq!(
            service.validate_credential().unwrap_err().code,
            "openrouter_response_too_large"
        );

        let (base_url, _) = start_server(vec![MockResponse {
            status: 200,
            body: r#"{"data":{}}"#.into(),
            delay: Duration::from_millis(250),
            content_length: None,
        }]);
        let (service, _) = make_service(&base_url, Duration::from_millis(50));
        assert_eq!(
            service.validate_credential().unwrap_err().code,
            "openrouter_timeout"
        );
    }

    #[test]
    fn missing_credential_stops_before_network_access() {
        let credentials = CredentialService::in_memory(None);
        let service = OpenRouterService::with_configuration(
            credentials,
            "http://127.0.0.1:1",
            Duration::from_millis(50),
            Duration::from_millis(50),
            Arc::new(FixedClock),
        )
        .unwrap();
        assert_eq!(
            service.validate_credential().unwrap_err().code,
            "openrouter_credential_missing"
        );
        let selection = LlmRoleModels {
            insights: Some("example/model".to_owned()),
            summaries: None,
            manual_questions: None,
        };
        assert_eq!(
            service
                .validate_cached_model_selection(&selection)
                .unwrap_err()
                .code,
            "openrouter_catalog_required"
        );
    }
}
