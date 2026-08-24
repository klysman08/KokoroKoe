use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::{
    domain::{
        AppError, CommandError, GenerateRecentInsightsRequest, RecentInsightsResponse,
        SessionInsightsPublication,
    },
    llm::InsightService,
    logging,
    security::authorize_main_window,
};

/// Carries a generated insight batch to the detached insights window, which
/// holds only event permissions and therefore cannot ask for one.
pub(crate) const SESSION_INSIGHTS_EVENT: &str = "session-insights";

#[tauri::command]
pub(crate) async fn generate_recent_insights<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: State<'_, InsightService>,
    request: GenerateRecentInsightsRequest,
) -> Result<RecentInsightsResponse, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    let response = run_blocking(move || service.generate_recent(request))
        .await
        .map_err(record_error)?;
    // A failed publish must not fail the generation: the main window already
    // has the result, and the detached window is an optional second view.
    if app
        .emit(
            SESSION_INSIGHTS_EVENT,
            SessionInsightsPublication::from_response(&response),
        )
        .is_err()
    {
        // The error itself is not logged: it can quote the payload, and that
        // payload is model output about the meeting.
        tracing::warn!("the generated insights could not be published");
    }
    Ok(response)
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
        .map_err(|_| AppError::insight_error("insight_worker_failed"))?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn recent_insights_authorize_before_service_access() {
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
