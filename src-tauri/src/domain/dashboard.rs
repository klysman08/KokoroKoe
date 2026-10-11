use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use super::{
    error::ErrorSeverity,
    projects::{JSON_SAFE_INTEGER_MAX, ProjectId, deserialize_non_null_optional},
};

/// How many entries each "most used" list carries.
pub(crate) const DASHBOARD_RANKING_LIMIT: usize = 5;
/// How many recent errors the dashboard shows.
pub(crate) const DASHBOARD_RECENT_ERROR_LIMIT: usize = 10;
const MAX_MODEL_ID_BYTES: usize = 256;

/// The period and project a dashboard reads.
///
/// Both bounds are optional so "all time" needs no sentinel date. `from` is
/// inclusive and `to` exclusive, so adjacent periods never count a moment twice.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DashboardMetricsQuery {
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    pub(crate) from: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    pub(crate) to: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    pub(crate) project_id: Option<ProjectId>,
}

/// A validated query, with its bounds as Unix milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetricsWindow {
    pub(crate) from_ms: Option<i64>,
    pub(crate) to_ms: Option<i64>,
    pub(crate) project_id: Option<ProjectId>,
}

impl DashboardMetricsQuery {
    pub(crate) fn window(&self) -> Result<MetricsWindow, &'static str> {
        let from_ms = self.from.as_deref().map(parse_unix_ms).transpose()?;
        let to_ms = self.to.as_deref().map(parse_unix_ms).transpose()?;
        if let (Some(from), Some(to)) = (from_ms, to_ms)
            && from >= to
        {
            return Err("dashboard_query_invalid");
        }
        if self
            .project_id
            .is_some_and(|project_id| project_id.as_uuid().is_nil())
        {
            return Err("dashboard_query_invalid");
        }
        Ok(MetricsWindow {
            from_ms,
            to_ms,
            project_id: self.project_id,
        })
    }
}

impl MetricsWindow {
    pub(crate) fn contains(&self, at_ms: i64) -> bool {
        self.from_ms.is_none_or(|from| at_ms >= from) && self.to_ms.is_none_or(|to| at_ms < to)
    }
}

/// Parses an RFC 3339 timestamp into Unix milliseconds.
pub(crate) fn parse_unix_ms(value: &str) -> Result<i64, &'static str> {
    if value.len() > 64 {
        return Err("dashboard_query_invalid");
    }
    let parsed = OffsetDateTime::parse(value, &Rfc3339).map_err(|_| "dashboard_query_invalid")?;
    i64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).map_err(|_| "dashboard_query_invalid")
}

/// Formats Unix milliseconds as an RFC 3339 UTC timestamp.
pub(crate) fn format_unix_ms(value: i64) -> Option<String> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(value) * 1_000_000)
        .ok()?
        .format(&Rfc3339)
        .ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectSessionCount {
    pub(crate) project_id: ProjectId,
    pub(crate) sessions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptionModelSessionCount {
    pub(crate) model_id: String,
    pub(crate) sessions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LlmModelUsage {
    pub(crate) model_id: String,
    pub(crate) requests: u32,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) actual_cost_usd: String,
}

/// Bytes on disk. `complete` is false when the walk stopped at its entry
/// ceiling, so the figures are a floor rather than a total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DashboardDiskUsage {
    pub(crate) workspace_bytes: u64,
    pub(crate) trash_bytes: u64,
    pub(crate) model_bytes: u64,
    pub(crate) complete: bool,
}

/// A failed operation, as the sanitized error envelope already reported it.
///
/// It carries the same fields the log does plus the fixed user message, and
/// never the technical detail: that is the one field allowed to name what went
/// wrong, and a dashboard has no reason to keep it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecentErrorView {
    pub(crate) code: String,
    pub(crate) user_message: String,
    pub(crate) severity: ErrorSeverity,
    pub(crate) retryable: bool,
    pub(crate) correlation_id: Uuid,
    pub(crate) occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DashboardMetrics {
    pub(crate) schema_version: u8,
    pub(crate) sessions: u32,
    pub(crate) transcribed_ms: u64,
    pub(crate) projects: Vec<ProjectSessionCount>,
    pub(crate) transcription_models: Vec<TranscriptionModelSessionCount>,
    pub(crate) llm_models: Vec<LlmModelUsage>,
    pub(crate) llm_requests: u32,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) actual_cost_usd: String,
    pub(crate) insights_generated: u64,
    /// When this installation began keeping a usage ledger. Spending before it
    /// was never recorded, so a total is only a total since then.
    pub(crate) usage_recorded_since: String,
    pub(crate) disk: DashboardDiskUsage,
    pub(crate) recent_errors: Vec<RecentErrorView>,
}

impl DashboardMetrics {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let safe = |value: u64| value <= JSON_SAFE_INTEGER_MAX;
        let valid_model = |value: &str| {
            !value.trim().is_empty()
                && value.len() <= MAX_MODEL_ID_BYTES
                && !value.chars().any(char::is_control)
        };
        if self.schema_version != 1
            || !safe(self.transcribed_ms)
            || !safe(self.input_tokens)
            || !safe(self.output_tokens)
            || !safe(self.insights_generated)
            || !safe(self.disk.workspace_bytes)
            || !safe(self.disk.trash_bytes)
            || !safe(self.disk.model_bytes)
            || self.disk.trash_bytes > self.disk.workspace_bytes
            || !canonical_usd(&self.actual_cost_usd)
            || parse_unix_ms(&self.usage_recorded_since).is_err()
            || self.projects.len() > DASHBOARD_RANKING_LIMIT
            || self.transcription_models.len() > DASHBOARD_RANKING_LIMIT
            || self.llm_models.len() > DASHBOARD_RANKING_LIMIT
            || self.recent_errors.len() > DASHBOARD_RECENT_ERROR_LIMIT
            || self
                .projects
                .iter()
                .any(|entry| entry.sessions == 0 || entry.sessions > self.sessions)
            || self.transcription_models.iter().any(|entry| {
                entry.sessions == 0
                    || entry.sessions > self.sessions
                    || !valid_model(&entry.model_id)
            })
            || self.llm_models.iter().any(|entry| {
                entry.requests == 0
                    || entry.requests > self.llm_requests
                    || !safe(entry.input_tokens)
                    || !safe(entry.output_tokens)
                    || !canonical_usd(&entry.actual_cost_usd)
                    || !valid_model(&entry.model_id)
            })
            || self
                .recent_errors
                .iter()
                .any(|entry| parse_unix_ms(&entry.occurred_at).is_err())
        {
            return Err("dashboard_metrics_invalid");
        }
        Ok(())
    }
}

/// The ledger's canonical amount: whole dollars and exactly twelve decimals.
pub(crate) fn canonical_usd(value: &str) -> bool {
    let Some((whole, fraction)) = value.split_once('.') else {
        return false;
    };
    !whole.is_empty()
        && whole.len() <= 7
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && (whole == "0" || !whole.starts_with('0'))
        && fraction.len() == 12
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../../fixtures/contracts/dashboard-metrics-v1.json"
        ))
        .unwrap()
    }

    fn fixture_metrics() -> DashboardMetrics {
        let project_id: ProjectId =
            serde_json::from_value(Value::String("8f8b6f2e-1a7d-4b8e-9f4c-2f2a9e6d1c01".into()))
                .unwrap();
        DashboardMetrics {
            schema_version: 1,
            sessions: 4,
            transcribed_ms: 5_400_000,
            projects: vec![ProjectSessionCount {
                project_id,
                sessions: 4,
            }],
            transcription_models: vec![TranscriptionModelSessionCount {
                model_id: "whisper-base".into(),
                sessions: 3,
            }],
            llm_models: vec![LlmModelUsage {
                model_id: "openai/gpt-4.1-mini".into(),
                requests: 12,
                input_tokens: 48_000,
                output_tokens: 6_000,
                actual_cost_usd: "0.028800000000".into(),
            }],
            llm_requests: 12,
            input_tokens: 48_000,
            output_tokens: 6_000,
            actual_cost_usd: "0.028800000000".into(),
            insights_generated: 21,
            usage_recorded_since: "2026-10-01T09:00:00Z".into(),
            disk: DashboardDiskUsage {
                workspace_bytes: 1_048_576,
                trash_bytes: 4_096,
                model_bytes: 147_951_465,
                complete: true,
            },
            recent_errors: vec![RecentErrorView {
                code: "insight_payment_required".into(),
                user_message: "OpenRouter needs credit before it can generate insights.".into(),
                severity: ErrorSeverity::Warning,
                retryable: false,
                correlation_id: Uuid::parse_str("4b1f9c2e-7d3a-4e8b-a1c5-9e2d7f6b3a10").unwrap(),
                occurred_at: "2026-10-10T14:30:00Z".into(),
            }],
        }
    }

    /// The fixture Zod parses is exactly what Rust serializes, so neither side
    /// can drift while both keep passing.
    #[test]
    fn the_shared_fixture_is_what_rust_serializes() {
        let metrics = fixture_metrics();
        metrics.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&metrics).unwrap(),
            fixture()["metrics"]
        );
    }

    #[test]
    fn queries_parse_strictly_and_order_their_bounds() {
        let query: DashboardMetricsQuery =
            serde_json::from_value(fixture()["query"].clone()).unwrap();
        let window = query.window().unwrap();
        assert!(window.contains(window.from_ms.unwrap()));
        assert!(!window.contains(window.to_ms.unwrap()));

        let all_time: DashboardMetricsQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(all_time.window().unwrap().from_ms, None);

        for invalid in [
            r#"{"from":null}"#,
            r#"{"unknown":1}"#,
            r#"{"from":"yesterday"}"#,
        ] {
            let parsed = serde_json::from_str::<DashboardMetricsQuery>(invalid);
            assert!(
                parsed.is_err() || parsed.unwrap().window().is_err(),
                "{invalid}"
            );
        }
        let reversed: DashboardMetricsQuery =
            serde_json::from_str(r#"{"from":"2026-10-02T00:00:00Z","to":"2026-10-01T00:00:00Z"}"#)
                .unwrap();
        assert_eq!(reversed.window().unwrap_err(), "dashboard_query_invalid");
    }

    #[test]
    fn metrics_reject_rankings_that_outnumber_their_totals() {
        let mut metrics = fixture_metrics();
        metrics.projects[0].sessions = 5;
        assert!(metrics.validate().is_err());

        let mut metrics = fixture_metrics();
        metrics.actual_cost_usd = "0.0288".into();
        assert!(metrics.validate().is_err());

        let mut metrics = fixture_metrics();
        metrics.disk.trash_bytes = metrics.disk.workspace_bytes + 1;
        assert!(metrics.validate().is_err());
    }

    #[test]
    fn canonical_amounts_are_exact() {
        assert!(canonical_usd("0.000000000000"));
        assert!(canonical_usd("12.500000000000"));
        assert!(!canonical_usd("012.500000000000"));
        assert!(!canonical_usd("1.5"));
        assert!(!canonical_usd("-1.000000000000"));
    }
}
