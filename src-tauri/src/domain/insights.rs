use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use uuid::Uuid;

use super::{InsightType, ProjectId, RequestId, SessionId};

const MAX_INSIGHTS: usize = 8;
const MAX_TITLE_UTF16: usize = 160;
const MAX_CONTENT_UTF16: usize = 2_000;
const MAX_RATIONALE_UTF16: usize = 1_000;
const MAX_RELATED_SEGMENTS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GenerateRecentInsightsRequest {
    pub(crate) request_id: RequestId,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
}

impl GenerateRecentInsightsRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.request_id.as_uuid().is_nil()
            || self.project_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
        {
            return Err("insight_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InsightUsage {
    pub(crate) input_tokens: u32,
    pub(crate) output_tokens: u32,
    pub(crate) actual_cost_usd: String,
}

impl InsightUsage {
    fn validate(&self) -> Result<(), &'static str> {
        if !canonical_cost(&self.actual_cost_usd) {
            return Err("insight_response_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecentInsight {
    pub(crate) r#type: InsightType,
    pub(crate) title: String,
    pub(crate) content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rationale: Option<String>,
    pub(crate) related_segment_ids: Vec<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) confidence: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawRecentInsight {
    r#type: InsightType,
    title: String,
    content: String,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    rationale: Option<String>,
    related_segment_ids: Vec<Uuid>,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    confidence: Option<f64>,
}

impl<'de> Deserialize<'de> for RecentInsight {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawRecentInsight::deserialize(deserializer)?;
        let insight = Self {
            r#type: raw.r#type,
            title: raw.title,
            content: raw.content,
            rationale: raw.rationale,
            related_segment_ids: raw.related_segment_ids,
            confidence: raw.confidence,
        };
        insight.validate().map_err(D::Error::custom)?;
        Ok(insight)
    }
}

impl RecentInsight {
    fn validate(&self) -> Result<(), &'static str> {
        if !valid_text(&self.title, MAX_TITLE_UTF16)
            || !valid_text(&self.content, MAX_CONTENT_UTF16)
            || self
                .rationale
                .as_deref()
                .is_some_and(|value| !valid_text(value, MAX_RATIONALE_UTF16))
            || self.related_segment_ids.len() > MAX_RELATED_SEGMENTS
            || self.related_segment_ids.iter().any(Uuid::is_nil)
            || has_duplicates(&self.related_segment_ids)
            || self
                .confidence
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err("insight_response_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecentInsightsResponse {
    pub(crate) schema_version: u8,
    pub(crate) request_id: RequestId,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) insights: Vec<RecentInsight>,
    pub(crate) primary_attempts: u8,
    pub(crate) repaired: bool,
    pub(crate) primary_usage: InsightUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repair_usage: Option<InsightUsage>,
    pub(crate) session_actual_cost_usd: String,
    pub(crate) available_budget_usd: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawRecentInsightsResponse {
    schema_version: u8,
    request_id: RequestId,
    project_id: ProjectId,
    session_id: SessionId,
    insights: Vec<RecentInsight>,
    primary_attempts: u8,
    repaired: bool,
    primary_usage: InsightUsage,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    repair_usage: Option<InsightUsage>,
    session_actual_cost_usd: String,
    available_budget_usd: String,
}

impl<'de> Deserialize<'de> for RecentInsightsResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawRecentInsightsResponse::deserialize(deserializer)?;
        let response = Self {
            schema_version: raw.schema_version,
            request_id: raw.request_id,
            project_id: raw.project_id,
            session_id: raw.session_id,
            insights: raw.insights,
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

impl RecentInsightsResponse {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || self.request_id.as_uuid().is_nil()
            || self.project_id.as_uuid().is_nil()
            || self.session_id.as_uuid().is_nil()
            || self.insights.len() > MAX_INSIGHTS
            || self.insights.iter().enumerate().any(|(index, insight)| {
                self.insights[..index].iter().any(|previous| {
                    previous.r#type == insight.r#type
                        && previous.title == insight.title
                        && previous.content == insight.content
                })
            })
            || !(1..=3).contains(&self.primary_attempts)
            || self.repaired != self.repair_usage.is_some()
            || !canonical_cost(&self.session_actual_cost_usd)
            || !canonical_cost(&self.available_budget_usd)
        {
            return Err("insight_response_invalid");
        }
        for insight in &self.insights {
            insight.validate()?;
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

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../fixtures/contracts/recent-insights-v1.json"
        ))
        .unwrap()
    }

    #[test]
    fn shared_recent_insight_contract_is_strict_and_valid() {
        let fixture = fixture();
        let request: GenerateRecentInsightsRequest =
            serde_json::from_value(fixture["request"].clone()).unwrap();
        request.validate().unwrap();
        let response: RecentInsightsResponse =
            serde_json::from_value(fixture["response"].clone()).unwrap();
        response.validate().unwrap();

        let mut unknown = fixture["request"].clone();
        unknown["extra"] = json!(true);
        assert!(serde_json::from_value::<GenerateRecentInsightsRequest>(unknown).is_err());
        let mut nullable = fixture["response"].clone();
        nullable["repairUsage"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<RecentInsightsResponse>(nullable).is_err());
    }

    #[test]
    fn out_of_range_confidence_and_duplicate_insights_are_rejected() {
        let mut confident = fixture()["response"].clone();
        confident["insights"][0]["confidence"] = json!(1.5);
        assert!(serde_json::from_value::<RecentInsightsResponse>(confident).is_err());

        let mut duplicated = fixture()["response"].clone();
        let first = duplicated["insights"][0].clone();
        duplicated["insights"] = json!([first.clone(), first]);
        assert!(serde_json::from_value::<RecentInsightsResponse>(duplicated).is_err());

        let mut controlled = fixture()["response"].clone();
        controlled["insights"][0]["title"] = json!("control\u{0007}character");
        assert!(serde_json::from_value::<RecentInsightsResponse>(controlled).is_err());
    }

    #[test]
    fn unpriced_costs_and_repair_mismatches_are_rejected() {
        let mut cost = fixture()["response"].clone();
        cost["sessionActualCostUsd"] = json!("0.1");
        assert!(serde_json::from_value::<RecentInsightsResponse>(cost).is_err());

        let mut mismatch = fixture()["response"].clone();
        mismatch["repaired"] = json!(true);
        assert!(serde_json::from_value::<RecentInsightsResponse>(mismatch).is_err());
    }
}
