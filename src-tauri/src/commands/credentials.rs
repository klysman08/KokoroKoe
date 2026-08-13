use tauri::{State, WebviewWindow};

use crate::{
    domain::{AppError, CommandError, CredentialStatus, OpenRouterApiKey},
    logging,
    security::{CredentialService, authorize_main_window},
};

#[tauri::command]
pub(crate) async fn get_openrouter_credential_status<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, CredentialService>,
) -> Result<CredentialStatus, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.status())
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn set_openrouter_api_key<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, CredentialService>,
    api_key: OpenRouterApiKey,
) -> Result<CredentialStatus, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.set(&api_key))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn delete_openrouter_api_key<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, CredentialService>,
) -> Result<CredentialStatus, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.delete())
        .await
        .map_err(record_error)
}

fn authorized<T>(label: &str, operation: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_main_window(label)?;
    Ok(operation())
}

async fn run_blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| AppError::credential_error("credential_worker_failed"))?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn credential_commands_authorize_before_service_access() {
        let accesses = AtomicUsize::new(0);
        for label in ["transcript", "insights", "unknown"] {
            let error =
                super::authorized(label, || accesses.fetch_add(1, Ordering::SeqCst)).unwrap_err();
            assert_eq!(error.code, "command_not_authorized");
        }
        assert_eq!(accesses.load(Ordering::SeqCst), 0);
        assert!(super::authorized("main", || ()).is_ok());
    }
}
