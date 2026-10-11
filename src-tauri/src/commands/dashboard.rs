use tauri::{State, WebviewWindow};

use crate::{
    domain::{AppError, CommandError, DashboardMetrics, DashboardMetricsQuery},
    logging,
    metrics::DashboardService,
    security::authorize_main_window,
};

/// The Home dashboard's local metrics.
///
/// Read-only and exact-`main`: it reads the workspace indexes and the local
/// usage ledger, writes nothing, and reaches no network.
#[tauri::command]
pub(crate) async fn get_dashboard_metrics<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, DashboardService>,
    query: DashboardMetricsQuery,
) -> Result<DashboardMetrics, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    tauri::async_runtime::spawn_blocking(move || service.get_metrics(&query))
        .await
        .map_err(|_| record_error(AppError::dashboard_error("dashboard_worker_failed")))?
        .map_err(record_error)
}

fn authorized<T>(label: &str, operation: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_main_window(label)?;
    Ok(operation())
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn the_dashboard_authorizes_before_service_access() {
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
