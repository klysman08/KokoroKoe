use std::sync::Arc;

use tauri::{Emitter, State, WebviewWindow};

use crate::{
    domain::{AppError, CommandError, LiveTranscriptionInput, LiveTranscriptionStatus, RequestId},
    logging,
    security::authorize_main_window,
    transcription::{LiveTranscriptionService, ProductTranscriptionEvent},
};

const TRANSCRIPTION_PARTIAL_EVENT: &str = "transcription-partial";
const TRANSCRIPTION_FINAL_EVENT: &str = "transcription-final";
const TRANSCRIPTION_GAP_EVENT: &str = "transcription-gap";

#[tauri::command]
pub(crate) async fn start_live_transcription<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, LiveTranscriptionService>,
    input: LiveTranscriptionInput,
    request_id: RequestId,
) -> Result<LiveTranscriptionStatus, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    let emit_window = webview_window.clone();
    run_transcription_worker(move || {
        let emit = Arc::new(move |event| emit_product_event(&emit_window, event));
        service.start(input, request_id, emit)
    })
    .await
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_live_transcription_status<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, LiveTranscriptionService>,
) -> Result<LiveTranscriptionStatus, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_transcription_worker(move || service.status())
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn stop_live_transcription<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, LiveTranscriptionService>,
    request_id: RequestId,
) -> Result<LiveTranscriptionStatus, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_transcription_worker(move || service.stop(request_id))
        .await
        .map_err(record_error)
}

fn emit_product_event<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
    event: ProductTranscriptionEvent,
) {
    let result = match event {
        ProductTranscriptionEvent::Partial(event) => {
            window.emit(TRANSCRIPTION_PARTIAL_EVENT, event)
        }
        ProductTranscriptionEvent::Final(event) => window.emit(TRANSCRIPTION_FINAL_EVENT, event),
        ProductTranscriptionEvent::Gap(event) => window.emit(TRANSCRIPTION_GAP_EVENT, event),
    };
    if result.is_err() {
        tracing::warn!("live transcription event could not be delivered to the main window");
    }
}

fn authorized<T>(window_label: &str, operation: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_main_window(window_label)?;
    Ok(operation())
}

async fn run_transcription_worker<T>(
    operation: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError>
where
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| AppError::live_transcription_error("live_transcription_worker_failed"))?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use super::authorized;

    #[test]
    fn live_transcription_commands_authorize_before_accessing_state() {
        assert_eq!(authorized("main", || 7).unwrap(), 7);
        for label in ["transcript", "insights", "unknown"] {
            let error = authorized(label, || panic!("must not access state")).unwrap_err();
            assert_eq!(error.code, "command_not_authorized");
        }
    }
}
