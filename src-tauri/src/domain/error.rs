use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use uuid::Uuid;

use crate::security::sanitize_technical_detail;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppError {
    pub(crate) code: String,
    pub(crate) user_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) technical_detail: Option<String>,
    pub(crate) severity: ErrorSeverity,
    pub(crate) retryable: bool,
    pub(crate) correlation_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawAppError {
    code: String,
    user_message: String,
    #[serde(default)]
    technical_detail: OptionalNonNull<String>,
    severity: ErrorSeverity,
    retryable: bool,
    correlation_id: Uuid,
}

enum OptionalNonNull<T> {
    Missing,
    Present(T),
}

impl<T> Default for OptionalNonNull<T> {
    fn default() -> Self {
        Self::Missing
    }
}

impl<'de, T> Deserialize<'de> for OptionalNonNull<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Self::Present)
    }
}

impl<'de> Deserialize<'de> for AppError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawAppError::deserialize(deserializer)?;
        if !has_bounded_utf16_length(&raw.code, 1, 128) {
            return Err(D::Error::custom("AppError code length is invalid"));
        }
        if !has_bounded_utf16_length(&raw.user_message, 1, 512) {
            return Err(D::Error::custom("AppError userMessage length is invalid"));
        }
        let technical_detail = match raw.technical_detail {
            OptionalNonNull::Missing => None,
            OptionalNonNull::Present(value) => {
                if !has_bounded_utf16_length(&value, 1, 512) {
                    return Err(D::Error::custom(
                        "AppError technicalDetail length is invalid",
                    ));
                }
                Some(value)
            }
        };

        Ok(Self {
            code: raw.code,
            user_message: raw.user_message,
            technical_detail,
            severity: raw.severity,
            retryable: raw.retryable,
            correlation_id: raw.correlation_id,
        })
    }
}

fn has_bounded_utf16_length(value: &str, minimum: usize, maximum: usize) -> bool {
    let length = value.encode_utf16().count();
    (minimum..=maximum).contains(&length)
}

impl AppError {
    pub(crate) fn command_not_authorized() -> Self {
        Self::new(
            "command_not_authorized",
            "This window is not allowed to perform that application operation.",
            Some("The command is restricted to the main application window."),
            ErrorSeverity::Warning,
            false,
        )
    }

    pub(crate) fn audio_operation_failed(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "audio_endpoint_id_invalid" | "audio_source_direction_mismatch" => (
                "The selected audio device is not valid for this test.",
                ErrorSeverity::Warning,
                false,
            ),
            "audio_device_test_already_running" => (
                "A test is already running for this audio source.",
                ErrorSeverity::Info,
                false,
            ),
            "audio_device_test_request_conflict" => (
                "That audio test request has already been used.",
                ErrorSeverity::Warning,
                false,
            ),
            "audio_device_test_not_running" => (
                "That audio device test is no longer running.",
                ErrorSeverity::Info,
                false,
            ),
            "audio_device_unavailable" => (
                "The selected audio device is unavailable.",
                ErrorSeverity::Warning,
                true,
            ),
            _ => (
                "Windows could not complete the audio device operation.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn settings_unavailable(technical_detail: &str) -> Self {
        Self::new(
            "settings_unavailable",
            "KokoroKoe could not load the foundation settings.",
            Some(technical_detail),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn settings_update_invalid(technical_detail: &str) -> Self {
        Self::new(
            "settings_update_invalid",
            "The settings change is not valid.",
            Some(technical_detail),
            ErrorSeverity::Warning,
            false,
        )
    }

    pub(crate) fn settings_revision_conflict() -> Self {
        Self::new(
            "settings_revision_conflict",
            "Settings changed since this screen was loaded. Reload and try again.",
            Some("The expected settings revision did not match the stored revision."),
            ErrorSeverity::Warning,
            false,
        )
    }

    pub(crate) fn settings_revision_exhausted() -> Self {
        Self::new(
            "settings_revision_exhausted",
            "KokoroKoe cannot save another settings revision.",
            Some("The settings revision reached the supported transport limit."),
            ErrorSeverity::Critical,
            false,
        )
    }

    pub(crate) fn settings_save_failed() -> Self {
        Self::new(
            "settings_save_failed",
            "KokoroKoe could not save the settings change.",
            Some("The local settings transaction did not complete."),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn settings_database_corrupt() -> Self {
        Self::new(
            "settings_database_corrupt",
            "The local settings database is damaged and must be rebuilt.",
            Some("SQLite reported physical corruption in the non-secret settings database."),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn workspace_selection_cancelled() -> Self {
        Self::new(
            "workspace_selection_cancelled",
            "Workspace selection was cancelled.",
            Some("The native folder picker closed without a selection."),
            ErrorSeverity::Info,
            false,
        )
    }

    pub(crate) fn workspace_selection_in_progress() -> Self {
        Self::new(
            "workspace_selection_in_progress",
            "A workspace folder picker is already open.",
            Some("Only one workspace selection can run at a time."),
            ErrorSeverity::Info,
            false,
        )
    }

    pub(crate) fn workspace_invalid(technical_detail: &str) -> Self {
        Self::new(
            "workspace_invalid",
            "That folder cannot be used as a KokoroKoe workspace.",
            Some(technical_detail),
            ErrorSeverity::Warning,
            false,
        )
    }

    pub(crate) fn workspace_unwritable() -> Self {
        Self::new(
            "workspace_unwritable",
            "KokoroKoe cannot write to that workspace folder.",
            Some("The temporary write and synchronization probe failed."),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn settings_worker_failed() -> Self {
        Self::new(
            "settings_worker_failed",
            "The local settings service stopped unexpectedly.",
            Some("The background settings operation did not complete."),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn model_operation_failed(technical_detail: &str) -> Self {
        Self::new(
            "model_operation_failed",
            "KokoroKoe could not complete the local model operation.",
            Some(technical_detail),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn model_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "model_unknown" => (
                "That transcription model is not in the KokoroKoe catalog.",
                ErrorSeverity::Warning,
                false,
            ),
            "model_download_in_progress" => (
                "Another transcription model download is already running.",
                ErrorSeverity::Info,
                false,
            ),
            "model_download_not_resumable" => (
                "That transcription model has no interrupted download to resume.",
                ErrorSeverity::Warning,
                false,
            ),
            "model_download_not_found" => (
                "That model download job is no longer available.",
                ErrorSeverity::Warning,
                false,
            ),
            "model_not_installed" | "installed_model_invalid" => (
                "Install and verify that transcription model before selecting it.",
                ErrorSeverity::Warning,
                false,
            ),
            "selected_model_cannot_be_deleted" => (
                "Choose another installed default model before deleting this one.",
                ErrorSeverity::Warning,
                false,
            ),
            "model_active_job_cannot_be_deleted" => (
                "Cancel the active model download before deleting it.",
                ErrorSeverity::Warning,
                false,
            ),
            "insufficient_model_disk" => (
                "There is not enough local disk space to install this model.",
                ErrorSeverity::Warning,
                true,
            ),
            "model_download_cancelled" => (
                "The model download was cancelled and can be resumed later.",
                ErrorSeverity::Info,
                false,
            ),
            "model_settings_route_required" => (
                "Use the model manager to change the default transcription model.",
                ErrorSeverity::Warning,
                false,
            ),
            _ => (
                "KokoroKoe could not complete the local model operation.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    fn new(
        code: &str,
        user_message: &str,
        technical_detail: Option<&str>,
        severity: ErrorSeverity,
        retryable: bool,
    ) -> Self {
        Self {
            code: bounded_nonempty(code, 128, "application_error"),
            user_message: bounded_nonempty(
                user_message,
                512,
                "KokoroKoe encountered an application error.",
            ),
            technical_detail: technical_detail.map(sanitize_technical_detail),
            severity,
            retryable,
            correlation_id: Uuid::new_v4(),
        }
    }
}

fn bounded_nonempty(value: &str, maximum_utf16_units: usize, fallback: &str) -> String {
    let selected = if value.is_empty() { fallback } else { value };
    let mut used_units = 0;

    selected
        .chars()
        .take_while(|character| {
            let next_units = character.len_utf16();
            if used_units + next_units > maximum_utf16_units {
                false
            } else {
                used_units += next_units;
                true
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandError {
    pub(crate) error: AppError,
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        Self { error }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, ErrorSeverity};

    #[test]
    fn app_error_serializes_with_the_public_camel_case_contract() {
        let error = AppError::new(
            "test_error",
            "A test failed.",
            Some("Stable detail"),
            ErrorSeverity::Info,
            false,
        );
        let value = serde_json::to_value(error).expect("AppError should serialize");

        assert_eq!(value["code"], "test_error");
        assert_eq!(value["userMessage"], "A test failed.");
        assert_eq!(value["technicalDetail"], "Stable detail");
        assert_eq!(value["severity"], "info");
        assert!(value["correlationId"].as_str().is_some());
    }

    #[test]
    fn all_error_severities_keep_the_fixed_transport_values() {
        let values = [
            (ErrorSeverity::Info, "info"),
            (ErrorSeverity::Warning, "warning"),
            (ErrorSeverity::Error, "error"),
            (ErrorSeverity::Critical, "critical"),
        ];

        for (severity, expected) in values {
            assert_eq!(serde_json::to_value(severity).unwrap(), expected);
        }
    }

    #[test]
    fn command_error_matches_the_shared_golden_contract() {
        let fixture = include_str!("../../../fixtures/contracts/command-error-v1.json");
        let parsed: super::CommandError =
            serde_json::from_str(fixture).expect("golden fixture should match CommandError");
        let expected: serde_json::Value =
            serde_json::from_str(fixture).expect("golden fixture should contain valid JSON");

        assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
    }

    #[test]
    fn command_error_rejects_invalid_or_nullable_public_fields() {
        let fixture = include_str!("../../../fixtures/contracts/command-error-v1.json");

        for (field, invalid_value) in [
            ("code", serde_json::json!("")),
            ("userMessage", serde_json::json!("")),
            ("technicalDetail", serde_json::Value::Null),
            ("technicalDetail", serde_json::json!("😀".repeat(257))),
            ("correlationId", serde_json::json!("not-a-uuid")),
        ] {
            let mut invalid: serde_json::Value = serde_json::from_str(fixture).unwrap();
            invalid["error"][field] = invalid_value;
            assert!(
                serde_json::from_value::<super::CommandError>(invalid).is_err(),
                "{field} should reject the invalid value"
            );
        }
    }
}
