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

// Every output schema is written in the subset that strict structured outputs
// accept: each object lists all of its properties in `required`, an optional
// value is a `null` union rather than an absent key, and no string-length,
// numeric-range, array-size, or format keyword appears. Providers enforcing
// strict mode reject a schema outside that subset with HTTP 400, which is how
// the version-one schemas failed against a real model. The bounds those
// keywords expressed are enforced locally in `llm/results.rs` instead, where
// they hold whatever the provider does.

const INSIGHT_OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["insights"],
  "properties": {
    "insights": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["type", "title", "content", "rationale", "relatedSegmentIds", "confidence"],
        "properties": {
          "type": {"type": "string", "enum": ["suggested_response", "follow_up_question", "clarification", "fact_or_number", "risk", "objection", "decision", "action_item", "contradiction", "unaddressed_topic"]},
          "title": {"type": "string"},
          "content": {"type": "string"},
          "rationale": {"type": ["string", "null"]},
          "relatedSegmentIds": {"type": "array", "items": {"type": "string"}},
          "confidence": {"type": ["number", "null"]}
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
    "executiveSummary": {"type": "string"},
    "mainTopics": {"type": "array", "items": {"type": "string"}},
    "decisions": {"type": "array", "items": {"type": "string"}},
    "actionItems": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["text", "owner", "deadline", "relatedSegmentIds"],
        "properties": {
          "text": {"type": "string"},
          "owner": {"type": ["string", "null"]},
          "deadline": {"type": ["string", "null"]},
          "relatedSegmentIds": {"type": "array", "items": {"type": "string"}}
        }
      }
    },
    "risks": {"type": "array", "items": {"type": "string"}},
    "openQuestions": {"type": "array", "items": {"type": "string"}},
    "nextSteps": {"type": "array", "items": {"type": "string"}}
  }
}"#;

const MANUAL_QUESTION_OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["answer", "relatedSegmentIds", "limitations"],
  "properties": {
    "answer": {"type": "string"},
    "relatedSegmentIds": {"type": "array", "items": {"type": "string"}},
    "limitations": {"type": "array", "items": {"type": "string"}}
  }
}"#;

const PROMPT_SPECIFICATIONS: [PromptSpecification; 3] = [
    PromptSpecification {
        id: "kokorokoe.insight.recent",
        version: 2,
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
        output_schema_id: "insight_batch_v2",
        output_schema: INSIGHT_OUTPUT_SCHEMA,
        fallback_strategy: PromptFallbackStrategy::SingleJsonRepairThenFail,
        task_instructions: "Return the insights of the requested types that would help the user most right now, usually one to five: questions worth asking, risks, decisions, action items, facts or numbers worth checking, topics left unaddressed. Make each one specific to what was said, cite the segments it rests on, and do not repeat an earlier insight. Return an empty list only when the segments hold no substantive discussion. Write every text value in the language named by the session's language field.",
    },
    PromptSpecification {
        id: "kokorokoe.summary.session",
        version: 2,
        purpose: PromptPurpose::Summary,
        variables: &[
            "project",
            "session",
            "preset",
            "summaryKind",
            "transcriptSegments",
        ],
        approximate_request_token_limit: 32_768,
        maximum_output_tokens: 4_096,
        output_schema_id: "session_summary_v2",
        output_schema: SUMMARY_OUTPUT_SCHEMA,
        fallback_strategy: PromptFallbackStrategy::SingleJsonRepairThenFail,
        task_instructions: "Summarize the meeting for someone who missed it, using only the supplied transcript segments. The executive summary is a full paragraph: what the meeting was for, what was discussed, and where it ended. List every main topic. Record decisions, action items with their owners and deadlines, risks, open questions, and next steps whenever the segments support them, and leave a list empty rather than invent an entry. Prefer concrete detail - names, numbers, tools, examples, reasons - over generic phrasing, and preserve uncertainty where speakers were unsure. Write every text value in the language named by the session's language field.",
    },
    PromptSpecification {
        id: "kokorokoe.manual_question.segment",
        version: 2,
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
        output_schema_id: "manual_answer_v2",
        output_schema: MANUAL_QUESTION_OUTPUT_SCHEMA,
        fallback_strategy: PromptFallbackStrategy::SingleJsonRepairThenFail,
        task_instructions: "Answer only the supplied question using the supplied transcript segments and session context; state limitations when the evidence is insufficient. Write every text value in the language named by the session's language field.",
    },
];

// Catalog self-inspection helpers used only by the catalog's own invariant tests.
#[allow(dead_code)]
pub(crate) const fn prompt_specifications() -> &'static [PromptSpecification] {
    &PROMPT_SPECIFICATIONS
}

pub(super) fn prompt_specification(purpose: PromptPurpose) -> &'static PromptSpecification {
    PROMPT_SPECIFICATIONS
        .iter()
        .find(|specification| specification.purpose == purpose)
        .expect("the fixed prompt catalog covers every purpose")
}

#[allow(dead_code)]
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

    /// Strict structured outputs reject a schema with an optional property or
    /// a keyword outside their subset, and they reject it with an HTTP 400 that
    /// reads to the user like a bad model choice. Every object therefore names
    /// all of its properties as required and closes itself, and none of the
    /// unsupported keywords appears anywhere.
    #[test]
    fn every_output_schema_is_in_the_strict_structured_output_subset() {
        fn walk(path: &str, value: &serde_json::Value) {
            let Some(object) = value.as_object() else {
                if let Some(items) = value.as_array() {
                    items.iter().for_each(|item| walk(path, item));
                }
                return;
            };
            for forbidden in [
                "maxLength",
                "minLength",
                "maxItems",
                "minItems",
                "minimum",
                "maximum",
                "format",
                "pattern",
            ] {
                assert!(!object.contains_key(forbidden), "{path} uses {forbidden}");
            }
            if let Some(properties) = object.get("properties").and_then(|value| value.as_object()) {
                let required = object
                    .get("required")
                    .and_then(|value| value.as_array())
                    .unwrap_or_else(|| panic!("{path} has no required list"));
                for name in properties.keys() {
                    assert!(
                        required.iter().any(|entry| entry == name),
                        "{path}.{name} is optional"
                    );
                }
                assert_eq!(
                    object.get("additionalProperties"),
                    Some(&serde_json::Value::Bool(false)),
                    "{path} is open"
                );
            }
            if object.contains_key("enum") {
                assert!(object.contains_key("type"), "{path} has an untyped enum");
            }
            for (key, child) in object {
                walk(&format!("{path}.{key}"), child);
            }
        }
        for specification in prompt_specifications() {
            let schema: serde_json::Value =
                serde_json::from_str(specification.output_schema).unwrap();
            walk(specification.output_schema_id, &schema);
        }
    }

    #[test]
    fn prompt_catalog_matches_the_frozen_fixture_and_invariants() {
        validate_catalog().unwrap();
        let fixture: CatalogFixture = serde_json::from_str(include_str!(
            "../../../fixtures/prompts/prompt-catalog-v2.json"
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
