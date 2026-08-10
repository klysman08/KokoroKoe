use std::path::PathBuf;

use tauri::{State, WebviewWindow};

use crate::{
    domain::{AppError, AppSettings, AppSettingsUpdate, CommandError, Versioned, WorkspaceStatus},
    logging,
    persistence::SettingsService,
    security::authorize_main_window,
};

#[tauri::command]
pub(crate) async fn get_settings<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, SettingsService>,
) -> Result<AppSettings, CommandError> {
    let service = authorize_before_side_effect(webview_window.label(), || state.inner().clone())
        .map_err(record_error)?;

    run_blocking(move || service.get_settings())
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn update_settings<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, SettingsService>,
    expected_revision: u64,
    value: AppSettingsUpdate,
) -> Result<AppSettings, CommandError> {
    let service = authorize_before_side_effect(webview_window.label(), || state.inner().clone())
        .map_err(record_error)?;
    if value.changes_default_transcription_model() {
        return Err(record_error(AppError::model_error(
            "model_settings_route_required",
        )));
    }
    let request = Versioned::new(expected_revision, value).map_err(record_error)?;

    run_blocking(move || service.update_settings(request.expected_revision, request.value))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn choose_workspace<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, SettingsService>,
) -> Result<WorkspaceStatus, CommandError> {
    let service = authorize_before_side_effect(webview_window.label(), || state.inner().clone())
        .map_err(record_error)?;
    let selection_guard = service.begin_workspace_selection().map_err(record_error)?;

    run_blocking(move || {
        let _selection_guard = selection_guard;
        let selected =
            pick_workspace_folder().ok_or_else(AppError::workspace_selection_cancelled)?;
        service.choose_workspace(&selected)
    })
    .await
    .map_err(record_error)
}

fn pick_workspace_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a KokoroKoe workspace folder")
        .pick_folder()
}

fn authorize_before_side_effect<T>(
    window_label: &str,
    operation: impl FnOnce() -> T,
) -> Result<T, AppError> {
    authorize_main_window(window_label)?;
    Ok(operation())
}

async fn run_blocking<T, F>(operation: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| AppError::settings_worker_failed())?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::security::authorize_main_window;

    #[test]
    fn every_settings_command_uses_the_shared_exact_window_authorizer() {
        assert!(authorize_main_window("main").is_ok());
        for label in ["transcript", "insights", "unknown"] {
            assert_eq!(
                authorize_main_window(label).unwrap_err().code,
                "command_not_authorized"
            );
        }
    }

    #[test]
    fn denied_authorization_runs_before_every_injected_side_effect() {
        let side_effects = AtomicUsize::new(0);
        for label in ["transcript", "insights", "unknown"] {
            let result = super::authorize_before_side_effect(label, || {
                side_effects.fetch_add(1, Ordering::SeqCst);
            });
            assert_eq!(result.unwrap_err().code, "command_not_authorized");
        }
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);
    }
}
