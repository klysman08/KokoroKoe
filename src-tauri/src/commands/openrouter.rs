use tauri::{State, WebviewWindow};

use crate::{
    domain::{AppError, CommandError, CredentialValidation, OpenRouterModel, RequestId},
    llm::OpenRouterService,
    logging,
    security::authorize_main_window,
};

#[tauri::command]
pub(crate) async fn validate_openrouter_api_key<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, OpenRouterService>,
    request_id: RequestId,
) -> Result<CredentialValidation, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || {
        let _ = request_id;
        service.validate_credential()
    })
    .await
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn list_openrouter_models<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, OpenRouterService>,
    force_refresh: bool,
    request_id: RequestId,
) -> Result<Vec<OpenRouterModel>, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || {
        let _ = request_id;
        service.list_models(force_refresh)
    })
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
        .map_err(|_| AppError::openrouter_error("openrouter_worker_failed"))?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn provider_commands_authorize_before_service_access() {
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
