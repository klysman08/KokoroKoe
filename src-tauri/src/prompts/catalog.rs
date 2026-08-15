use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PromptPurpose {
    Insight,
    Summary,
    ManualQuestion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PromptFallbackStrategy {
    SingleJsonRepairThenFail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PromptSpecification {
    pub(crate) id: &'static str,
    pub(crate) version: u16,
    pub(crate) purpose: PromptPurpose,
    pub(crate) variables: &'static [&'static str],
    pub(crate) approximate_request_token_limit: u32,
    pub(crate) maximum_output_tokens: u32,
    pub(crate) output_schema_id: &'static str,
    pub(crate) output_schema: &'static str,
    pub(crate) fallback_strategy: PromptFallbackStrategy,
    pub(crate) task_instructions: &'static str,
}

const INSIGHT_OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["insights"],
  "properties": {
    "insights": {
      "type": "array",
      "maxItems": 8,
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["type", "title", "content", "relatedSegmentIds"],
        "properties": {
          "type": {"enum": ["suggested_response", "follow_up_question", "clarification", "fact_or_number", "risk", "objection", "decision", "action_item", "contradiction", "unaddressed_topic"]},
          "title": {"type": "string", "maxLength": 160},
          "content": {"type": "string", "maxLength": 2000},
          "rationale": {"type": "string", "maxLength": 1000},
          "relatedSegmentIds": {"type": "array", "maxItems": 16, "items": {"type": "string", "format": "uuid"}},
          "confidence": {"type": "number", "minimum": 0, "maximum": 1}
        }
      }
    }
  }
}"#;

const SUMMARY_OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["executiveSummary", "mainTopics", "decisions", "actionItems", "risks", "openQuestions", "nextSteps"],
  "properties": {
    "executiveSummary": {"type": "string", "maxLength": 8000},
    "mainTopics": {"type": "array", "maxItems": 64, "items": {"type": "string", "maxLength": 1000}},
    "decisions": {"type": "array", "maxItems": 64, "items": {"type": "string", "maxLength": 2000}},
    "actionItems": {
      "type": "array",
      "maxItems": 64,
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["text", "relatedSegmentIds"],
        "properties": {
          "text": {"type": "string", "maxLength": 2000},
          "owner": {"type": "string", "maxLength": 256},
          "deadline": {"type": "string", "format": "date-time"},
          "relatedSegmentIds": {"type": "array", "maxItems": 16, "items": {"type": "string", "format": "uuid"}}
        }
      }
    },
    "risks": {"type": "array", "maxItems": 64, "items": {"type": "string", "maxLength": 2000}},
    "openQuestions": {"type": "array", "maxItems": 64, "items": {"type": "string", "maxLength": 2000}},
    "nextSteps": {"type": "array", "maxItems": 64, "items": {"type": "string", "maxLength": 2000}}
  }
}"#;

const MANUAL_QUESTION_OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["answer", "relatedSegmentIds", "limitations"],
  "properties": {
    "answer": {"type": "string", "maxLength": 8000},
    "relatedSegmentIds": {"type": "array", "maxItems": 16, "items": {"type": "string", "format": "uuid"}},
    "limitations": {"type": "array", "maxItems": 16, "items": {"type": "string", "maxLength": 1000}}
  }
}"#;

const PROMPT_SPECIFICATIONS: [PromptSpecification; 3] = [
    PromptSpecification {
        id: "kokorokoe.insight.recent",
        version: 1,
        purpose: PromptPurpose::Insight,
        variables: &[
            "project",
            "session",
            "preset",
            "requestedInsightTypes",
            "transcriptSegments",
        ],
        approximate_request_token_limit: 8_192,
        maximum_output_tokens: 1_024,
        output_schema_id: "insight_batch_v1",
        output_schema: INSIGHT_OUTPUT_SCHEMA,
        fallback_strategy: PromptFallbackStrategy::SingleJsonRepairThenFail,
        task_instructions: "Return only concise, non-duplicative insights of the requested types that are supported by the supplied transcript segments.",
    },
    PromptSpecification {
        id: "kokorokoe.summary.session",
        version: 1,
        purpose: PromptPurpose::Summary,
        variables: &[
            "project",
            "session",
            "preset",
            "summaryKind",
            "transcriptSegments",
        ],
        approximate_request_token_limit: 16_384,
        maximum_output_tokens: 4_096,
        output_schema_id: "session_summary_v1",
        output_schema: SUMMARY_OUTPUT_SCHEMA,
        fallback_strategy: PromptFallbackStrategy::SingleJsonRepairThenFail,
        task_instructions: "Return only a factual summary supported by the supplied transcript segments, preserving uncertainty and omitting invented decisions, owners, or deadlines.",
    },
    PromptSpecification {
        id: "kokorokoe.manual_question.segment",
        version: 1,
        purpose: PromptPurpose::ManualQuestion,
        variables: &[
            "project",
            "session",
            "preset",
            "question",
            "selectedSegmentId",
            "transcriptSegments",
        ],
        approximate_request_token_limit: 8_192,
        maximum_output_tokens: 2_048,
        output_schema_id: "manual_answer_v1",
        output_schema: MANUAL_QUESTION_OUTPUT_SCHEMA,
        fallback_strategy: PromptFallbackStrategy::SingleJsonRepairThenFail,
        task_instructions: "Answer only the supplied question using the supplied transcript segments and session context; state limitations when the evidence is insufficient.",
    },
];

pub(crate) const fn prompt_specifications() -> &'static [PromptSpecification] {
    &PROMPT_SPECIFICATIONS
}

pub(super) fn prompt_specification(purpose: PromptPurpose) -> &'static PromptSpecification {
    PROMPT_SPECIFICATIONS
        .iter()
        .find(|specification| specification.purpose == purpose)
        .expect("the fixed prompt catalog covers every purpose")
}

fn validate_catalog() -> Result<(), &'static str> {
    let mut ids = HashSet::new();
    let mut purposes = HashSet::new();
    for specification in PROMPT_SPECIFICATIONS {
        let schema: serde_json::Value = serde_json::from_str(specification.output_schema)
            .map_err(|_| "prompt_catalog_invalid")?;
        if specification.id.is_empty()
            || specification.id.len() > 128
            || !specification.id.bytes().all(|value| {
                value.is_ascii_lowercase()
                    || value.is_ascii_digit()
                    || matches!(value, b'.' | b'_' | b'-')
            })
            || specification.version == 0
            || specification.variables.is_empty()
            || specification.variables.len() > 16
            || specification
                .variables
                .iter()
                .any(|variable| variable.is_empty() || variable.len() > 64)
            || specification
                .variables
                .iter()
                .enumerate()
                .any(|(index, variable)| specification.variables[..index].contains(variable))
            || specification.approximate_request_token_limit < 1_024
            || specification.maximum_output_tokens == 0
            || specification.maximum_output_tokens
                > specification.approximate_request_token_limit / 2
            || specification.output_schema_id.is_empty()
            || specification.output_schema_id.len() > 128
            || specification.task_instructions.is_empty()
            || specification.task_instructions.len() > 1_024
            || schema.get("type").and_then(serde_json::Value::as_str) != Some("object")
            || schema
                .get("additionalProperties")
                .and_then(serde_json::Value::as_bool)
                != Some(false)
            || !ids.insert(specification.id)
            || !purposes.insert(specification.purpose)
        {
            return Err("prompt_catalog_invalid");
        }
    }
    if purposes.len() != 3 {
        return Err("prompt_catalog_invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::{PromptFallbackStrategy, PromptPurpose, prompt_specifications, validate_catalog};

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct CatalogFixture {
        specifications: Vec<SpecificationFixture>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SpecificationFixture {
        id: String,
        version: u16,
        purpose: PromptPurpose,
        variables: Vec<String>,
        approximate_request_token_limit: u32,
        maximum_output_tokens: u32,
        output_schema_id: String,
        output_schema: serde_json::Value,
        fallback_strategy: PromptFallbackStrategy,
    }

    #[test]
    fn prompt_catalog_matches_the_frozen_fixture_and_invariants() {
        validate_catalog().unwrap();
        let fixture: CatalogFixture = serde_json::from_str(include_str!(
            "../../../fixtures/prompts/prompt-catalog-v1.json"
        ))
        .unwrap();
        assert_eq!(fixture.specifications.len(), prompt_specifications().len());
        for (expected, actual) in fixture.specifications.iter().zip(prompt_specifications()) {
            assert_eq!(expected.id, actual.id);
            assert_eq!(expected.version, actual.version);
            assert_eq!(expected.purpose, actual.purpose);
            assert_eq!(expected.variables, actual.variables.to_vec());
            assert_eq!(
                expected.approximate_request_token_limit,
                actual.approximate_request_token_limit
            );
            assert_eq!(expected.maximum_output_tokens, actual.maximum_output_tokens);
            assert_eq!(expected.output_schema_id, actual.output_schema_id);
            assert_eq!(
                expected.output_schema,
                serde_json::from_str::<serde_json::Value>(actual.output_schema).unwrap()
            );
            assert_eq!(expected.fallback_strategy, actual.fallback_strategy);
        }
    }
}
