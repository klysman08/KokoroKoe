use serde::{Deserialize, Serialize};

const MIN_API_KEY_BYTES: usize = 16;
const MAX_API_KEY_BYTES: usize = 2_048;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CredentialStatus {
    pub(crate) configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) validated_at: Option<String>,
}

impl CredentialStatus {
    pub(crate) const fn configured(configured: bool) -> Self {
        Self {
            configured,
            validated_at: None,
        }
    }

    pub(crate) fn validated(validated_at: String) -> Result<Self, &'static str> {
        crate::domain::validate_rfc3339(&validated_at)?;
        Ok(Self {
            configured: true,
            validated_at: Some(validated_at),
        })
    }
}

#[derive(Deserialize)]
#[serde(transparent)]
pub(crate) struct OpenRouterApiKey(String);

impl OpenRouterApiKey {
    pub(crate) fn from_bytes(mut bytes: Vec<u8>) -> Result<Self, &'static str> {
        let value = match String::from_utf8(bytes) {
            Ok(value) => value,
            Err(error) => {
                bytes = error.into_bytes();
                bytes.fill(0);
                return Err("credential_key_invalid");
            }
        };
        let key = Self(value);
        key.validate()?;
        Ok(key)
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let bytes = self.0.as_bytes();
        if !(MIN_API_KEY_BYTES..=MAX_API_KEY_BYTES).contains(&bytes.len())
            || self.0.trim() != self.0
            || self.0.chars().any(char::is_control)
        {
            return Err("credential_key_invalid");
        }
        Ok(())
    }

    pub(crate) fn expose(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Drop for OpenRouterApiKey {
    fn drop(&mut self) {
        // SAFETY: the bytes are overwritten without changing the String length or UTF-8 shape,
        // and the String is dropped immediately afterward without being observed again.
        unsafe { self.0.as_bytes_mut() }.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_contract_matches_the_shared_fixture() {
        let fixture = include_str!("../../../fixtures/contracts/credential-status-v1.json");
        let parsed: CredentialStatus = serde_json::from_str(fixture).unwrap();
        let expected: serde_json::Value = serde_json::from_str(fixture).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
    }

    #[test]
    fn api_key_validation_is_bounded_and_format_agnostic() {
        for invalid in [
            "short",
            " leading-key-material",
            "trailing-key-material ",
            "key-material-with\ncontrol",
        ] {
            let key: OpenRouterApiKey = serde_json::from_value(serde_json::json!(invalid)).unwrap();
            assert_eq!(key.validate(), Err("credential_key_invalid"));
        }
        let key: OpenRouterApiKey =
            serde_json::from_value(serde_json::json!("provider-format-can-change-1234")).unwrap();
        assert!(key.validate().is_ok());
    }
}
