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
    pub(crate) fn window_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "window_open_failed" => (
                "KokoroKoe could not open the transcript window.",
                ErrorSeverity::Error,
                true,
            ),
            "window_focus_failed" => (
                "KokoroKoe could not bring the transcript window to the front.",
                ErrorSeverity::Warning,
                true,
            ),
            "window_close_failed" => (
                "KokoroKoe could not close the transcript window.",
                ErrorSeverity::Warning,
                true,
            ),
            // The user can act on this one: the main window is where
            // click-through is turned back off.
            "window_click_through_unrecoverable" => (
                "Keep the main KokoroKoe window open to let clicks pass through the transcript window. It is the only place to turn that back off.",
                ErrorSeverity::Warning,
                false,
            ),
            _ => (
                "KokoroKoe could not complete the window operation.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn summary_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "summary_invalid" => (
                "Open a saved Session before generating its summary.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_session_not_finished" => (
                "Stop the Session before generating its final summary.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_transcript_empty" => (
                "This Session has no finalized transcript to summarize.",
                ErrorSeverity::Info,
                false,
            ),
            "summary_model_required" => (
                "Choose a summaries model for this Session before generating its summary.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_catalog_required" => (
                "Refresh the privacy-filtered OpenRouter model list before generating a summary.",
                ErrorSeverity::Warning,
                true,
            ),
            "summary_credential_required" => (
                "Add an OpenRouter API key before generating a summary.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_authentication_failed" => (
                "OpenRouter did not accept the configured API key.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_payment_required" => (
                "The OpenRouter account does not have enough credit for this summary.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_budget_exceeded" => (
                "This summary would exceed the Session spending limit.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_context_too_large" => (
                "This Session's transcript does not fit its model limit. Choose a model with a larger context.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_cancelled" => (
                "Summary generation was cancelled.",
                ErrorSeverity::Info,
                false,
            ),
            "summary_rate_limited" => (
                "OpenRouter is rate limiting requests. Try again later.",
                ErrorSeverity::Info,
                true,
            ),
            "summary_provider_requirements_unavailable" => (
                "The selected model has no available private provider that supports this structured request. Refresh the model list or choose another model.",
                ErrorSeverity::Warning,
                true,
            ),
            "summary_request_rejected" => (
                "OpenRouter rejected this summary request. Refresh the model list or choose another model.",
                ErrorSeverity::Warning,
                true,
            ),
            "summary_timeout" => (
                "OpenRouter took too long to return a summary. Try again.",
                ErrorSeverity::Info,
                true,
            ),
            "summary_network_unavailable" => (
                "KokoroKoe could not reach OpenRouter. Check the network and try again.",
                ErrorSeverity::Info,
                true,
            ),
            "summary_provider_temporarily_unavailable" => (
                "The selected OpenRouter provider is temporarily unavailable. Try again or choose another model.",
                ErrorSeverity::Info,
                true,
            ),
            "summary_content_filtered" => (
                "The selected model could not return a summary for this transcript.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_response_invalid" => (
                "The selected model returned an unsupported summary.",
                ErrorSeverity::Error,
                true,
            ),
            "summary_document_invalid" | "summary_document_identity_mismatch" => (
                "The saved summary document could not be read. Generate the summary again to replace it.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_document_too_large" => (
                "The saved summary document is too large to read.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_document_publish_failed"
            | "summary_document_verify_failed"
            | "summary_document_read_failed" => (
                "KokoroKoe could not save the summary document.",
                ErrorSeverity::Error,
                true,
            ),
            "summary_session_missing" | "summary_session_unavailable" => (
                "That Session is no longer available in the workspace.",
                ErrorSeverity::Warning,
                false,
            ),
            "summary_worker_failed" => (
                "Summary generation stopped unexpectedly.",
                ErrorSeverity::Error,
                true,
            ),
            _ => (
                "KokoroKoe could not generate the summary.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn insight_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "insight_invalid" => (
                "Open a saved Session before generating insights.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_transcript_empty" => (
                "No finalized transcript is available yet. Wait for speech to be transcribed and try again.",
                ErrorSeverity::Info,
                true,
            ),
            "insight_model_required" => (
                "Choose an insights model for this Session before generating insights.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_types_required" => (
                "This Session's preset requests no insight types.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_catalog_required" => (
                "Refresh the privacy-filtered OpenRouter model list before generating insights.",
                ErrorSeverity::Warning,
                true,
            ),
            "insight_credential_required" => (
                "Add an OpenRouter API key before generating insights.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_authentication_failed" => (
                "OpenRouter did not accept the configured API key.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_payment_required" => (
                "The OpenRouter account does not have enough credit for these insights.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_budget_exceeded" => (
                "Generating insights would exceed the Session spending limit.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_context_too_large" => (
                "The recent transcript does not fit this Session's model limit.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_cancelled" => (
                "Insight generation was cancelled.",
                ErrorSeverity::Info,
                false,
            ),
            "insight_rate_limited" => (
                "OpenRouter is rate limiting requests. Try again later.",
                ErrorSeverity::Info,
                true,
            ),
            "insight_provider_requirements_unavailable" => (
                "The selected model has no available private provider that supports this structured request. Refresh the model list or choose another model.",
                ErrorSeverity::Warning,
                true,
            ),
            "insight_request_rejected" => (
                "OpenRouter rejected this insight request. Refresh the model list or choose another model.",
                ErrorSeverity::Warning,
                true,
            ),
            "insight_timeout" => (
                "OpenRouter took too long to return insights. Try again.",
                ErrorSeverity::Info,
                true,
            ),
            "insight_network_unavailable" => (
                "KokoroKoe could not reach OpenRouter. Check the network and try again.",
                ErrorSeverity::Info,
                true,
            ),
            "insight_provider_temporarily_unavailable" => (
                "The selected OpenRouter provider is temporarily unavailable. Try again or choose another model.",
                ErrorSeverity::Info,
                true,
            ),
            "insight_content_filtered" => (
                "The selected model could not return insights for this transcript.",
                ErrorSeverity::Warning,
                false,
            ),
            "insight_response_invalid" => (
                "The selected model returned unsupported insights.",
                ErrorSeverity::Error,
                true,
            ),
            "insight_worker_failed" => (
                "Insight generation stopped unexpectedly.",
                ErrorSeverity::Error,
                true,
            ),
            _ => (
                "KokoroKoe could not generate insights.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn manual_question_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "manual_question_invalid" => (
                "Enter a valid question for that transcript segment.",
                ErrorSeverity::Warning,
                false,
            ),
            "manual_question_segment_not_found" => (
                "That finalized transcript segment is no longer available.",
                ErrorSeverity::Info,
                false,
            ),
            "manual_question_model_required" => (
                "Choose a manual-question model before asking OpenRouter.",
                ErrorSeverity::Warning,
                false,
            ),
            "manual_question_catalog_required" => (
                "Refresh the privacy-filtered OpenRouter model list before asking a question.",
                ErrorSeverity::Warning,
                true,
            ),
            "manual_question_credential_required" => (
                "Add an OpenRouter API key before asking an online-model question.",
                ErrorSeverity::Warning,
                false,
            ),
            "manual_question_authentication_failed" => (
                "OpenRouter did not accept the configured API key.",
                ErrorSeverity::Warning,
                false,
            ),
            "manual_question_payment_required" => (
                "The OpenRouter account does not have enough credit for this question.",
                ErrorSeverity::Warning,
                false,
            ),
            "manual_question_budget_exceeded" => (
                "This question would exceed the Session spending limit.",
                ErrorSeverity::Warning,
                false,
            ),
            "manual_question_context_too_large" => (
                "The selected transcript segment does not fit this Session's model limit.",
                ErrorSeverity::Warning,
                false,
            ),
            "manual_question_cancelled" => (
                "The OpenRouter question was cancelled.",
                ErrorSeverity::Info,
                false,
            ),
            "manual_question_rate_limited" => (
                "OpenRouter is rate limiting questions. Try again later.",
                ErrorSeverity::Info,
                true,
            ),
            "manual_question_provider_requirements_unavailable" => (
                "The selected model has no available private provider that supports this structured question. Refresh the model list or choose another model.",
                ErrorSeverity::Warning,
                true,
            ),
            "manual_question_request_rejected" => (
                "OpenRouter rejected this question. Refresh the model list or choose another model.",
                ErrorSeverity::Warning,
                true,
            ),
            "manual_question_timeout" => (
                "OpenRouter took too long to answer. Try the question again.",
                ErrorSeverity::Info,
                true,
            ),
            "manual_question_network_unavailable" => (
                "KokoroKoe could not reach OpenRouter. Check the network and try again.",
                ErrorSeverity::Info,
                true,
            ),
            "manual_question_provider_temporarily_unavailable" => (
                "The selected OpenRouter provider is temporarily unavailable. Try again or choose another model.",
                ErrorSeverity::Info,
                true,
            ),
            "manual_question_content_filtered" => (
                "The selected model could not return an answer for this question.",
                ErrorSeverity::Warning,
                false,
            ),
            "manual_question_response_invalid" => (
                "The selected model returned an unsupported answer.",
                ErrorSeverity::Error,
                true,
            ),
            "manual_question_worker_failed" => (
                "The OpenRouter question stopped unexpectedly.",
                ErrorSeverity::Error,
                true,
            ),
            _ => (
                "KokoroKoe could not complete the OpenRouter question.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn openrouter_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "openrouter_credential_missing" => (
                "Add an OpenRouter API key before using online model features.",
                ErrorSeverity::Warning,
                false,
            ),
            "openrouter_credential_invalid" | "openrouter_authentication_failed" => (
                "OpenRouter did not accept the configured API key.",
                ErrorSeverity::Warning,
                false,
            ),
            "openrouter_permission_denied" => (
                "The OpenRouter API key does not allow this operation.",
                ErrorSeverity::Warning,
                false,
            ),
            "openrouter_payment_required" => (
                "The OpenRouter account does not have enough credit for this operation.",
                ErrorSeverity::Warning,
                false,
            ),
            "openrouter_rate_limited" => (
                "OpenRouter is rate limiting requests. Try again later.",
                ErrorSeverity::Info,
                true,
            ),
            "openrouter_timeout" | "openrouter_network_unavailable" => (
                "KokoroKoe could not reach OpenRouter.",
                ErrorSeverity::Warning,
                true,
            ),
            "openrouter_response_invalid"
            | "openrouter_response_too_large"
            | "openrouter_model_catalog_invalid" => (
                "OpenRouter returned an unsupported response.",
                ErrorSeverity::Error,
                true,
            ),
            "openrouter_catalog_required" => (
                "Refresh the OpenRouter model list before saving model defaults.",
                ErrorSeverity::Warning,
                true,
            ),
            "openrouter_model_not_available" => (
                "Choose models from the current privacy-filtered OpenRouter catalog.",
                ErrorSeverity::Warning,
                false,
            ),
            "openrouter_request_rejected" => (
                "OpenRouter rejected the request.",
                ErrorSeverity::Warning,
                false,
            ),
            _ => (
                "The OpenRouter service is temporarily unavailable.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn credential_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "credential_key_invalid" => (
                "Enter a valid OpenRouter API key without surrounding whitespace.",
                ErrorSeverity::Warning,
                false,
            ),
            "credential_delete_unavailable" => (
                "The OpenRouter API key could not be removed from Windows Credential Manager.",
                ErrorSeverity::Error,
                true,
            ),
            _ => (
                "Windows Credential Manager is unavailable for the OpenRouter API key.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn transcript_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "transcript_page_invalid"
            | "transcript_page_cursor_invalid"
            | "transcript_search_request_invalid"
            | "transcript_search_cursor_invalid" => (
                "The transcript request is not valid.",
                ErrorSeverity::Warning,
                false,
            ),
            "project_not_found" | "session_not_found" => (
                "That saved transcript is no longer available.",
                ErrorSeverity::Info,
                false,
            ),
            "transcript_snapshot_missing" => (
                "This session does not have a saved transcript yet.",
                ErrorSeverity::Info,
                false,
            ),
            "transcript_page_stale" | "transcript_search_page_stale" => (
                "The saved transcript changed. Reload it to continue.",
                ErrorSeverity::Info,
                false,
            ),
            "transcript_snapshot_invalid"
            | "transcript_snapshot_recovery_failed"
            | "transcript_search_index_invalid" => (
                "The saved transcript could not be verified.",
                ErrorSeverity::Error,
                true,
            ),
            _ => (
                "KokoroKoe could not complete the local transcript operation.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn transcript_worker_failed() -> Self {
        Self::new(
            "transcript_worker_failed",
            "The local transcript service stopped unexpectedly.",
            Some("The background transcript operation did not complete."),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn session_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "session_contract_invalid" | "session_update_invalid" | "session_page_invalid" => (
                "The session information is not valid.",
                ErrorSeverity::Warning,
                false,
            ),
            "session_not_found" => (
                "That session is no longer available.",
                ErrorSeverity::Info,
                false,
            ),
            "project_not_found" => (
                "The project for that session is no longer available.",
                ErrorSeverity::Info,
                false,
            ),
            "session_revision_conflict" | "session_external_modification" => (
                "The session changed since it was loaded. Reload it and try again.",
                ErrorSeverity::Warning,
                false,
            ),
            "session_lifecycle_invalid" => (
                "That session cannot perform this lifecycle action in its current state.",
                ErrorSeverity::Warning,
                false,
            ),
            "session_already_running" | "session_not_running" => (
                "The persisted session run changed. Reload the session and try again.",
                ErrorSeverity::Info,
                false,
            ),
            "session_page_stale" => (
                "The session list changed. Reload the list to continue.",
                ErrorSeverity::Info,
                false,
            ),
            "session_projection_refresh_pending" => (
                "The session was saved, but the local list must be refreshed.",
                ErrorSeverity::Warning,
                false,
            ),
            "session_revision_exhausted" => (
                "KokoroKoe cannot save another revision of this session.",
                ErrorSeverity::Critical,
                false,
            ),
            _ => (
                "KokoroKoe could not complete the local session operation.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn session_worker_failed() -> Self {
        Self::new(
            "session_worker_failed",
            "The local session service stopped unexpectedly.",
            Some("The background session operation did not complete."),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn project_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "project_contract_invalid" | "project_update_invalid" | "project_page_invalid" => (
                "The project information is not valid.",
                ErrorSeverity::Warning,
                false,
            ),
            "project_not_found" => (
                "That project is no longer available.",
                ErrorSeverity::Info,
                false,
            ),
            "project_revision_conflict" | "project_external_modification" => (
                "The project changed since it was loaded. Reload it and try again.",
                ErrorSeverity::Warning,
                false,
            ),
            "project_page_stale" => (
                "The project list changed. Reload the list to continue.",
                ErrorSeverity::Info,
                false,
            ),
            "project_already_exists" => (
                "A project with that local identity already exists.",
                ErrorSeverity::Warning,
                false,
            ),
            "project_projection_refresh_pending" => (
                "The project was saved, but the local list must be refreshed.",
                ErrorSeverity::Warning,
                false,
            ),
            "project_revision_exhausted" => (
                "KokoroKoe cannot save another revision of this project.",
                ErrorSeverity::Critical,
                false,
            ),
            _ => (
                "KokoroKoe could not complete the local project operation.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

    pub(crate) fn project_worker_failed() -> Self {
        Self::new(
            "project_worker_failed",
            "The local project service stopped unexpectedly.",
            Some("The background project operation did not complete."),
            ErrorSeverity::Error,
            true,
        )
    }

    pub(crate) fn live_transcription_error(code: &str) -> Self {
        let (user_message, severity, retryable) = match code {
            "live_transcription_consent_required" => (
                "Acknowledge the recording and transcription notice before starting.",
                ErrorSeverity::Warning,
                false,
            ),
            "live_transcription_already_running" | "live_transcription_request_conflict" => (
                "A live transcription run is already active.",
                ErrorSeverity::Info,
                false,
            ),
            "live_transcription_not_running" => (
                "That live transcription run is no longer active.",
                ErrorSeverity::Info,
                false,
            ),
            "live_transcription_runtime_unavailable" => (
                "The local transcription runtime is not available in this build.",
                ErrorSeverity::Error,
                false,
            ),
            "model_not_installed" | "installed_model_invalid" => (
                "Install and select a verified transcription model before starting.",
                ErrorSeverity::Warning,
                false,
            ),
            _ => (
                "KokoroKoe could not complete the live transcription operation.",
                ErrorSeverity::Error,
                true,
            ),
        };
        Self::new(code, user_message, Some(code), severity, retryable)
    }

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

    pub(crate) fn workspace_open_failed() -> Self {
        Self::new(
            "workspace_open_failed",
            "KokoroKoe could not open the workspace folder in File Explorer.",
            Some("Windows File Explorer did not accept the verified workspace path."),
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
