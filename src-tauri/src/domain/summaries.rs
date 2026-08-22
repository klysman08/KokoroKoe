use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::{ProjectId, RequestId, SessionId, validate_rfc3339};

const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_SUMMARY_MARKDOWN_UTF16: usize = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GenerateSessionSummaryRequest {
    pub(crate) request_id: RequestId,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
}

impl GenerateSessionSummaryRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.request_id.as_uuid().is_nil()
            || self.project_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
        {
            return Err("summary_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GetSessionSummaryRequest {
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
}

impl GetSessionSummaryRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.project_id.as_uuid().is_nil() || self.session_id.as_uuid().is_nil() {
            return Err("summary_invalid");
        }
        Ok(())
    }
}

/// The validated summary content the store renders into `summary.md`.
///
/// This is the sanitized hand-off between the LLM boundary and persistence: it
/// holds only already-validated plain text, and carries no provider metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SessionSummaryContent {
    pub(crate) executive_summary: String,
    pub(crate) main_topics: Vec<String>,
    pub(crate) decisions: Vec<String>,
    pub(crate) action_items: Vec<SummaryActionItem>,
    pub(crate) risks: Vec<String>,
    pub(crate) open_questions: Vec<String>,
    pub(crate) next_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SummaryActionItem {
    pub(crate) text: String,
    pub(crate) owner: Option<String>,
    pub(crate) deadline: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SummaryUsage {
    pub(crate) input_tokens: u32,
    pub(crate) output_tokens: u32,
    pub(crate) actual_cost_usd: String,
}

impl SummaryUsage {
    fn validate(&self) -> Result<(), &'static str> {
        if !canonical_cost(&self.actual_cost_usd) {
            return Err("summary_response_invalid");
        }
        Ok(())
    }
}

/// The rendered `summary.md` body together with the coverage it was built from.
///
/// `markdown` is the portable source-of-truth document text; the frontend renders
/// it through the existing inert Markdown renderer and never parses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SessionSummaryDocument {
    pub(crate) schema_version: u8,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) generated_at: String,
    pub(crate) model_id: String,
    pub(crate) markdown: String,
    pub(crate) segments_considered: u32,
    pub(crate) segments_included: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSessionSummaryDocument {
    schema_version: u8,
    project_id: ProjectId,
    session_id: SessionId,
    generated_at: String,
    model_id: String,
    markdown: String,
    segments_considered: u32,
    segments_included: u32,
}

impl<'de> Deserialize<'de> for SessionSummaryDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSessionSummaryDocument::deserialize(deserializer)?;
        let document = Self {
            schema_version: raw.schema_version,
            project_id: raw.project_id,
            session_id: raw.session_id,
            generated_at: raw.generated_at,
            model_id: raw.model_id,
            markdown: raw.markdown,
            segments_considered: raw.segments_considered,
            segments_included: raw.segments_included,
        };
        document.validate().map_err(D::Error::custom)?;
        Ok(document)
    }
}

impl SessionSummaryDocument {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.project_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || validate_rfc3339(&self.generated_at).is_err()
            || self.model_id.trim().is_empty()
            || self.model_id.len() > MAX_MODEL_ID_BYTES
            || self.model_id.chars().any(char::is_control)
            || self.markdown.trim().is_empty()
            || self.markdown.encode_utf16().count() > MAX_SUMMARY_MARKDOWN_UTF16
            || self.markdown.contains('\r')
            || self
                .markdown
                .chars()
                .any(|value| value.is_control() && value != '\n')
            || self.segments_included > self.segments_considered
        {
            return Err("summary_response_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SessionSummaryStatus {
    pub(crate) schema_version: u8,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) document: Option<SessionSummaryDocument>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSessionSummaryStatus {
    schema_version: u8,
    project_id: ProjectId,
    session_id: SessionId,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    document: Option<SessionSummaryDocument>,
}

impl<'de> Deserialize<'de> for SessionSummaryStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSessionSummaryStatus::deserialize(deserializer)?;
        let status = Self {
            schema_version: raw.schema_version,
            project_id: raw.project_id,
            session_id: raw.session_id,
            document: raw.document,
        };
        status.validate().map_err(D::Error::custom)?;
        Ok(status)
    }
}

impl SessionSummaryStatus {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.project_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
        {
            return Err("summary_response_invalid");
        }
        if let Some(document) = &self.document {
            if document.project_id != self.project_id || document.session_id != self.session_id {
                return Err("summary_response_invalid");
            }
            document.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GenerateSessionSummaryResponse {
    pub(crate) schema_version: u8,
    pub(crate) request_id: RequestId,
    pub(crate) document: SessionSummaryDocument,
    pub(crate) primary_attempts: u8,
    pub(crate) repaired: bool,
    pub(crate) primary_usage: SummaryUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repair_usage: Option<SummaryUsage>,
    pub(crate) session_actual_cost_usd: String,
    pub(crate) available_budget_usd: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawGenerateSessionSummaryResponse {
    schema_version: u8,
    request_id: RequestId,
    document: SessionSummaryDocument,
    primary_attempts: u8,
    repaired: bool,
    primary_usage: SummaryUsage,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    repair_usage: Option<SummaryUsage>,
    session_actual_cost_usd: String,
    available_budget_usd: String,
}

impl<'de> Deserialize<'de> for GenerateSessionSummaryResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawGenerateSessionSummaryResponse::deserialize(deserializer)?;
        let response = Self {
            schema_version: raw.schema_version,
            request_id: raw.request_id,
            document: raw.document,
            primary_attempts: raw.primary_attempts,
            repaired: raw.repaired,
            primary_usage: raw.primary_usage,
            repair_usage: raw.repair_usage,
            session_actual_cost_usd: raw.session_actual_cost_usd,
            available_budget_usd: raw.available_budget_usd,
        };
        response.validate().map_err(D::Error::custom)?;
        Ok(response)
    }
}

impl GenerateSessionSummaryResponse {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.request_id.as_uuid().is_nil()
            || !(1..=3).contains(&self.primary_attempts)
            || self.repaired != self.repair_usage.is_some()
            || !canonical_cost(&self.session_actual_cost_usd)
            || !canonical_cost(&self.available_budget_usd)
        {
            return Err("summary_response_invalid");
        }
        self.document.validate()?;
        self.primary_usage.validate()?;
        if let Some(usage) = &self.repair_usage {
            usage.validate()?;
        }
        Ok(())
    }
}

fn canonical_cost(value: &str) -> bool {
    let Some((whole, fraction)) = value.split_once('.') else {
        return false;
    };
    value.len() <= 48
        && !whole.is_empty()
        && whole.bytes().all(|value| value.is_ascii_digit())
        && (whole == "0" || !whole.starts_with('0'))
        && fraction.len() == 12
        && fraction.bytes().all(|value| value.is_ascii_digit())
}

fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../fixtures/contracts/session-summary-v1.json"
        ))
        .unwrap()
    }

    #[test]
    fn shared_session_summary_contract_is_strict_and_valid() {
        let fixture = fixture();
        let request: GenerateSessionSummaryRequest =
            serde_json::from_value(fixture["generateRequest"].clone()).unwrap();
        request.validate().unwrap();
        let response: GenerateSessionSummaryResponse =
            serde_json::from_value(fixture["generateResponse"].clone()).unwrap();
        response.validate().unwrap();
        let status: SessionSummaryStatus =
            serde_json::from_value(fixture["status"].clone()).unwrap();
        status.validate().unwrap();
        let absent: SessionSummaryStatus =
            serde_json::from_value(fixture["absentStatus"].clone()).unwrap();
        assert!(absent.document.is_none());

        let mut unknown = fixture["generateRequest"].clone();
        unknown["extra"] = json!(true);
        assert!(serde_json::from_value::<GenerateSessionSummaryRequest>(unknown).is_err());
        let mut nullable = fixture["status"].clone();
        nullable["document"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<SessionSummaryStatus>(nullable).is_err());
    }

    #[test]
    fn a_status_document_must_match_its_own_scope() {
        let mut mismatched = fixture()["status"].clone();
        mismatched["document"]["sessionId"] = json!("99999999-9999-4999-8999-999999999999");
        assert!(serde_json::from_value::<SessionSummaryStatus>(mismatched).is_err());
    }

    #[test]
    fn malformed_documents_and_costs_are_rejected() {
        let mut carriage = fixture()["generateResponse"].clone();
        carriage["document"]["markdown"] = json!("# Summary\r\n");
        assert!(serde_json::from_value::<GenerateSessionSummaryResponse>(carriage).is_err());

        let mut coverage = fixture()["generateResponse"].clone();
        coverage["document"]["segmentsIncluded"] = json!(9_999);
        assert!(serde_json::from_value::<GenerateSessionSummaryResponse>(coverage).is_err());

        let mut generated = fixture()["generateResponse"].clone();
        generated["document"]["generatedAt"] = json!("not-a-timestamp");
        assert!(serde_json::from_value::<GenerateSessionSummaryResponse>(generated).is_err());

        let mut cost = fixture()["generateResponse"].clone();
        cost["availableBudgetUsd"] = json!("0.5");
        assert!(serde_json::from_value::<GenerateSessionSummaryResponse>(cost).is_err());

        let mut mismatch = fixture()["generateResponse"].clone();
        mismatch["repaired"] = json!(true);
        assert!(serde_json::from_value::<GenerateSessionSummaryResponse>(mismatch).is_err());
    }
}
