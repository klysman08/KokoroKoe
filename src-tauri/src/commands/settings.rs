use tauri::{AppHandle, Manager, WebviewWindow};

use crate::{
    domain::{AppError, AppSettings, CommandError},
    logging,
};

const AUTHORIZED_WINDOW: &str = "main";

#[tauri::command]
pub(crate) fn get_settings(
    app_handle: AppHandle,
    webview_window: WebviewWindow,
) -> Result<AppSettings, CommandError> {
    authorize_window(webview_window.label()).map_err(record_error)?;

    let documents_directory = app_handle.path().document_dir().map_err(|_| {
        record_error(AppError::settings_unavailable(
            "The Windows Documents directory could not be resolved.",
        ))
    })?;

    AppSettings::foundation_defaults(documents_directory).map_err(record_error)
}

fn authorize_window(window_label: &str) -> Result<(), AppError> {
    if window_label == AUTHORIZED_WINDOW {
        Ok(())
    } else {
        Err(AppError::command_not_authorized())
    }
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use super::authorize_window;

    #[test]
    fn settings_are_authorized_only_for_the_main_window() {
        assert!(authorize_window("main").is_ok());

        for label in ["transcript", "insights", "unknown"] {
            let error = authorize_window(label).expect_err("must reject other windows");
            assert_eq!(error.code, "command_not_authorized");
            assert!(!error.retryable);
        }
    }
}
