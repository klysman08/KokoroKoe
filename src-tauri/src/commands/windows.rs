use tauri::{AppHandle, State, WebviewWindow};

use crate::{
    domain::{
        AppError, CommandError, DetachedWindow, DetachedWindowAppearance,
        DetachedWindowInteraction, DetachedWindowShortcutStatus, DetachedWindowView,
        SetDetachedWindowAppearanceRequest, SetDetachedWindowInteractionRequest,
        SetDetachedWindowShortcutRequest,
    },
    logging,
    security::authorize_main_window,
    windows::DetachedWindowService,
};

/// Names one of the windows Rust owns.
///
/// The frontend chooses a variant, never a label, URL, or size, so an unknown
/// value fails to deserialize before the command body runs.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DetachedWindowRequest {
    pub(crate) window: DetachedWindow,
}

#[tauri::command]
pub(crate) async fn open_detached_window<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, DetachedWindowService>,
    request: DetachedWindowRequest,
) -> Result<(), CommandError> {
    authorized(webview_window.label(), || {
        state.inner().open(&app, request.window)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn close_detached_window<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, DetachedWindowService>,
    request: DetachedWindowRequest,
) -> Result<(), CommandError> {
    authorized(webview_window.label(), || {
        state.inner().close(&app, request.window)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_detached_window_appearance<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DetachedWindowService>,
    request: DetachedWindowRequest,
) -> Result<DetachedWindowView<DetachedWindowAppearance>, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().appearance(request.window)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn set_detached_window_appearance<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, DetachedWindowService>,
    request: SetDetachedWindowAppearanceRequest,
) -> Result<DetachedWindowView<DetachedWindowAppearance>, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().apply(&app, request)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_detached_window_shortcut<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DetachedWindowService>,
    request: DetachedWindowRequest,
) -> Result<DetachedWindowView<DetachedWindowShortcutStatus>, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().shortcut_status(request.window)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn set_detached_window_shortcut<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DetachedWindowService>,
    request: SetDetachedWindowShortcutRequest,
) -> Result<DetachedWindowView<DetachedWindowShortcutStatus>, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().set_shortcut(request)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_detached_window_interaction<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DetachedWindowService>,
    request: DetachedWindowRequest,
) -> Result<DetachedWindowView<DetachedWindowInteraction>, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().interaction(request.window)
    })
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn set_detached_window_interaction<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, DetachedWindowService>,
    request: SetDetachedWindowInteractionRequest,
) -> Result<DetachedWindowView<DetachedWindowInteraction>, CommandError> {
    authorized(webview_window.label(), || {
        state.inner().set_click_through(&app, request)
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

    use crate::domain::{AppError, DetachedWindow};

    use super::DetachedWindowRequest;

    /// No detached window may open, close, or restyle a window itself, so every
    /// detached label is rejected alongside every other non-main one.
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

    /// The window argument is a closed set, not a label. Anything outside it
    /// fails to deserialize, so no frontend value can name another webview.
    #[test]
    fn only_the_windows_rust_owns_can_be_named() {
        for window in DetachedWindow::ALL {
            let request: DetachedWindowRequest =
                serde_json::from_value(serde_json::json!({ "window": window.label() })).unwrap();
            assert_eq!(request.window, window);
        }
        for rejected in [
            serde_json::json!({ "window": "main" }),
            serde_json::json!({ "window": "Transcript" }),
            serde_json::json!({ "window": "index.html" }),
            serde_json::json!({ "window": "transcript", "url": "http://example.test" }),
            serde_json::json!({}),
        ] {
            assert!(serde_json::from_value::<DetachedWindowRequest>(rejected).is_err());
        }
    }
}
