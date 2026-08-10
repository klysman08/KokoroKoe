use tauri::{Emitter, State, WebviewWindow};

use crate::{
    domain::{AppError, AppSettings, CommandError, ModelDownloadJob, ModelInstallation, RequestId},
    logging,
    models::ModelService,
    security::authorize_main_window,
};

const MODEL_DOWNLOAD_PROGRESS_EVENT: &str = "model-download-progress";

#[tauri::command]
pub(crate) async fn list_transcription_models<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ModelService>,
) -> Result<Vec<ModelInstallation>, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.list())
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn download_transcription_model<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ModelService>,
    model_id: String,
    request_id: RequestId,
) -> Result<ModelDownloadJob, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    let job = run_blocking({
        let service = service.clone();
        move || service.queue_download(&model_id, request_id)
    })
    .await
    .map_err(record_error)?;
    spawn_download(service, webview_window, job.request_id);
    Ok(job)
}

#[tauri::command]
pub(crate) async fn resume_model_download<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ModelService>,
    model_id: String,
    request_id: RequestId,
) -> Result<ModelDownloadJob, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    let job = run_blocking({
        let service = service.clone();
        move || service.queue_resume(&model_id, request_id)
    })
    .await
    .map_err(record_error)?;
    spawn_download(service, webview_window, job.request_id);
    Ok(job)
}

#[tauri::command]
pub(crate) async fn cancel_model_download<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ModelService>,
    request_id: RequestId,
) -> Result<ModelDownloadJob, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.cancel(request_id))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn delete_transcription_model<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ModelService>,
    model_id: String,
) -> Result<ModelInstallation, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.delete(&model_id))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn set_default_transcription_model<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ModelService>,
    model_id: String,
    expected_settings_revision: u64,
) -> Result<AppSettings, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.set_default(&model_id, expected_settings_revision))
        .await
        .map_err(record_error)
}

fn spawn_download<R: tauri::Runtime>(
    service: ModelService,
    window: WebviewWindow<R>,
    request_id: RequestId,
) {
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = service.run_download(request_id, |event| {
            if window.emit(MODEL_DOWNLOAD_PROGRESS_EVENT, event).is_err() {
                tracing::warn!("model progress event could not be delivered to the main window");
            }
        }) {
            logging::record_app_error(&error);
        }
    });
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
        .map_err(|_| AppError::model_operation_failed("The model worker stopped unexpectedly."))?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn every_model_command_uses_the_exact_main_window_authorizer() {
        let side_effects = AtomicUsize::new(0);
        for label in ["transcript", "insights", "unknown"] {
            let result = super::authorized(label, || side_effects.fetch_add(1, Ordering::SeqCst));
            assert_eq!(result.unwrap_err().code, "command_not_authorized");
        }
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);
        assert!(super::authorized("main", || ()).is_ok());
    }
}
