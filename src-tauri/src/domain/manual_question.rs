use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use uuid::Uuid;

use super::{ProjectId, RequestId, SessionId};

const MAX_QUESTION_BYTES: usize = 4_096;
const MAX_ANSWER_UTF16: usize = 8_000;
const MAX_LIMITATIONS: usize = 16;
const MAX_LIMITATION_UTF16: usize = 1_000;
const MAX_RELATED_SEGMENTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AskManualQuestionRequest {
    pub(crate) request_id: RequestId,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) selected_segment_id: Uuid,
    pub(crate) question: String,
}

impl AskManualQuestionRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.request_id.as_uuid().is_nil()
            || self.project_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || self.selected_segment_id.is_nil()
            || self.question.trim() != self.question
            || self.question.is_empty()
            || self.question.len() > MAX_QUESTION_BYTES
            || self.question.chars().any(|value| {
                value == '\r' || (value.is_control() && value != '\n' && value != '\t')
            })
        {
            return Err("manual_question_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManualQuestionUsage {
    pub(crate) input_tokens: u32,
    pub(crate) output_tokens: u32,
    pub(crate) actual_cost_usd: String,
}

impl ManualQuestionUsage {
    fn validate(&self) -> Result<(), &'static str> {
        if !canonical_cost(&self.actual_cost_usd) {
            return Err("manual_question_response_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManualQuestionResponse {
    pub(crate) schema_version: u8,
    pub(crate) request_id: RequestId,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) selected_segment_id: Uuid,
    pub(crate) answer: String,
    pub(crate) related_segment_ids: Vec<Uuid>,
    pub(crate) limitations: Vec<String>,
    pub(crate) primary_attempts: u8,
    pub(crate) repaired: bool,
    pub(crate) primary_usage: ManualQuestionUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repair_usage: Option<ManualQuestionUsage>,
    pub(crate) session_actual_cost_usd: String,
    pub(crate) available_budget_usd: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawManualQuestionResponse {
    schema_version: u8,
    request_id: RequestId,
    project_id: ProjectId,
    session_id: SessionId,
    selected_segment_id: Uuid,
    answer: String,
    related_segment_ids: Vec<Uuid>,
    limitations: Vec<String>,
    primary_attempts: u8,
    repaired: bool,
    primary_usage: ManualQuestionUsage,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    repair_usage: Option<ManualQuestionUsage>,
    session_actual_cost_usd: String,
    available_budget_usd: String,
}

impl<'de> Deserialize<'de> for ManualQuestionResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawManualQuestionResponse::deserialize(deserializer)?;
        let response = Self {
            schema_version: raw.schema_version,
            request_id: raw.request_id,
            project_id: raw.project_id,
            session_id: raw.session_id,
            selected_segment_id: raw.selected_segment_id,
            answer: raw.answer,
            related_segment_ids: raw.related_segment_ids,
            limitations: raw.limitations,
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

impl ManualQuestionResponse {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.request_id.as_uuid().is_nil()
            || self.project_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || self.selected_segment_id.is_nil()
            || !valid_text(&self.answer, MAX_ANSWER_UTF16)
            || self.related_segment_ids.len() > MAX_RELATED_SEGMENTS
            || self.related_segment_ids.iter().any(|value| value.is_nil())
            || has_duplicates(&self.related_segment_ids)
            || self.limitations.len() > MAX_LIMITATIONS
            || self
                .limitations
                .iter()
                .any(|value| !valid_text(value, MAX_LIMITATION_UTF16))
            || has_duplicates(&self.limitations)
            || !(1..=3).contains(&self.primary_attempts)
            || self.repaired != self.repair_usage.is_some()
            || !canonical_cost(&self.session_actual_cost_usd)
            || !canonical_cost(&self.available_budget_usd)
        {
            return Err("manual_question_response_invalid");
        }
        self.primary_usage.validate()?;
        if let Some(usage) = &self.repair_usage {
            usage.validate()?;
        }
        Ok(())
    }
}

fn valid_text(value: &str, maximum_utf16: usize) -> bool {
    !value.trim().is_empty()
        && value.encode_utf16().count() <= maximum_utf16
        && !value.chars().any(char::is_control)
}

fn has_duplicates<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
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

    #[test]
    fn shared_manual_question_contract_is_strict_and_valid() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/manual-question-v1.json"
        ))
        .unwrap();
        let request: AskManualQuestionRequest =
            serde_json::from_value(fixture["request"].clone()).unwrap();
        request.validate().unwrap();
        let response: ManualQuestionResponse =
            serde_json::from_value(fixture["response"].clone()).unwrap();
        response.validate().unwrap();

        let mut unknown = fixture["request"].clone();
        unknown["extra"] = json!(true);
        assert!(serde_json::from_value::<AskManualQuestionRequest>(unknown).is_err());
        let mut nullable = fixture["response"].clone();
        nullable["repairUsage"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<ManualQuestionResponse>(nullable).is_err());
    }
}
