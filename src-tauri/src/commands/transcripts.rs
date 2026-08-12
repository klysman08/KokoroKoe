use tauri::{State, WebviewWindow};

use crate::{
    domain::{
        AppError, CommandError, ProjectId, SessionId, TranscriptPage, TranscriptPageRequest,
        TranscriptSearchPageView, TranscriptSearchQuery,
    },
    logging,
    persistence::TranscriptService,
    security::authorize_main_window,
};

#[tauri::command]
pub(crate) async fn get_transcript_page<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptService>,
    project_id: ProjectId,
    session_id: SessionId,
    cursor: Option<String>,
    limit: u16,
) -> Result<TranscriptPage, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || {
        service.get_transcript_page(TranscriptPageRequest {
            project_id,
            session_id,
            cursor,
            limit,
        })
    })
    .await
    .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn search_transcript<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, TranscriptService>,
    project_id: ProjectId,
    session_id: Option<SessionId>,
    query: String,
    cursor: Option<String>,
    limit: u16,
) -> Result<TranscriptSearchPageView, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || {
        service.search_transcript(TranscriptSearchQuery {
            project_id,
            session_id,
            query,
            cursor,
            limit,
        })
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
        .map_err(|_| AppError::transcript_worker_failed())?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn every_transcript_command_authorizes_before_service_access() {
        let side_effects = AtomicUsize::new(0);
        for label in ["transcript", "insights", "unknown"] {
            let result = super::authorized(label, || side_effects.fetch_add(1, Ordering::SeqCst));
            assert_eq!(result.unwrap_err().code, "command_not_authorized");
        }
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);
        assert!(super::authorized("main", || ()).is_ok());
    }
}
