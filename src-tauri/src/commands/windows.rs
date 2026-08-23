use tauri::{AppHandle, WebviewWindow};

use crate::{
    domain::{AppError, CommandError},
    logging,
    security::authorize_main_window,
    windows,
};

#[tauri::command]
pub(crate) async fn open_transcript_window<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
) -> Result<(), CommandError> {
    authorized(webview_window.label(), || {
        windows::open_transcript_window(&app)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn close_transcript_window<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
) -> Result<(), CommandError> {
    authorized(webview_window.label(), || {
        windows::close_transcript_window(&app)
    })
    .map_err(record_error)
}

fn authorized<T>(
    label: &str,
    operation: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    authorize_main_window(label)?;
    operation()
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::domain::AppError;

    /// The transcript window must never be able to open or close windows
    /// itself, so its own label is rejected alongside every other non-main one.
    #[test]
    fn window_commands_authorize_before_touching_any_window() {
        let operations = AtomicUsize::new(0);
        for label in ["transcript", "insights", "Main", "unknown"] {
            let error = super::authorized(label, || {
                operations.fetch_add(1, Ordering::SeqCst);
                Ok::<(), AppError>(())
            })
            .unwrap_err();
            assert_eq!(error.code, "command_not_authorized");
        }
        assert_eq!(operations.load(Ordering::SeqCst), 0);
        assert!(super::authorized("main", || Ok::<(), AppError>(())).is_ok());
        assert_eq!(operations.load(Ordering::SeqCst), 0);
    }
}
