use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::{
    domain::{AppError, AppSettings, AppSettingsUpdate, WorkspaceStatus},
    security::{prepare_foundation_workspace, probe_workspace, validate_workspace_path_syntax},
};

const FOUNDATION_MIGRATION: &str = include_str!("../../migrations/0001_foundation.sql");
const SETTINGS_DATABASE_NAME: &str = "kokorokoe.sqlite3";
const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone)]
pub(crate) struct SettingsService {
    app_data_directory: PathBuf,
    database_path: PathBuf,
    operation_lock: Arc<Mutex<()>>,
    foundation_defaults: AppSettings,
    workspace_selection_active: Arc<AtomicBool>,
}

pub(crate) struct WorkspaceSelectionGuard(Arc<AtomicBool>);

impl Drop for WorkspaceSelectionGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl SettingsService {
    pub(crate) fn open(
        app_data_directory: PathBuf,
        documents_directory: PathBuf,
    ) -> Result<Self, AppError> {
        let foundation_defaults = AppSettings::foundation_defaults(documents_directory)?;
        Ok(Self {
            database_path: app_data_directory.join(SETTINGS_DATABASE_NAME),
            app_data_directory,
            operation_lock: Arc::new(Mutex::new(())),
            foundation_defaults,
            workspace_selection_active: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(crate) fn get_settings(&self) -> Result<AppSettings, AppError> {
        let _operation = self.lock_operation()?;
        self.with_database_recovery(|connection| {
            get_settings_on_connection(connection, &self.foundation_defaults)
        })
    }

    pub(crate) fn update_settings(
        &self,
        expected_revision: u64,
        update: AppSettingsUpdate,
    ) -> Result<AppSettings, AppError> {
        let _operation = self.lock_operation()?;
        self.with_database_recovery(|connection| {
            update_settings_on_connection(
                connection,
                &self.foundation_defaults,
                expected_revision,
                update.clone(),
            )
        })
    }

    pub(crate) fn choose_workspace(&self, path: &Path) -> Result<WorkspaceStatus, AppError> {
        let status = probe_workspace(path)?;
        let _operation = self.lock_operation()?;
        self.with_database_recovery(|connection| {
            choose_workspace_on_connection(connection, &self.foundation_defaults, status.clone())
        })
    }

    pub(crate) fn begin_workspace_selection(&self) -> Result<WorkspaceSelectionGuard, AppError> {
        self.workspace_selection_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| AppError::workspace_selection_in_progress())?;
        Ok(WorkspaceSelectionGuard(Arc::clone(
            &self.workspace_selection_active,
        )))
    }

    fn lock_operation(&self) -> Result<std::sync::MutexGuard<'_, ()>, AppError> {
        self.operation_lock
            .lock()
            .map_err(|_| AppError::settings_save_failed())
    }

    fn open_connection(&self) -> Result<Connection, AppError> {
        self.open_connection_with_recovery(true)
    }

    fn with_database_recovery<T>(
        &self,
        mut operation: impl FnMut(&mut Connection) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut connection = match self.open_connection() {
            Ok(connection) => connection,
            Err(error) if error.code == "settings_database_corrupt" => {
                self.quarantine_corrupt_database()?;
                self.open_connection_with_recovery(false)?
            }
            Err(error) => return Err(error),
        };
        match operation(&mut connection) {
            Err(error) if error.code == "settings_database_corrupt" => {
                drop(connection);
                self.quarantine_corrupt_database()?;
                let mut recovered = self.open_connection_with_recovery(false)?;
                operation(&mut recovered)
            }
            result => result,
        }
    }

    fn open_connection_with_recovery(
        &self,
        allow_corruption_recovery: bool,
    ) -> Result<Connection, AppError> {
        fs::create_dir_all(&self.app_data_directory).map_err(|_| {
            AppError::settings_unavailable(
                "The local application settings directory could not be created.",
            )
        })?;
        let mut connection = Connection::open(&self.database_path).map_err(|_| {
            AppError::settings_unavailable("The local settings database could not be opened.")
        })?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|_| {
                AppError::settings_unavailable(
                    "The local settings database timeout could not be configured.",
                )
            })?;
        let initial_version = match raw_schema_version(&connection) {
            Ok(version) => version,
            Err(error) if allow_corruption_recovery && is_physical_corruption(&error) => {
                drop(connection);
                self.quarantine_corrupt_database()?;
                return self.open_connection_with_recovery(false);
            }
            Err(_) => {
                return Err(AppError::settings_unavailable(
                    "The local settings schema version could not be read.",
                ));
            }
        };
        ensure_schema(&mut connection, initial_version)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
            )
            .map_err(|error| {
                map_sqlite_read_error(
                    error,
                    "The local settings database could not be configured.",
                )
            })?;
        Ok(connection)
    }

    fn quarantine_corrupt_database(&self) -> Result<(), AppError> {
        let suffix = format!(".corrupt-{}", uuid::Uuid::new_v4());
        for source in database_family_paths(&self.database_path) {
            if source.exists() {
                let mut destination = source.as_os_str().to_owned();
                destination.push(&suffix);
                fs::rename(&source, PathBuf::from(destination)).map_err(|_| {
                    AppError::settings_unavailable(
                        "The corrupt settings database could not be quarantined.",
                    )
                })?;
            }
        }
        tracing::warn!("physically corrupt non-secret settings database quarantined");
        Ok(())
    }
}

fn ensure_schema(connection: &mut Connection, initial_version: u32) -> Result<(), AppError> {
    if initial_version > SETTINGS_SCHEMA_VERSION {
        return Err(AppError::settings_unavailable(
            "The local settings database was created by a newer application version.",
        ));
    }
    if initial_version == SETTINGS_SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| map_sqlite_read_error(error, "The local settings migration is busy."))?;
    match schema_version(&transaction)? {
        0 => {
            transaction
                .execute_batch(FOUNDATION_MIGRATION)
                .and_then(|()| {
                    transaction.pragma_update(None, "user_version", SETTINGS_SCHEMA_VERSION)
                })
                .map_err(|error| {
                    map_sqlite_read_error(
                        error,
                        "The local settings database migration did not complete.",
                    )
                })?;
        }
        SETTINGS_SCHEMA_VERSION => {}
        _ => {
            return Err(AppError::settings_unavailable(
                "The local settings database was created by a newer application version.",
            ));
        }
    }
    transaction.commit().map_err(|error| {
        map_sqlite_read_error(error, "The local settings migration did not commit.")
    })?;
    Ok(())
}

fn schema_version(connection: &Connection) -> Result<u32, AppError> {
    raw_schema_version(connection).map_err(|error| {
        map_sqlite_read_error(
            error,
            "The local settings schema version could not be read.",
        )
    })
}

fn raw_schema_version(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn is_physical_corruption(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if matches!(
                sqlite_error.code,
                rusqlite::ffi::ErrorCode::DatabaseCorrupt
                    | rusqlite::ffi::ErrorCode::NotADatabase
            )
    )
}

fn map_sqlite_read_error(error: rusqlite::Error, technical_detail: &str) -> AppError {
    if is_physical_corruption(&error) {
        AppError::settings_database_corrupt()
    } else {
        AppError::settings_unavailable(technical_detail)
    }
}

fn map_sqlite_write_error(error: rusqlite::Error) -> AppError {
    if is_physical_corruption(&error) {
        AppError::settings_database_corrupt()
    } else {
        AppError::settings_save_failed()
    }
}

fn get_settings_on_connection(
    connection: &mut Connection,
    defaults: &AppSettings,
) -> Result<AppSettings, AppError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_sqlite_write_error)?;
    let settings = load_or_recover(&transaction, defaults)?;
    transaction.commit().map_err(map_sqlite_write_error)?;
    Ok(settings)
}

fn update_settings_on_connection(
    connection: &mut Connection,
    defaults: &AppSettings,
    expected_revision: u64,
    update: AppSettingsUpdate,
) -> Result<AppSettings, AppError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_sqlite_write_error)?;
    let current = load_or_recover(&transaction, defaults)?;
    if current.revision() != expected_revision {
        return Err(AppError::settings_revision_conflict());
    }
    let updated = current.apply_update(update)?;
    store_settings(&transaction, &updated)?;
    transaction.commit().map_err(map_sqlite_write_error)?;
    Ok(updated)
}

fn choose_workspace_on_connection(
    connection: &mut Connection,
    defaults: &AppSettings,
    status: WorkspaceStatus,
) -> Result<WorkspaceStatus, AppError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_sqlite_write_error)?;
    let current = match load_or_recover(&transaction, defaults) {
        Ok(settings) => settings,
        Err(error)
            if matches!(
                error.code.as_str(),
                "workspace_invalid" | "workspace_unwritable"
            ) =>
        {
            let revision = last_allocated_revision(&transaction)?.unwrap_or(0);
            defaults.clone().with_revision(revision)?
        }
        Err(error) => return Err(error),
    };
    let updated = current.with_workspace_path(status.path.clone())?;
    store_settings(&transaction, &updated)?;
    transaction.commit().map_err(map_sqlite_write_error)?;
    Ok(status)
}

fn database_family_paths(database_path: &Path) -> [PathBuf; 3] {
    let sidecar = |suffix: &str| {
        let mut value = database_path.as_os_str().to_owned();
        value.push(suffix);
        PathBuf::from(value)
    };
    [
        sidecar("-wal"),
        sidecar("-shm"),
        database_path.to_path_buf(),
    ]
}

fn load_or_recover(
    connection: &Connection,
    defaults: &AppSettings,
) -> Result<AppSettings, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT revision, settings_json
             FROM app_settings_versions
             ORDER BY revision DESC
             LIMIT 3",
        )
        .map_err(|error| map_sqlite_read_error(error, "The settings record could not be read."))?;
    let stored_rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| map_sqlite_read_error(error, "The settings record could not be read."))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| map_sqlite_read_error(error, "The settings record could not be read."))?;
    drop(statement);
    let rows = stored_rows
        .into_iter()
        .map(|(revision, json)| {
            u64::try_from(revision)
                .map(|revision| (revision, json))
                .map_err(|_| {
                    AppError::settings_unavailable("The settings record revision is invalid.")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        let revision = last_allocated_revision(connection)?
            .map(next_revision)
            .transpose()?
            .unwrap_or(0);
        let workspace = prepare_foundation_workspace(Path::new(defaults.workspace_path()))?;
        let recovered = defaults
            .clone()
            .with_revision(revision)?
            .with_workspace_path_at_current_revision(workspace.path)?;
        store_settings(connection, &recovered)?;
        return Ok(recovered);
    }

    let newest_revision = rows[0].0;
    for (index, (stored_revision, json)) in rows.iter().enumerate() {
        let parsed = serde_json::from_str::<AppSettings>(json).ok();
        if let Some(settings) = parsed.filter(|settings| {
            settings.revision() == *stored_revision
                && validate_workspace_path_syntax(Path::new(settings.workspace_path())).is_ok()
        }) {
            if index == 0 {
                return Ok(settings);
            }

            let recovered = settings.with_revision(next_revision(newest_revision)?)?;
            store_settings(connection, &recovered)?;
            tracing::warn!("invalid non-secret settings record recovered from prior version");
            return Ok(recovered);
        }
    }

    let workspace = prepare_foundation_workspace(Path::new(defaults.workspace_path()))?;
    let recovered = defaults
        .clone()
        .with_revision(next_revision(newest_revision)?)?
        .with_workspace_path_at_current_revision(workspace.path)?;
    store_settings(connection, &recovered)?;
    tracing::warn!("invalid non-secret settings records reset to foundation defaults");
    Ok(recovered)
}

fn last_allocated_revision(connection: &Connection) -> Result<Option<u64>, AppError> {
    let value = connection
        .query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = 'app_settings_versions'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| {
            map_sqlite_read_error(error, "The settings revision could not be read.")
        })?;
    value
        .map(|revision| {
            u64::try_from(revision)
                .map_err(|_| AppError::settings_unavailable("The settings revision is invalid."))
        })
        .transpose()
}

fn store_settings(connection: &Connection, settings: &AppSettings) -> Result<(), AppError> {
    let json = serde_json::to_string(settings).map_err(|_| AppError::settings_save_failed())?;
    connection
        .execute(
            "INSERT INTO app_settings_versions (revision, settings_json) VALUES (?1, ?2)",
            params![settings.revision() as i64, json],
        )
        .map_err(map_sqlite_write_error)?;
    connection
        .execute(
            "DELETE FROM app_settings_versions
             WHERE revision NOT IN (
               SELECT revision FROM app_settings_versions ORDER BY revision DESC LIMIT 3
             )",
            [],
        )
        .map_err(map_sqlite_write_error)?;
    Ok(())
}

fn next_revision(revision: u64) -> Result<u64, AppError> {
    revision
        .checked_add(1)
        .filter(|value| *value <= 9_007_199_254_740_991)
        .ok_or_else(AppError::settings_revision_exhausted)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Seek, SeekFrom, Write},
        ops::Deref,
        sync::{Arc, Barrier},
    };

    use super::{SETTINGS_DATABASE_NAME, SettingsService, schema_version};
    use crate::domain::AppSettingsUpdate;
    use rusqlite::Connection;

    struct TestService {
        service: SettingsService,
        _app_data: tempfile::TempDir,
        _documents: tempfile::TempDir,
    }

    impl Deref for TestService {
        type Target = SettingsService;

        fn deref(&self) -> &Self::Target {
            &self.service
        }
    }

    fn service() -> TestService {
        let app_data = tempfile::tempdir().expect("app data should be available");
        let documents = tempfile::tempdir().expect("documents should be available");
        let service = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .expect("settings service should initialize lazily");
        TestService {
            service,
            _app_data: app_data,
            _documents: documents,
        }
    }

    fn update_fixture() -> AppSettingsUpdate {
        let versioned: crate::domain::Versioned<AppSettingsUpdate> = serde_json::from_str(
            include_str!("../../../fixtures/contracts/versioned-app-settings-update-v1.json"),
        )
        .expect("settings update fixture should be valid");
        versioned.value
    }

    #[test]
    fn updates_are_persistent_and_revision_checked() {
        let service = service();
        let updated = service.update_settings(0, update_fixture()).unwrap();
        assert_eq!(updated.revision(), 1);
        assert_eq!(service.get_settings().unwrap(), updated);

        let error = service.update_settings(0, update_fixture()).unwrap_err();
        assert_eq!(error.code, "settings_revision_conflict");
        assert!(!error.retryable);
        assert_eq!(service.get_settings().unwrap(), updated);
    }

    #[test]
    fn corrupted_settings_record_recovers_to_private_defaults() {
        let service = service();
        service.get_settings().unwrap();
        service
            .open_connection()
            .unwrap()
            .execute(
                "UPDATE app_settings_versions SET settings_json = ?1 WHERE revision = 0",
                [r#"{"unexpectedSecret":"secret-canary"}"#],
            )
            .unwrap();

        let recovered = service.get_settings().unwrap();
        assert_eq!(recovered.revision(), 1);
        assert!(
            !serde_json::to_string(&recovered)
                .unwrap()
                .contains("secret-canary")
        );
    }

    #[test]
    fn choosing_a_workspace_persists_the_canonical_path_and_revision() {
        let service = service();
        let workspace = tempfile::tempdir().expect("workspace should be available");
        let status = service.choose_workspace(workspace.path()).unwrap();
        let settings = service.get_settings().unwrap();
        assert!(status.writable);
        assert_eq!(settings.revision(), 1);
        assert_eq!(settings.workspace_path(), status.path);
    }

    #[test]
    fn file_backed_settings_survive_a_service_restart() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let service = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let updated = service.update_settings(0, update_fixture()).unwrap();
        drop(service);
        let reopened = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        assert_eq!(reopened.get_settings().unwrap(), updated);
    }

    #[test]
    fn a_corrupt_newest_version_recovers_from_the_previous_revision() {
        let service = service();
        service.update_settings(0, update_fixture()).unwrap();
        service
            .open_connection()
            .unwrap()
            .execute(
                "UPDATE app_settings_versions SET settings_json = ?1 WHERE revision = 1",
                [r#"{"defaultPresetId":"not-a-uuid"}"#],
            )
            .unwrap();
        let recovered = service.get_settings().unwrap();
        assert_eq!(recovered.revision(), 2);
        assert_eq!(recovered.default_session_budget_usd, "0.00");
    }

    #[test]
    fn independent_services_allow_exactly_one_expected_revision() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let first = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let second = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        first.get_settings().unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let workers = [first.clone(), second]
            .into_iter()
            .map(|service| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    service.update_settings(0, update_fixture())
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| result
                    .as_ref()
                    .is_err_and(|error| error.code == "settings_revision_conflict"))
                .count(),
            1
        );
        assert_eq!(first.get_settings().unwrap().revision(), 1);
    }

    #[test]
    fn concurrent_first_open_migrates_once() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let services = (0..2)
            .map(|_| {
                SettingsService::open(
                    app_data.path().to_path_buf(),
                    documents.path().to_path_buf(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let barrier = Arc::new(Barrier::new(3));
        let workers = services
            .into_iter()
            .map(|service| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    service.get_settings()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        for worker in workers {
            assert_eq!(worker.join().unwrap().unwrap().revision(), 0);
        }
    }

    #[test]
    fn empty_version_history_recovers_monotonically() {
        let service = service();
        service.update_settings(0, update_fixture()).unwrap();
        service.update_settings(1, update_fixture()).unwrap();
        service
            .open_connection()
            .unwrap()
            .execute("DELETE FROM app_settings_versions", [])
            .unwrap();
        let recovered = service.get_settings().unwrap();
        assert_eq!(recovered.revision(), 3);
        assert_eq!(
            service
                .update_settings(1, update_fixture())
                .unwrap_err()
                .code,
            "settings_revision_conflict"
        );
    }

    #[test]
    fn a_future_schema_version_fails_without_persistent_configuration() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let database_path = app_data.path().join(SETTINGS_DATABASE_NAME);
        let connection = Connection::open(&database_path).unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        let before_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        drop(connection);
        let service = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        assert_eq!(
            service.get_settings().unwrap_err().code,
            "settings_unavailable"
        );
        let connection = Connection::open(database_path).unwrap();
        let after_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(before_mode, after_mode);
        assert_eq!(schema_version(&connection).unwrap(), 2);
    }

    #[test]
    fn a_non_database_file_is_quarantined_and_rebuilt_without_exposing_raw_content() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let database_path = app_data.path().join(SETTINGS_DATABASE_NAME);
        let canary = b"secret-canary-not-a-database";
        fs::write(&database_path, canary).unwrap();
        let service = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let recovered = service.get_settings().expect("settings should rebuild");
        assert_eq!(recovered.revision(), 0);
        assert_ne!(fs::read(&database_path).unwrap(), canary);
        let quarantined = fs::read_dir(app_data.path())
            .unwrap()
            .filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
            .expect("corrupt database should be quarantined");
        assert_eq!(fs::read(quarantined.path()).unwrap(), canary);
    }

    #[test]
    fn a_corrupt_settings_table_page_is_quarantined_and_rebuilt() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let database_path = app_data.path().join(SETTINGS_DATABASE_NAME);
        let service = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        service.get_settings().unwrap();
        let connection = service.open_connection().unwrap();
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
        let page_size = u64::try_from(
            connection
                .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
                .unwrap(),
        )
        .unwrap();
        let root_page = u64::try_from(
            connection
                .query_row(
                    "SELECT rootpage FROM sqlite_master WHERE name = 'app_settings_versions'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
        )
        .unwrap();
        drop(connection);

        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&database_path)
            .unwrap();
        file.seek(SeekFrom::Start((root_page - 1) * page_size))
            .unwrap();
        file.write_all(&[0xFF; 128]).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let recovered = service
            .get_settings()
            .expect("table-page corruption should rebuild settings");
        assert_eq!(recovered.revision(), 0);
        assert!(
            fs::read_dir(app_data.path())
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
        );
    }
}
