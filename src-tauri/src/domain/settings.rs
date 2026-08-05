use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use uuid::Uuid;

use super::AppError;

const TECHNICAL_INTERVIEW_PRESET_ID: &str = "00000000-0000-4000-8000-000000000001";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub(crate) struct PresetId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppSettings {
    pub(crate) revision: u64,
    pub(crate) workspace_path: String,
    pub(crate) default_preset_id: PresetId,
    pub(crate) default_transcription_model_id: String,
    pub(crate) llm_enabled: bool,
    pub(crate) retain_audio_by_default: bool,
    pub(crate) require_zero_data_retention: bool,
    pub(crate) deny_provider_data_collection: bool,
    pub(crate) max_tokens_per_request: u32,
    pub(crate) default_session_budget_usd: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAppSettings {
    revision: u64,
    workspace_path: String,
    default_preset_id: PresetId,
    default_transcription_model_id: String,
    llm_enabled: bool,
    retain_audio_by_default: bool,
    require_zero_data_retention: bool,
    deny_provider_data_collection: bool,
    max_tokens_per_request: u32,
    default_session_budget_usd: String,
}

impl<'de> Deserialize<'de> for AppSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawAppSettings::deserialize(deserializer)?;
        let settings = Self {
            revision: raw.revision,
            workspace_path: raw.workspace_path,
            default_preset_id: raw.default_preset_id,
            default_transcription_model_id: raw.default_transcription_model_id,
            llm_enabled: raw.llm_enabled,
            retain_audio_by_default: raw.retain_audio_by_default,
            require_zero_data_retention: raw.require_zero_data_retention,
            deny_provider_data_collection: raw.deny_provider_data_collection,
            max_tokens_per_request: raw.max_tokens_per_request,
            default_session_budget_usd: raw.default_session_budget_usd,
        };
        settings.validate_transport().map_err(D::Error::custom)?;
        Ok(settings)
    }
}

fn has_bounded_utf16_length(value: &str, minimum: usize, maximum: usize) -> bool {
    let length = value.encode_utf16().count();
    (minimum..=maximum).contains(&length)
}

fn is_fixed_decimal(value: &str) -> bool {
    let Some((whole, fractional)) = value.split_once('.') else {
        return false;
    };

    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fractional.len() == 2
        && fractional.bytes().all(|byte| byte.is_ascii_digit())
}

impl AppSettings {
    pub(crate) fn foundation_defaults(documents_directory: PathBuf) -> Result<Self, AppError> {
        let workspace_path = documents_directory
            .join("KokoroKoe")
            .into_os_string()
            .into_string()
            .map_err(|_| {
                AppError::settings_unavailable(
                    "The default workspace path is not valid Unicode and cannot be displayed.",
                )
            })?;
        let default_preset_id = Uuid::parse_str(TECHNICAL_INTERVIEW_PRESET_ID)
            .expect("the checked-in built-in preset ID must be a valid UUID");

        let settings = Self {
            revision: 0,
            workspace_path,
            default_preset_id: PresetId(default_preset_id),
            default_transcription_model_id: "whisper-base-multilingual".to_owned(),
            llm_enabled: false,
            retain_audio_by_default: false,
            require_zero_data_retention: true,
            deny_provider_data_collection: true,
            max_tokens_per_request: 2_048,
            default_session_budget_usd: "0.00".to_owned(),
        };
        settings
            .validate_transport()
            .map_err(AppError::settings_unavailable)?;
        Ok(settings)
    }

    fn validate_transport(&self) -> Result<(), &'static str> {
        if self.revision > 9_007_199_254_740_991 {
            return Err("The settings revision exceeds the JSON safe-integer range.");
        }
        if !has_bounded_utf16_length(&self.workspace_path, 1, 32_767) {
            return Err("The workspace path length is outside its transport range.");
        }
        if !has_bounded_utf16_length(&self.default_transcription_model_id, 1, 128) {
            return Err("The transcription model identifier length is invalid.");
        }
        if !(1..=1_000_000).contains(&self.max_tokens_per_request) {
            return Err("The maximum token count is outside its transport range.");
        }
        if !is_fixed_decimal(&self.default_session_budget_usd) {
            return Err("The default session budget must use two decimal places.");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AppSettings;

    #[test]
    fn app_settings_match_the_shared_golden_contract() {
        let fixture = include_str!("../../../fixtures/contracts/app-settings-v1.json");
        let parsed: AppSettings =
            serde_json::from_str(fixture).expect("golden fixture should match AppSettings");
        let expected: serde_json::Value =
            serde_json::from_str(fixture).expect("golden fixture should contain valid JSON");
        let actual = serde_json::to_value(parsed).expect("AppSettings should serialize");

        assert_eq!(actual, expected);
    }

    #[test]
    fn foundation_defaults_are_private_and_match_the_public_fixture() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/app-settings-v1.json"
        ))
        .expect("golden fixture should contain valid JSON");
        let actual = serde_json::to_value(
            AppSettings::foundation_defaults("C:\\Users\\Example\\Documents".into())
                .expect("foundation defaults should be constructible"),
        )
        .expect("AppSettings should serialize");

        assert_eq!(actual, expected);
        assert_eq!(actual["llmEnabled"], false);
        assert_eq!(actual["retainAudioByDefault"], false);
        assert_eq!(actual["requireZeroDataRetention"], true);
        assert_eq!(actual["denyProviderDataCollection"], true);
    }

    #[test]
    fn app_settings_reject_unknown_fields_and_invalid_preset_ids() {
        let fixture = include_str!("../../../fixtures/contracts/app-settings-v1.json");
        let mut unknown: serde_json::Value = serde_json::from_str(fixture).unwrap();
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<AppSettings>(unknown).is_err());

        let mut invalid_id: serde_json::Value = serde_json::from_str(fixture).unwrap();
        invalid_id["defaultPresetId"] = serde_json::json!("not-a-uuid");
        assert!(serde_json::from_value::<AppSettings>(invalid_id).is_err());
    }

    #[test]
    fn app_settings_reject_values_outside_the_typescript_contract() {
        let fixture = include_str!("../../../fixtures/contracts/app-settings-v1.json");

        for (field, invalid_value) in [
            ("revision", serde_json::json!(9_007_199_254_740_992_u64)),
            ("workspacePath", serde_json::json!("")),
            ("defaultTranscriptionModelId", serde_json::json!("")),
            (
                "defaultTranscriptionModelId",
                serde_json::json!("😀".repeat(65)),
            ),
            ("maxTokensPerRequest", serde_json::json!(0)),
            ("defaultSessionBudgetUsd", serde_json::json!("1.0")),
        ] {
            let mut invalid: serde_json::Value = serde_json::from_str(fixture).unwrap();
            invalid[field] = invalid_value;
            assert!(
                serde_json::from_value::<AppSettings>(invalid).is_err(),
                "{field} should reject the invalid value"
            );
        }
    }
}
