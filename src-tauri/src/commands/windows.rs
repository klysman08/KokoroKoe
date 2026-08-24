use tauri::{AppHandle, State, WebviewWindow};

use crate::{
    domain::{
        AppError, CommandError, SetTranscriptWindowAppearanceRequest,
        SetTranscriptWindowInteractionRequest, SetTranscriptWindowShortcutRequest,
        TranscriptWindowAppearance, TranscriptWindowInteraction, TranscriptWindowShortcutStatus,
    },
    logging,
    security::authorize_main_window,
    windows::{InsightsWindowService, TranscriptWindowService},
};

#[tauri::command]
pub(crate) async fn open_transcript_window<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, TranscriptWindowService>,
) -> Result<(), CommandError> {
    authorized(webview_window.label(), || state.inner().clone().open(&app)).map_err(record_error)
}

#[tauri::command]
pub(crate) async fn close_transcript_window<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, TranscriptWindowService>,
) -> Result<(), CommandError> {
    authorized(webview_window.label(), || state.inner().clone().close(&app)).map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_transcript_window_appearance<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptWindowService>,
) -> Result<TranscriptWindowAppearance, CommandError> {
    authorized(webview_window.label(), || state.inner().appearance()).map_err(record_error)
}

#[tauri::command]
pub(crate) async fn set_transcript_window_appearance<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, TranscriptWindowService>,
    request: SetTranscriptWindowAppearanceRequest,
) -> Result<TranscriptWindowAppearance, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().clone().apply(&app, request)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_transcript_window_shortcut<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptWindowService>,
) -> Result<TranscriptWindowShortcutStatus, CommandError> {
    authorized(webview_window.label(), || state.inner().shortcut_status()).map_err(record_error)
}

#[tauri::command]
pub(crate) async fn set_transcript_window_shortcut<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptWindowService>,
    request: SetTranscriptWindowShortcutRequest,
) -> Result<TranscriptWindowShortcutStatus, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().set_shortcut(request)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_transcript_window_interaction<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptWindowService>,
) -> Result<TranscriptWindowInteraction, CommandError> {
    authorized(webview_window.label(), || state.inner().interaction()).map_err(record_error)
}

#[tauri::command]
pub(crate) async fn set_transcript_window_interaction<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, TranscriptWindowService>,
    request: SetTranscriptWindowInteractionRequest,
) -> Result<TranscriptWindowInteraction, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().set_click_through(&app, request)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn open_insights_window<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, InsightsWindowService>,
) -> Result<(), CommandError> {
    authorized(webview_window.label(), || state.inner().open(&app)).map_err(record_error)
}

#[tauri::command]
pub(crate) async fn close_insights_window<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, InsightsWindowService>,
) -> Result<(), CommandError> {
    authorized(webview_window.label(), || state.inner().close(&app)).map_err(record_error)
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

    /// Neither display-only window may open, close, or restyle a window itself,
    /// so both labels are rejected alongside every other non-main one.
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
