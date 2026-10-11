mod dashboard;
mod disk;
mod store;

use std::sync::OnceLock;

use crate::domain::AppError;

pub(crate) use dashboard::DashboardService;
pub(crate) use store::{MetricsStore, UsageRecord};

/// The store every sanitized error is also written to.
///
/// Errors reach `logging::record_app_error` from every command module through a
/// free function, so the sink is process-wide rather than threaded through
/// each of them. It is installed once at startup; until then — and in unit
/// tests — recording an error only logs it.
static ERROR_SINK: OnceLock<MetricsStore> = OnceLock::new();

pub(crate) fn install_error_sink(store: MetricsStore) {
    let _ = ERROR_SINK.set(store);
}

pub(crate) fn record_error(error: &AppError) {
    if let Some(store) = ERROR_SINK.get()
        && store.record_error(error).is_err()
    {
        // Not routed back through `record_app_error`: a failing error log must
        // not recurse into itself.
        tracing::warn!("the error could not be added to the local error log");
    }
}
