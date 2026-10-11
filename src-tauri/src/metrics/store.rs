// The local usage ledger and error log behind the dashboard.
//
// Unlike the project, session, and transcript indexes this database is not a
// projection of Markdown: nothing in the workspace records what a request cost
// or which operation failed, so these rows are the only copy. That is the
// architecture's division — SQLite owns detailed cost entries and sanitized
// errors — and it is why a corrupt file is quarantined beside the new one
// rather than deleted.
//
// It is a separate file from the settings database on purpose. The settings
// service holds its operation lock for the whole of a workspace operation, and
// a usage entry is written from inside an OpenRouter call; sharing that lock
// would make recording a cost able to wait on, or deadlock against, the very
// operation that incurred it.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use rusqlite::{Connection, TransactionBehavior, params};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    domain::{
        AppError, DASHBOARD_RECENT_ERROR_LIMIT, ErrorSeverity, LlmModelUsage, MetricsWindow,
        ProjectId, RecentErrorView, RequestId, SessionId, format_unix_ms,
    },
    prompts::PromptPurpose,
};

const MIGRATION: &str = include_str!("../../migrations/metrics_v1.sql");
const DATABASE_NAME: &str = "metrics.sqlite3";
const SCHEMA_VERSION: u32 = 1;
/// Errors are a recent-history view, not an audit log.
const MAX_STORED_ERRORS: i64 = 200;
const MAX_MODEL_ROWS: i64 = 1_000;
const PICO_PER_USD: u128 = 1_000_000_000_000;
const MAX_MODEL_ID_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetricsError {
    pub(crate) code: &'static str,
}

impl MetricsError {
    const fn new(code: &'static str) -> Self {
        Self { code }
    }
}

/// One reconciled OpenRouter request, as the budget ledger priced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageRecord<'a> {
    pub(crate) request_id: RequestId,
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) purpose: PromptPurpose,
    pub(crate) model_id: &'a str,
    pub(crate) input_tokens: u32,
    pub(crate) output_tokens: u32,
    pub(crate) actual_cost_usd: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct UsageSummary {
    pub(crate) requests: u32,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) actual_cost_usd: String,
    pub(crate) insights_generated: u64,
    /// Every model in the window, most requested first.
    pub(crate) models: Vec<LlmModelUsage>,
    pub(crate) recorded_since: String,
}

#[derive(Clone)]
pub(crate) struct MetricsStore {
    app_data_directory: PathBuf,
    database_path: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

impl MetricsStore {
    /// Names the database without touching the disk; it is created on first use.
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            database_path: app_data_directory.join(DATABASE_NAME),
            app_data_directory,
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn record_usage(&self, record: &UsageRecord<'_>) -> Result<(), MetricsError> {
        let cost = parse_picousd(record.actual_cost_usd)
            .ok_or_else(|| MetricsError::new("metrics_usage_invalid"))?;
        if record.model_id.trim().is_empty()
            || record.model_id.len() > MAX_MODEL_ID_BYTES
            || record.model_id.chars().any(char::is_control)
        {
            return Err(MetricsError::new("metrics_usage_invalid"));
        }
        let at_ms = now_unix_ms()?;
        self.with_connection(|connection| {
            // A request id is reconciled exactly once by the budget ledger, so a
            // repeat can only be a retry of this write; keeping the first row
            // means a cost is never counted twice.
            connection
                .execute(
                    "INSERT INTO usage_entries (request_id, project_id, session_id, purpose,
                       model_id, input_tokens, output_tokens, actual_cost_picousd, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(request_id) DO NOTHING",
                    params![
                        record.request_id.as_uuid().to_string(),
                        record.project_id.as_uuid().to_string(),
                        record.session_id.as_uuid().to_string(),
                        purpose_text(record.purpose),
                        record.model_id,
                        record.input_tokens,
                        record.output_tokens,
                        cost,
                        at_ms
                    ],
                )
                .map(|_| ())
                .map_err(map_write_error)
        })
    }

    pub(crate) fn record_insight_batch(
        &self,
        request_id: RequestId,
        project_id: ProjectId,
        session_id: SessionId,
        insight_count: usize,
    ) -> Result<(), MetricsError> {
        let count =
            u8::try_from(insight_count).map_err(|_| MetricsError::new("metrics_usage_invalid"))?;
        let at_ms = now_unix_ms()?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO insight_batches (request_id, project_id, session_id,
                       insight_count, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(request_id) DO NOTHING",
                    params![
                        request_id.as_uuid().to_string(),
                        project_id.as_uuid().to_string(),
                        session_id.as_uuid().to_string(),
                        count,
                        at_ms
                    ],
                )
                .map(|_| ())
                .map_err(map_write_error)
        })
    }

    /// Keeps the sanitized envelope of a failed operation. Informational
    /// outcomes — a project that is no longer there — are not failures worth
    /// surfacing, so they are not kept.
    pub(crate) fn record_error(&self, error: &AppError) -> Result<(), MetricsError> {
        let severity = match error.severity {
            ErrorSeverity::Info => return Ok(()),
            ErrorSeverity::Warning => "warning",
            ErrorSeverity::Error => "error",
            ErrorSeverity::Critical => "critical",
        };
        let at_ms = now_unix_ms()?;
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(map_write_error)?;
            transaction
                .execute(
                    "INSERT INTO recent_errors (code, user_message, severity, retryable,
                       correlation_id, occurred_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        error.code,
                        error.user_message,
                        severity,
                        i64::from(error.retryable),
                        error.correlation_id.to_string(),
                        at_ms
                    ],
                )
                .map_err(map_write_error)?;
            transaction
                .execute(
                    "DELETE FROM recent_errors
                     WHERE row_id <= (SELECT MAX(row_id) FROM recent_errors) - ?1",
                    params![MAX_STORED_ERRORS],
                )
                .map_err(map_write_error)?;
            transaction.commit().map_err(map_write_error)
        })
    }

    pub(crate) fn usage_summary(
        &self,
        window: &MetricsWindow,
    ) -> Result<UsageSummary, MetricsError> {
        let (from, to, project) = window_params(window);
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT model_id, COUNT(*), SUM(input_tokens), SUM(output_tokens),
                            SUM(actual_cost_picousd)
                     FROM usage_entries
                     WHERE (?1 IS NULL OR created_at_ms >= ?1)
                       AND (?2 IS NULL OR created_at_ms < ?2)
                       AND (?3 IS NULL OR project_id = ?3)
                     GROUP BY model_id
                     ORDER BY COUNT(*) DESC, SUM(actual_cost_picousd) DESC, model_id ASC
                     LIMIT ?4",
                )
                .map_err(map_read_error)?;
            let rows = statement
                .query_map(params![from, to, project, MAX_MODEL_ROWS], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })
                .map_err(map_read_error)?;
            let mut summary = UsageSummary::default();
            let mut total_cost: u128 = 0;
            for row in rows {
                let (model_id, requests, input, output, cost) = row.map_err(map_read_error)?;
                let requests = to_u32(requests)?;
                let input = to_u64(input)?;
                let output = to_u64(output)?;
                let cost = u128::from(to_u64(cost)?);
                summary.requests = summary
                    .requests
                    .checked_add(requests)
                    .ok_or_else(|| MetricsError::new("metrics_invalid"))?;
                summary.input_tokens = summary.input_tokens.saturating_add(input);
                summary.output_tokens = summary.output_tokens.saturating_add(output);
                total_cost = total_cost.saturating_add(cost);
                summary.models.push(LlmModelUsage {
                    model_id,
                    requests,
                    input_tokens: input,
                    output_tokens: output,
                    actual_cost_usd: format_picousd(cost),
                });
            }
            drop(statement);
            summary.actual_cost_usd = format_picousd(total_cost);
            let insights: i64 = connection
                .query_row(
                    "SELECT COALESCE(SUM(insight_count), 0) FROM insight_batches
                     WHERE (?1 IS NULL OR created_at_ms >= ?1)
                       AND (?2 IS NULL OR created_at_ms < ?2)
                       AND (?3 IS NULL OR project_id = ?3)",
                    params![from, to, project],
                    |row| row.get(0),
                )
                .map_err(map_read_error)?;
            summary.insights_generated = to_u64(insights)?;
            let started: i64 = connection
                .query_row(
                    "SELECT started_at_ms FROM metrics_meta WHERE singleton = 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(map_read_error)?;
            summary.recorded_since =
                format_unix_ms(started).ok_or_else(|| MetricsError::new("metrics_invalid"))?;
            Ok(summary)
        })
    }

    /// The most recent failures inside the window, newest first.
    ///
    /// Errors are not scoped to a project: most of them — a missing API key, an
    /// unreadable workspace — belong to no project at all.
    pub(crate) fn recent_errors(
        &self,
        window: &MetricsWindow,
    ) -> Result<Vec<RecentErrorView>, MetricsError> {
        let (from, to, _) = window_params(window);
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT code, user_message, severity, retryable, correlation_id, occurred_at_ms
                     FROM recent_errors
                     WHERE (?1 IS NULL OR occurred_at_ms >= ?1)
                       AND (?2 IS NULL OR occurred_at_ms < ?2)
                     ORDER BY row_id DESC
                     LIMIT ?3",
                )
                .map_err(map_read_error)?;
            let rows = statement
                .query_map(
                    params![from, to, DASHBOARD_RECENT_ERROR_LIMIT as i64],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    },
                )
                .map_err(map_read_error)?;
            let mut errors = Vec::new();
            for row in rows {
                let (code, user_message, severity, retryable, correlation_id, occurred_at) =
                    row.map_err(map_read_error)?;
                let severity = match severity.as_str() {
                    "warning" => ErrorSeverity::Warning,
                    "error" => ErrorSeverity::Error,
                    "critical" => ErrorSeverity::Critical,
                    _ => return Err(MetricsError::new("metrics_invalid")),
                };
                errors.push(RecentErrorView {
                    code,
                    user_message,
                    severity,
                    retryable: retryable != 0,
                    correlation_id: Uuid::parse_str(&correlation_id)
                        .map_err(|_| MetricsError::new("metrics_invalid"))?,
                    occurred_at: format_unix_ms(occurred_at)
                        .ok_or_else(|| MetricsError::new("metrics_invalid"))?,
                });
            }
            Ok(errors)
        })
    }

    fn with_connection<T>(
        &self,
        operation: impl Fn(&mut Connection) -> Result<T, MetricsError>,
    ) -> Result<T, MetricsError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| MetricsError::new("metrics_unavailable"))?;
        let mut connection = match self.open_connection() {
            Err(error) if error.code == "metrics_corrupt" => {
                self.quarantine_corrupt_database()?;
                self.open_connection()?
            }
            result => result?,
        };
        match operation(&mut connection) {
            Err(error) if error.code == "metrics_corrupt" => {
                drop(connection);
                self.quarantine_corrupt_database()?;
                let mut recovered = self.open_connection()?;
                operation(&mut recovered)
            }
            result => result,
        }
    }

    fn open_connection(&self) -> Result<Connection, MetricsError> {
        fs::create_dir_all(&self.app_data_directory)
            .map_err(|_| MetricsError::new("metrics_unavailable"))?;
        let mut connection = Connection::open(&self.database_path)
            .map_err(|_| MetricsError::new("metrics_unavailable"))?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|_| MetricsError::new("metrics_unavailable"))?;
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(map_read_error)?;
        ensure_schema(&mut connection, version)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
            )
            .map_err(map_read_error)?;
        Ok(connection)
    }

    fn quarantine_corrupt_database(&self) -> Result<(), MetricsError> {
        let suffix = format!(".corrupt-{}", Uuid::new_v4());
        for source in database_family_paths(&self.database_path) {
            if source.exists() {
                let mut destination = source.as_os_str().to_owned();
                destination.push(&suffix);
                fs::rename(&source, PathBuf::from(destination))
                    .map_err(|_| MetricsError::new("metrics_unavailable"))?;
            }
        }
        tracing::warn!("physically corrupt local metrics database quarantined");
        Ok(())
    }
}

fn ensure_schema(connection: &mut Connection, version: u32) -> Result<(), MetricsError> {
    if version > SCHEMA_VERSION {
        return Err(MetricsError::new("metrics_future_schema"));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    let started_at = now_unix_ms()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_write_error)?;
    transaction
        .execute_batch(MIGRATION)
        .and_then(|()| {
            transaction.execute(
                "INSERT INTO metrics_meta (singleton, started_at_ms) VALUES (1, ?1)",
                params![started_at],
            )
        })
        .and_then(|_| transaction.pragma_update(None, "user_version", SCHEMA_VERSION))
        .map_err(map_write_error)?;
    transaction.commit().map_err(map_write_error)
}

fn window_params(window: &MetricsWindow) -> (Option<i64>, Option<i64>, Option<String>) {
    (
        window.from_ms,
        window.to_ms,
        window
            .project_id
            .map(|project_id| project_id.as_uuid().to_string()),
    )
}

const fn purpose_text(purpose: PromptPurpose) -> &'static str {
    match purpose {
        PromptPurpose::Insight => "insight",
        PromptPurpose::Summary => "summary",
        PromptPurpose::ManualQuestion => "manual_question",
    }
}

fn now_unix_ms() -> Result<i64, MetricsError> {
    i64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| MetricsError::new("metrics_unavailable"))
}

/// Parses the budget ledger's canonical amount into whole picodollars.
pub(crate) fn parse_picousd(value: &str) -> Option<i64> {
    if !crate::domain::canonical_usd(value) {
        return None;
    }
    let (whole, fraction) = value.split_once('.')?;
    let whole: u128 = whole.parse().ok()?;
    let fraction: u128 = fraction.parse().ok()?;
    i64::try_from(whole.checked_mul(PICO_PER_USD)?.checked_add(fraction)?).ok()
}

/// The inverse of [`parse_picousd`], in the same canonical form.
pub(crate) fn format_picousd(value: u128) -> String {
    format!("{}.{:012}", value / PICO_PER_USD, value % PICO_PER_USD)
}

fn to_u64(value: i64) -> Result<u64, MetricsError> {
    u64::try_from(value).map_err(|_| MetricsError::new("metrics_invalid"))
}

fn to_u32(value: i64) -> Result<u32, MetricsError> {
    u32::try_from(value).map_err(|_| MetricsError::new("metrics_invalid"))
}

fn is_physical_corruption(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if matches!(
                sqlite_error.code,
                rusqlite::ffi::ErrorCode::DatabaseCorrupt | rusqlite::ffi::ErrorCode::NotADatabase
            )
    )
}

fn map_read_error(error: rusqlite::Error) -> MetricsError {
    if is_physical_corruption(&error) {
        MetricsError::new("metrics_corrupt")
    } else {
        MetricsError::new("metrics_unavailable")
    }
}

fn map_write_error(error: rusqlite::Error) -> MetricsError {
    if is_physical_corruption(&error) {
        MetricsError::new("metrics_corrupt")
    } else {
        MetricsError::new("metrics_write_failed")
    }
}

fn database_family_paths(path: &Path) -> [PathBuf; 3] {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_owned();
    shm.push("-shm");
    [path.to_path_buf(), PathBuf::from(wal), PathBuf::from(shm)]
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    fn project() -> ProjectId {
        serde_json::from_value(Value::String(Uuid::new_v4().to_string())).unwrap()
    }

    fn session() -> SessionId {
        serde_json::from_value(Value::String(Uuid::new_v4().to_string())).unwrap()
    }

    fn all_time() -> MetricsWindow {
        MetricsWindow {
            from_ms: None,
            to_ms: None,
            project_id: None,
        }
    }

    fn usage<'a>(
        request_id: RequestId,
        project_id: ProjectId,
        model_id: &'a str,
        cost: &'a str,
    ) -> UsageRecord<'a> {
        UsageRecord {
            request_id,
            project_id,
            session_id: session(),
            purpose: PromptPurpose::Insight,
            model_id,
            input_tokens: 1_000,
            output_tokens: 100,
            actual_cost_usd: cost,
        }
    }

    #[test]
    fn usage_totals_are_exact_and_ranked_by_requests() {
        let app_data = tempfile::tempdir().unwrap();
        let store = MetricsStore::new(app_data.path().to_path_buf());
        let first = project();
        let second = project();
        store
            .record_usage(&usage(RequestId::new(), first, "model/a", "0.000000000001"))
            .unwrap();
        store
            .record_usage(&usage(RequestId::new(), first, "model/b", "0.100000000000"))
            .unwrap();
        store
            .record_usage(&usage(
                RequestId::new(),
                second,
                "model/b",
                "0.200000000000",
            ))
            .unwrap();

        let summary = store.usage_summary(&all_time()).unwrap();
        assert_eq!(summary.requests, 3);
        assert_eq!(summary.input_tokens, 3_000);
        assert_eq!(summary.output_tokens, 300);
        // Picodollar arithmetic: a binary float would lose the one picodollar.
        assert_eq!(summary.actual_cost_usd, "0.300000000001");
        assert_eq!(summary.models[0].model_id, "model/b");
        assert_eq!(summary.models[0].requests, 2);
        assert_eq!(summary.models[0].actual_cost_usd, "0.300000000000");

        let scoped = store
            .usage_summary(&MetricsWindow {
                project_id: Some(second),
                ..all_time()
            })
            .unwrap();
        assert_eq!(scoped.requests, 1);
        assert_eq!(scoped.actual_cost_usd, "0.200000000000");
    }

    /// The budget ledger reconciles a request exactly once, so a second write
    /// for the same id is a retry and must not double the spend.
    #[test]
    fn a_repeated_request_is_counted_once() {
        let app_data = tempfile::tempdir().unwrap();
        let store = MetricsStore::new(app_data.path().to_path_buf());
        let request = RequestId::new();
        let entry = usage(request, project(), "model/a", "0.500000000000");
        store.record_usage(&entry).unwrap();
        store.record_usage(&entry).unwrap();
        assert_eq!(store.usage_summary(&all_time()).unwrap().requests, 1);
    }

    #[test]
    fn invalid_usage_is_refused_before_the_database() {
        let app_data = tempfile::tempdir().unwrap();
        let store = MetricsStore::new(app_data.path().to_path_buf());
        for (model, cost) in [
            ("model/a", "0.5"),
            ("", "0.500000000000"),
            ("a\nb", "0.500000000000"),
        ] {
            assert_eq!(
                store
                    .record_usage(&usage(RequestId::new(), project(), model, cost))
                    .unwrap_err()
                    .code,
                "metrics_usage_invalid"
            );
        }
        assert!(!app_data.path().join(DATABASE_NAME).exists());
    }

    #[test]
    fn a_window_excludes_what_falls_outside_it() {
        let app_data = tempfile::tempdir().unwrap();
        let store = MetricsStore::new(app_data.path().to_path_buf());
        store
            .record_usage(&usage(
                RequestId::new(),
                project(),
                "model/a",
                "0.100000000000",
            ))
            .unwrap();
        store
            .record_insight_batch(RequestId::new(), project(), session(), 3)
            .unwrap();
        let future = MetricsWindow {
            from_ms: Some(now_unix_ms().unwrap() + 60_000),
            ..all_time()
        };
        let summary = store.usage_summary(&future).unwrap();
        assert_eq!(summary.requests, 0);
        assert_eq!(summary.insights_generated, 0);
        assert_eq!(summary.actual_cost_usd, "0.000000000000");
        assert_eq!(
            store.usage_summary(&all_time()).unwrap().insights_generated,
            3
        );
    }

    /// The technical detail is the one field allowed to describe what went
    /// wrong, so it is the one field the error log never keeps.
    #[test]
    fn errors_keep_the_sanitized_envelope_and_drop_information() {
        let app_data = tempfile::tempdir().unwrap();
        let store = MetricsStore::new(app_data.path().to_path_buf());
        let failure =
            AppError::settings_unavailable("secret-canary C:\\Users\\Example\\meeting.md");
        store.record_error(&failure).unwrap();
        store
            .record_error(&AppError::project_error("project_not_found"))
            .unwrap();

        let errors = store.recent_errors(&all_time()).unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, failure.code);
        assert_eq!(errors[0].correlation_id, failure.correlation_id);
        let bytes = std::fs::read(app_data.path().join(DATABASE_NAME)).unwrap_or_default();
        let wal =
            std::fs::read(app_data.path().join(format!("{DATABASE_NAME}-wal"))).unwrap_or_default();
        for contents in [bytes, wal] {
            assert!(!String::from_utf8_lossy(&contents).contains("secret-canary"));
        }
    }

    #[test]
    fn the_error_log_keeps_only_the_newest_entries() {
        let app_data = tempfile::tempdir().unwrap();
        let store = MetricsStore::new(app_data.path().to_path_buf());
        for _ in 0..(MAX_STORED_ERRORS + 5) {
            store
                .record_error(&AppError::workspace_error("workspace_unavailable"))
                .unwrap();
        }
        let connection = Connection::open(app_data.path().join(DATABASE_NAME)).unwrap();
        let stored: i64 = connection
            .query_row("SELECT COUNT(*) FROM recent_errors", [], |row| row.get(0))
            .unwrap();
        assert_eq!(stored, MAX_STORED_ERRORS);
        assert_eq!(
            store.recent_errors(&all_time()).unwrap().len(),
            DASHBOARD_RECENT_ERROR_LIMIT
        );
    }

    /// These rows have no Markdown to be rebuilt from, so a damaged file is set
    /// aside rather than overwritten.
    #[test]
    fn a_corrupt_database_is_quarantined_and_replaced() {
        let app_data = tempfile::tempdir().unwrap();
        std::fs::write(app_data.path().join(DATABASE_NAME), b"not sqlite").unwrap();
        let store = MetricsStore::new(app_data.path().to_path_buf());
        assert_eq!(store.usage_summary(&all_time()).unwrap().requests, 0);
        assert!(std::fs::read_dir(app_data.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("metrics.sqlite3.corrupt-")
        }));
    }

    #[test]
    fn amounts_round_trip_exactly() {
        for value in ["0.000000000000", "0.000000000001", "1234567.999999999999"] {
            let pico = parse_picousd(value).unwrap();
            assert_eq!(format_picousd(u128::try_from(pico).unwrap()), value);
        }
        assert_eq!(parse_picousd("1.0"), None);
    }
}
