use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_MODEL_NAME_BYTES: usize = 256;
const MAX_PROVIDER_BYTES: usize = 128;
const MAX_PRICE_BYTES: usize = 64;
const MAX_CONTEXT_LENGTH: u64 = 10_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CredentialValidation {
    pub(crate) valid: bool,
    pub(crate) validated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) usage_usd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) remaining_limit_usd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<String>,
}

impl CredentialValidation {
    pub(crate) fn valid_at(validated_at: String) -> Result<Self, &'static str> {
        validate_rfc3339(&validated_at)?;
        Ok(Self {
            valid: true,
            validated_at,
            usage_usd: None,
            remaining_limit_usd: None,
            expires_at: None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OpenRouterDataCollection {
    Allow,
    Deny,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OpenRouterModel {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) context_length: u64,
    pub(crate) prompt_price_per_token: String,
    pub(crate) completion_price_per_token: String,
    pub(crate) supports_structured_outputs: bool,
    pub(crate) supports_streaming: bool,
    pub(crate) zero_data_retention_available: bool,
    pub(crate) data_collection: OpenRouterDataCollection,
}

impl OpenRouterModel {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        validate_text(&self.id, MAX_MODEL_ID_BYTES)?;
        validate_text(&self.name, MAX_MODEL_NAME_BYTES)?;
        validate_text(&self.provider, MAX_PROVIDER_BYTES)?;
        if self.context_length == 0 || self.context_length > MAX_CONTEXT_LENGTH {
            return Err("openrouter_model_catalog_invalid");
        }
        validate_price(&self.prompt_price_per_token)?;
        validate_price(&self.completion_price_per_token)?;
        if !self.zero_data_retention_available
            || self.data_collection != OpenRouterDataCollection::Deny
        {
            return Err("openrouter_model_catalog_invalid");
        }
        Ok(())
    }
}

fn validate_text(value: &str, maximum_bytes: usize) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > maximum_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err("openrouter_model_catalog_invalid");
    }
    Ok(())
}

fn validate_price(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > MAX_PRICE_BYTES || value.trim() != value {
        return Err("openrouter_model_catalog_invalid");
    }
    let parsed = value
        .parse::<f64>()
        .map_err(|_| "openrouter_model_catalog_invalid")?;
    if !parsed.is_finite() || parsed < 0.0 {
        return Err("openrouter_model_catalog_invalid");
    }
    Ok(())
}

pub(crate) fn validate_rfc3339(value: &str) -> Result<(), &'static str> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|_| "openrouter_response_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_contract_fixtures_are_strict() {
        for fixture in [
            include_str!("../../../fixtures/contracts/openrouter-validation-v1.json"),
            include_str!("../../../fixtures/contracts/openrouter-model-v1.json"),
        ] {
            let value: serde_json::Value = serde_json::from_str(fixture).unwrap();
            if value.get("valid").is_some() {
                let parsed: CredentialValidation = serde_json::from_value(value.clone()).unwrap();
                assert_eq!(serde_json::to_value(parsed).unwrap(), value);
            } else {
                let parsed: OpenRouterModel = serde_json::from_value(value.clone()).unwrap();
                parsed.validate().unwrap();
                assert_eq!(serde_json::to_value(parsed).unwrap(), value);
            }
        }
    }

    #[test]
    fn model_contract_rejects_unbounded_or_non_private_values() {
        let fixture = include_str!("../../../fixtures/contracts/openrouter-model-v1.json");
        let model: OpenRouterModel = serde_json::from_str(fixture).unwrap();
        assert!(model.validate().is_ok());

        let mut invalid = model.clone();
        invalid.zero_data_retention_available = false;
        assert_eq!(invalid.validate(), Err("openrouter_model_catalog_invalid"));
        invalid = model;
        invalid.prompt_price_per_token = "NaN".into();
        assert_eq!(invalid.validate(), Err("openrouter_model_catalog_invalid"));
    }
}
