use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use uuid::Uuid;

use super::AppError;

const TECHNICAL_INTERVIEW_PRESET_ID: &str = "00000000-0000-4000-8000-000000000001";
const JSON_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppSettingsUpdate {
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_preset_id: Option<PresetId>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_transcription_model_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) llm_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) retain_audio_by_default: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) require_zero_data_retention: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deny_provider_data_collection: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens_per_request: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_session_budget_usd: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAppSettingsUpdate {
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    default_preset_id: Option<PresetId>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    default_transcription_model_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    llm_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    retain_audio_by_default: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    require_zero_data_retention: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    deny_provider_data_collection: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    max_tokens_per_request: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    default_session_budget_usd: Option<String>,
}

impl<'de> Deserialize<'de> for AppSettingsUpdate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawAppSettingsUpdate::deserialize(deserializer)?;
        let update = Self {
            default_preset_id: raw.default_preset_id,
            default_transcription_model_id: raw.default_transcription_model_id,
            llm_enabled: raw.llm_enabled,
            retain_audio_by_default: raw.retain_audio_by_default,
            require_zero_data_retention: raw.require_zero_data_retention,
            deny_provider_data_collection: raw.deny_provider_data_collection,
            max_tokens_per_request: raw.max_tokens_per_request,
            default_session_budget_usd: raw.default_session_budget_usd,
        };
        update.validate().map_err(D::Error::custom)?;
        Ok(update)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Versioned<T> {
    pub(crate) expected_revision: u64,
    pub(crate) value: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawVersioned<T> {
    expected_revision: u64,
    value: T,
}

impl<'de, T> Deserialize<'de> for Versioned<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawVersioned::<T>::deserialize(deserializer)?;
        if raw.expected_revision > JSON_SAFE_INTEGER_MAX {
            return Err(D::Error::custom(
                "The expected revision exceeds the JSON safe-integer range.",
            ));
        }
        Ok(Self {
            expected_revision: raw.expected_revision,
            value: raw.value,
        })
    }
}

impl<T> Versioned<T> {
    pub(crate) fn new(expected_revision: u64, value: T) -> Result<Self, AppError> {
        if expected_revision > JSON_SAFE_INTEGER_MAX {
            return Err(AppError::settings_update_invalid(
                "The expected revision exceeds the JSON safe-integer range.",
            ));
        }
        Ok(Self {
            expected_revision,
            value,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceStatus {
    pub(crate) path: String,
    pub(crate) writable: bool,
    pub(crate) free_bytes: u64,
    #[serde(
        default,
        deserialize_with = "deserialize_non_null_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) warning: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawWorkspaceStatus {
    path: String,
    writable: bool,
    free_bytes: u64,
    #[serde(default, deserialize_with = "deserialize_non_null_optional")]
    warning: Option<String>,
}

impl<'de> Deserialize<'de> for WorkspaceStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawWorkspaceStatus::deserialize(deserializer)?;
        if !has_bounded_utf16_length(&raw.path, 1, 32_767) {
            return Err(D::Error::custom(
                "The workspace status path length is invalid.",
            ));
        }
        if raw.free_bytes > JSON_SAFE_INTEGER_MAX {
            return Err(D::Error::custom(
                "The workspace free-space value exceeds the JSON safe-integer range.",
            ));
        }
        if raw
            .warning
            .as_deref()
            .is_some_and(|warning| !has_bounded_utf16_length(warning, 1, 512))
        {
            return Err(D::Error::custom(
                "The workspace status warning length is invalid.",
            ));
        }
        Ok(Self {
            path: raw.path,
            writable: raw.writable,
            free_bytes: raw.free_bytes,
            warning: raw.warning,
        })
    }
}

fn deserialize_non_null_optional<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
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

    value.len() <= 18
        && !whole.is_empty()
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

    pub(crate) fn apply_update(mut self, update: AppSettingsUpdate) -> Result<Self, AppError> {
        update
            .validate()
            .map_err(AppError::settings_update_invalid)?;

        if let Some(value) = update.default_preset_id {
            self.default_preset_id = value;
        }
        if let Some(value) = update.default_transcription_model_id {
            self.default_transcription_model_id = value;
        }
        if let Some(value) = update.llm_enabled {
            self.llm_enabled = value;
        }
        if let Some(value) = update.retain_audio_by_default {
            self.retain_audio_by_default = value;
        }
        if let Some(value) = update.require_zero_data_retention {
            self.require_zero_data_retention = value;
        }
        if let Some(value) = update.deny_provider_data_collection {
            self.deny_provider_data_collection = value;
        }
        if let Some(value) = update.max_tokens_per_request {
            self.max_tokens_per_request = value;
        }
        if let Some(value) = update.default_session_budget_usd {
            self.default_session_budget_usd = value;
        }

        self.increment_revision()?;
        self.validate_transport()
            .map_err(AppError::settings_update_invalid)?;
        Ok(self)
    }

    pub(crate) fn with_workspace_path(mut self, workspace_path: String) -> Result<Self, AppError> {
        self.workspace_path = workspace_path;
        self.increment_revision()?;
        self.validate_transport()
            .map_err(AppError::settings_update_invalid)?;
        Ok(self)
    }

    pub(crate) fn with_workspace_path_at_current_revision(
        mut self,
        workspace_path: String,
    ) -> Result<Self, AppError> {
        self.workspace_path = workspace_path;
        self.validate_transport()
            .map_err(AppError::settings_update_invalid)?;
        Ok(self)
    }

    pub(crate) fn with_revision(mut self, revision: u64) -> Result<Self, AppError> {
        self.revision = revision;
        self.validate_transport()
            .map_err(AppError::settings_update_invalid)?;
        Ok(self)
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn workspace_path(&self) -> &str {
        &self.workspace_path
    }

    fn increment_revision(&mut self) -> Result<(), AppError> {
        self.revision = self
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= JSON_SAFE_INTEGER_MAX)
            .ok_or_else(AppError::settings_revision_exhausted)?;
        Ok(())
    }

    pub(crate) fn validate_transport(&self) -> Result<(), &'static str> {
        if self.revision > JSON_SAFE_INTEGER_MAX {
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

impl AppSettingsUpdate {
    fn validate(&self) -> Result<(), &'static str> {
        if self.is_empty() {
            return Err("At least one settings field must be changed.");
        }
        if self
            .default_transcription_model_id
            .as_deref()
            .is_some_and(|value| !has_bounded_utf16_length(value, 1, 128))
        {
            return Err("The transcription model identifier length is invalid.");
        }
        if self
            .max_tokens_per_request
            .is_some_and(|value| !(1..=1_000_000).contains(&value))
        {
            return Err("The maximum token count is outside its transport range.");
        }
        if self
            .default_session_budget_usd
            .as_deref()
            .is_some_and(|value| !is_fixed_decimal(value))
        {
            return Err("The default session budget must use two decimal places.");
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.default_preset_id.is_none()
            && self.default_transcription_model_id.is_none()
            && self.llm_enabled.is_none()
            && self.retain_audio_by_default.is_none()
            && self.require_zero_data_retention.is_none()
            && self.deny_provider_data_collection.is_none()
            && self.max_tokens_per_request.is_none()
            && self.default_session_budget_usd.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppSettings, AppSettingsUpdate, JSON_SAFE_INTEGER_MAX, Versioned, WorkspaceStatus,
    };

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

    #[test]
    fn settings_update_matches_the_shared_golden_contract() {
        let fixture =
            include_str!("../../../fixtures/contracts/versioned-app-settings-update-v1.json");
        let parsed: Versioned<AppSettingsUpdate> =
            serde_json::from_str(fixture).expect("golden update fixture should be valid");
        let expected: serde_json::Value = serde_json::from_str(fixture).unwrap();

        assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
    }

    #[test]
    fn settings_update_rejects_empty_unknown_null_and_invalid_values() {
        assert!(serde_json::from_str::<AppSettingsUpdate>("{}").is_err());
        assert!(serde_json::from_str::<AppSettingsUpdate>(r#"{"llmEnabled":null}"#).is_err());
        assert!(serde_json::from_str::<AppSettingsUpdate>(r#"{"unexpected":true}"#).is_err());
        assert!(serde_json::from_str::<AppSettingsUpdate>(r#"{"maxTokensPerRequest":0}"#).is_err());
        assert!(
            serde_json::from_str::<Versioned<AppSettingsUpdate>>(&format!(
                r#"{{"expectedRevision":{},"value":{{"llmEnabled":false}}}}"#,
                JSON_SAFE_INTEGER_MAX + 1
            ))
            .is_err()
        );
    }

    #[test]
    fn workspace_status_matches_the_shared_golden_contract() {
        let fixture = include_str!("../../../fixtures/contracts/workspace-status-v1.json");
        let parsed: WorkspaceStatus =
            serde_json::from_str(fixture).expect("golden workspace fixture should be valid");
        let expected: serde_json::Value = serde_json::from_str(fixture).unwrap();

        assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
    }

    #[test]
    fn workspace_status_rejects_values_outside_the_typescript_contract() {
        let fixture = include_str!("../../../fixtures/contracts/workspace-status-v1.json");
        for (field, invalid_value) in [
            ("path", serde_json::json!("")),
            ("freeBytes", serde_json::json!(JSON_SAFE_INTEGER_MAX + 1)),
            ("warning", serde_json::Value::Null),
            ("warning", serde_json::json!("😀".repeat(257))),
        ] {
            let mut invalid: serde_json::Value = serde_json::from_str(fixture).unwrap();
            invalid[field] = invalid_value;
            assert!(
                serde_json::from_value::<WorkspaceStatus>(invalid).is_err(),
                "{field} should reject the invalid value"
            );
        }
    }
}
