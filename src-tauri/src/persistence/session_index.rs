// Markdown session snapshots remain authoritative; this database is only a rebuildable list.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::domain::{PageRequest, Project, ProjectId, Session, SessionId, SessionPage};

use super::session_store::{
    SessionDiscoveryReport, SessionLocator, SessionSnapshot, SessionSnapshotFingerprint,
    SessionStore, SessionStoreError,
};

const MIGRATION: &str = include_str!("../../migrations/session_index_v1.sql");
const DATABASE_NAME: &str = "session-index.sqlite3";
const SCHEMA_VERSION: u32 = 1;
const MAX_PAGE_LIMIT: u16 = 100;
const CURSOR_PREFIX: &str = "s1:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionIndexRebuildReport {
    pub(crate) generation: u64,
    pub(crate) indexed_sessions: u32,
    pub(crate) discovery: SessionDiscoveryReport,
    pub(crate) quarantined_corrupt_index: bool,
}

pub(crate) struct SessionCatalog {
    store: SessionStore,
    app_data_directory: PathBuf,
    database_path: PathBuf,
    workspace_key: String,
    operation_lock: Mutex<()>,
}

impl SessionCatalog {
    pub(crate) fn open(
        workspace_path: &Path,
        app_data_directory: PathBuf,
    ) -> Result<Self, SessionStoreError> {
        let store = SessionStore::open(workspace_path)?;
        let canonical = fs::canonicalize(workspace_path)
            .map_err(|_| SessionStoreError::new("session_workspace_invalid"))?;
        let normalized = canonical.to_string_lossy().to_lowercase();
        let workspace_key = Sha256::digest(normalized.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            database_path: app_data_directory.join(DATABASE_NAME),
            app_data_directory,
            store,
            workspace_key,
            operation_lock: Mutex::new(()),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn rebuild_index(&self) -> Result<SessionIndexRebuildReport, SessionStoreError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        self.rebuild_index_with_fault(None)
    }

    #[allow(dead_code)]
    pub(crate) fn list_sessions(
        &self,
        request: &PageRequest,
    ) -> Result<SessionPage, SessionStoreError> {
        let cursor = decode_cursor(request)?;
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        match self.list_sessions_locked(cursor, request.limit) {
            Err(error) if error.code == "session_index_corrupt" => {
                self.quarantine_corrupt_index()?;
                self.rebuild_index_with_fault(None)?;
                self.list_sessions_locked(cursor, request.limit)
            }
            Err(error) if error.code == "session_index_invalid" => {
                self.rebuild_index_with_fault(None)?;
                self.list_sessions_locked(cursor, request.limit)
            }
            result => result,
        }
    }

    pub(crate) fn list_project_sessions(
        &self,
        project_id: ProjectId,
        request: &PageRequest,
    ) -> Result<SessionPage, SessionStoreError> {
        let project_id_text = id_text(project_id)?;
        let cursor = decode_project_cursor(request, &project_id_text)?;
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        match self.list_project_sessions_locked(&project_id_text, cursor, request.limit) {
            Err(error) if error.code == "session_index_corrupt" => {
                self.quarantine_corrupt_index()?;
                self.rebuild_index_with_fault(None)?;
                self.list_project_sessions_locked(&project_id_text, cursor, request.limit)
            }
            Err(error) if error.code == "session_index_invalid" => {
                self.rebuild_index_with_fault(None)?;
                self.list_project_sessions_locked(&project_id_text, cursor, request.limit)
            }
            result => result,
        }
    }

    pub(crate) fn read_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, SessionStoreError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        let snapshot = self.store.read_session_by_id(project_id, session_id)?;
        if snapshot.recovered_from_backup {
            self.rebuild_index_with_fault(None)?;
        }
        Ok(snapshot)
    }

    pub(crate) fn create_session(
        &self,
        project: &Project,
        session: &Session,
    ) -> Result<Session, SessionStoreError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        self.invalidate_projection_locked()?;
        self.store.create_session(project, session)?;
        self.refresh_after_write(session.id)?;
        Ok(session.clone())
    }

    #[cfg(test)]
    fn create_session_with_refresh_fault(
        &self,
        project: &Project,
        session: &Session,
        fault: RebuildFault,
    ) -> Result<Session, SessionStoreError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        self.invalidate_projection_locked()?;
        self.store.create_session(project, session)?;
        self.refresh_after_write_with_fault(session.id, Some(fault))?;
        Ok(session.clone())
    }

    pub(crate) fn update_session(
        &self,
        project: &Project,
        session: &Session,
        expected_revision: u64,
        expected_fingerprint: SessionSnapshotFingerprint,
    ) -> Result<Session, SessionStoreError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        self.invalidate_projection_locked()?;
        let locator = SessionLocator::from_records(project, session)?;
        let snapshot = self.store.update_session(
            &locator,
            session,
            expected_revision,
            expected_fingerprint,
        )?;
        self.refresh_after_write(snapshot.session.id)?;
        Ok(snapshot.session)
    }

    #[allow(dead_code)]
    fn list_sessions_locked(
        &self,
        cursor: DecodedCursor,
        limit: u16,
    ) -> Result<SessionPage, SessionStoreError> {
        let connection = self.open_connection_without_recovery()?;
        if !index_has_projection(&connection, &self.workspace_key)? {
            drop(connection);
            self.rebuild_index_with_fault(None)?;
        }
        let connection = self.open_connection_without_recovery()?;
        read_page(&connection, cursor, limit)
    }

    fn list_project_sessions_locked(
        &self,
        project_id: &str,
        cursor: DecodedCursor,
        limit: u16,
    ) -> Result<SessionPage, SessionStoreError> {
        let connection = self.open_connection_without_recovery()?;
        if !index_has_projection(&connection, &self.workspace_key)? {
            drop(connection);
            self.rebuild_index_with_fault(None)?;
        }
        let connection = self.open_connection_without_recovery()?;
        read_project_page(&connection, project_id, cursor, limit)
    }

    fn invalidate_projection_locked(&self) -> Result<(), SessionStoreError> {
        let mut connection = match self.open_connection_without_recovery() {
            Ok(connection) => connection,
            Err(error) if error.code == "session_index_corrupt" => {
                self.quarantine_corrupt_index()?;
                self.open_connection_without_recovery()?
            }
            Err(error) => return Err(error),
        };
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_write_error)?;
        transaction
            .execute("DELETE FROM session_index_meta", [])
            .map_err(map_write_error)?;
        transaction.commit().map_err(map_write_error)
    }

    fn refresh_after_write(&self, session_id: SessionId) -> Result<(), SessionStoreError> {
        self.refresh_after_write_with_fault(session_id, None)
    }

    fn refresh_after_write_with_fault(
        &self,
        session_id: SessionId,
        fault: Option<RebuildFault>,
    ) -> Result<(), SessionStoreError> {
        let report = self
            .rebuild_index_with_fault(fault)
            .map_err(|_| SessionStoreError::new("session_projection_refresh_pending"))?;
        if report
            .discovery
            .sessions
            .iter()
            .any(|candidate| candidate.snapshot.session.id == session_id)
        {
            Ok(())
        } else {
            Err(SessionStoreError::new("session_projection_refresh_pending"))
        }
    }

    fn rebuild_index_with_fault(
        &self,
        fault: Option<RebuildFault>,
    ) -> Result<SessionIndexRebuildReport, SessionStoreError> {
        let discovery = self.store.discover_sessions()?;
        let (mut connection, mut quarantined) = match self.open_connection_without_recovery() {
            Ok(connection) => (connection, false),
            Err(error) if error.code == "session_index_corrupt" => {
                self.quarantine_corrupt_index()?;
                (self.open_connection_without_recovery()?, true)
            }
            Err(error) => return Err(error),
        };
        let generation =
            match rebuild_on_connection(&mut connection, &discovery, &self.workspace_key, fault) {
                Err(error) if error.code == "session_index_corrupt" && !quarantined => {
                    drop(connection);
                    self.quarantine_corrupt_index()?;
                    quarantined = true;
                    let mut recovered = self.open_connection_without_recovery()?;
                    rebuild_on_connection(&mut recovered, &discovery, &self.workspace_key, fault)?
                }
                result => result?,
            };
        Ok(SessionIndexRebuildReport {
            generation,
            indexed_sessions: discovery.sessions.len() as u32,
            discovery,
            quarantined_corrupt_index: quarantined,
        })
    }

    fn open_connection_without_recovery(&self) -> Result<Connection, SessionStoreError> {
        fs::create_dir_all(&self.app_data_directory)
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        let mut connection = Connection::open(&self.database_path)
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
        let version = raw_schema_version(&connection).map_err(map_read_error)?;
        ensure_schema(&mut connection, version)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
            )
            .map_err(map_read_error)?;
        Ok(connection)
    }

    fn quarantine_corrupt_index(&self) -> Result<(), SessionStoreError> {
        let suffix = format!(".corrupt-{}", uuid::Uuid::new_v4());
        for source in database_family_paths(&self.database_path) {
            if source.exists() {
                let mut destination = source.as_os_str().to_owned();
                destination.push(&suffix);
                fs::rename(&source, PathBuf::from(destination))
                    .map_err(|_| SessionStoreError::new("session_index_unavailable"))?;
            }
        }
        tracing::warn!("physically corrupt rebuildable session index quarantined");
        Ok(())
    }
}

fn ensure_schema(connection: &mut Connection, version: u32) -> Result<(), SessionStoreError> {
    if version > SCHEMA_VERSION {
        return Err(SessionStoreError::new("session_index_future_schema"));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_write_error)?;
    transaction
        .execute_batch(MIGRATION)
        .and_then(|()| transaction.pragma_update(None, "user_version", SCHEMA_VERSION))
        .map_err(map_write_error)?;
    transaction.commit().map_err(map_write_error)
}

fn raw_schema_version(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn index_has_projection(
    connection: &Connection,
    workspace_key: &str,
) -> Result<bool, SessionStoreError> {
    connection
        .query_row(
            "SELECT generation, workspace_key FROM session_index_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map(|projection| {
            projection.is_some_and(|(generation, key)| generation >= 1 && key == workspace_key)
        })
        .map_err(map_read_error)
}

fn rebuild_on_connection(
    connection: &mut Connection,
    discovery: &SessionDiscoveryReport,
    workspace_key: &str,
    fault: Option<RebuildFault>,
) -> Result<u64, SessionStoreError> {
    let current = connection
        .query_row(
            "SELECT generation FROM session_index_meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(map_read_error)?
        .unwrap_or(0);
    let generation = current
        .checked_add(1)
        .ok_or_else(|| SessionStoreError::new("session_index_generation_exhausted"))?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_write_error)?;
    transaction
        .execute("DELETE FROM session_index_entries", [])
        .map_err(map_write_error)?;
    inject_fault(fault, RebuildFault::AfterDelete)?;
    for (rank, snapshot) in discovery.sessions.iter().enumerate() {
        let session_json = serde_json::to_string(&snapshot.snapshot.session)
            .map_err(|_| SessionStoreError::new("session_index_invalid"))?;
        let value = serde_json::to_value(&snapshot.snapshot.session)
            .map_err(|_| SessionStoreError::new("session_index_invalid"))?;
        let string = |name: &str| {
            value
                .get(name)
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .ok_or_else(|| SessionStoreError::new("session_index_invalid"))
        };
        transaction
            .execute(
                "INSERT INTO session_index_entries (
                    session_id, project_id, project_folder, session_folder, sort_rank,
                    session_json, snapshot_sha256
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    string("id")?,
                    string("projectId")?,
                    snapshot.project_folder,
                    snapshot.snapshot.session.folder_name,
                    rank as u32,
                    session_json,
                    snapshot.snapshot.fingerprint.as_bytes().as_slice(),
                ],
            )
            .map_err(map_write_error)?;
    }
    inject_fault(fault, RebuildFault::AfterInsert)?;
    transaction
        .execute(
            "INSERT INTO session_index_meta (singleton, generation, workspace_key) VALUES (1, ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET generation = excluded.generation,
                 workspace_key = excluded.workspace_key",
            params![generation, workspace_key],
        )
        .map_err(map_write_error)?;
    transaction.commit().map_err(map_write_error)?;
    u64::try_from(generation).map_err(|_| SessionStoreError::new("session_index_invalid"))
}

#[allow(dead_code)]
fn read_page(
    connection: &Connection,
    cursor: DecodedCursor,
    limit: u16,
) -> Result<SessionPage, SessionStoreError> {
    let generation = connection
        .query_row(
            "SELECT generation FROM session_index_meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_read_error)?;
    let generation =
        u64::try_from(generation).map_err(|_| SessionStoreError::new("session_index_invalid"))?;
    if cursor.generation.is_some_and(|value| value != generation) {
        return Err(SessionStoreError::new("session_page_stale"));
    }
    let mut statement = connection
        .prepare(
            "SELECT session_id, project_id, project_folder, session_folder, session_json,
                    snapshot_sha256 FROM session_index_entries
             WHERE sort_rank >= ?1 ORDER BY sort_rank ASC LIMIT ?2",
        )
        .map_err(map_read_error)?;
    let rows = statement
        .query_map(params![cursor.offset, u32::from(limit) + 1], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Vec<u8>>(5)?,
            ))
        })
        .map_err(map_read_error)?;
    let mut items = Vec::with_capacity(usize::from(limit) + 1);
    for row in rows {
        let (session_id, project_id, project_folder, session_folder, json, fingerprint) =
            row.map_err(map_read_error)?;
        let session: Session = serde_json::from_str(&json)
            .map_err(|_| SessionStoreError::new("session_index_invalid"))?;
        session
            .validate()
            .map_err(|_| SessionStoreError::new("session_index_invalid"))?;
        let value = serde_json::to_value(&session)
            .map_err(|_| SessionStoreError::new("session_index_invalid"))?;
        let stored_identity_matches = |name: &str, stored: &str| {
            value.get(name).and_then(serde_json::Value::as_str) == Some(stored)
        };
        if !stored_identity_matches("id", &session_id)
            || !stored_identity_matches("projectId", &project_id)
            || session.folder_name != session_folder
            || !valid_project_folder(&project_folder, &project_id)
            || fingerprint.len() != 32
        {
            return Err(SessionStoreError::new("session_index_invalid"));
        }
        items.push(session);
    }
    let has_more = items.len() > usize::from(limit);
    if has_more {
        items.pop();
    }
    Ok(SessionPage {
        next_cursor: has_more.then(|| {
            format!(
                "{CURSOR_PREFIX}{generation}:{}",
                cursor.offset + u32::from(limit)
            )
        }),
        items,
    })
}

fn read_project_page(
    connection: &Connection,
    project_id: &str,
    cursor: DecodedCursor,
    limit: u16,
) -> Result<SessionPage, SessionStoreError> {
    let generation = connection
        .query_row(
            "SELECT generation FROM session_index_meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_read_error)?;
    let generation =
        u64::try_from(generation).map_err(|_| SessionStoreError::new("session_index_invalid"))?;
    if cursor.generation.is_some_and(|value| value != generation) {
        return Err(SessionStoreError::new("session_page_stale"));
    }
    let mut statement = connection
        .prepare(
            "SELECT session_id, project_id, project_folder, session_folder, session_json,
                    snapshot_sha256 FROM session_index_entries
             WHERE project_id = ?1 ORDER BY sort_rank ASC LIMIT ?2 OFFSET ?3",
        )
        .map_err(map_read_error)?;
    let rows = statement
        .query_map(
            params![project_id, u32::from(limit) + 1, cursor.offset],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                ))
            },
        )
        .map_err(map_read_error)?;
    let mut items = Vec::with_capacity(usize::from(limit) + 1);
    for row in rows {
        let (session_id, stored_project_id, project_folder, session_folder, json, fingerprint) =
            row.map_err(map_read_error)?;
        let session: Session = serde_json::from_str(&json)
            .map_err(|_| SessionStoreError::new("session_index_invalid"))?;
        session
            .validate()
            .map_err(|_| SessionStoreError::new("session_index_invalid"))?;
        let value = serde_json::to_value(&session)
            .map_err(|_| SessionStoreError::new("session_index_invalid"))?;
        if value.get("id").and_then(serde_json::Value::as_str) != Some(&session_id)
            || value.get("projectId").and_then(serde_json::Value::as_str)
                != Some(stored_project_id.as_str())
            || stored_project_id != project_id
            || session.folder_name != session_folder
            || !valid_project_folder(&project_folder, &stored_project_id)
            || fingerprint.len() != 32
        {
            return Err(SessionStoreError::new("session_index_invalid"));
        }
        items.push(session);
    }
    let has_more = items.len() > usize::from(limit);
    if has_more {
        items.pop();
    }
    Ok(SessionPage {
        next_cursor: has_more.then(|| {
            format!(
                "{CURSOR_PREFIX}{generation}:{project_id}:{}",
                cursor.offset + u32::from(limit)
            )
        }),
        items,
    })
}

fn valid_project_folder(folder: &str, project_id: &str) -> bool {
    let compact_id = project_id.replace('-', "");
    compact_id.len() >= 8
        && !folder.is_empty()
        && folder.len() <= 128
        && folder
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && folder.ends_with(&format!("--{}", &compact_id[..8]))
}

#[derive(Debug, Clone, Copy)]
struct DecodedCursor {
    generation: Option<u64>,
    offset: u32,
}

#[allow(dead_code)]
fn decode_cursor(request: &PageRequest) -> Result<DecodedCursor, SessionStoreError> {
    if request.validate().is_err() || request.limit > MAX_PAGE_LIMIT {
        return Err(SessionStoreError::new("session_page_invalid"));
    }
    let Some(cursor) = request.cursor.as_deref() else {
        return Ok(DecodedCursor {
            generation: None,
            offset: 0,
        });
    };
    let encoded = cursor
        .strip_prefix(CURSOR_PREFIX)
        .ok_or_else(|| SessionStoreError::new("session_page_invalid"))?;
    let (generation, offset) = encoded
        .split_once(':')
        .ok_or_else(|| SessionStoreError::new("session_page_invalid"))?;
    Ok(DecodedCursor {
        generation: Some(
            generation
                .parse::<u64>()
                .ok()
                .filter(|value| *value >= 1)
                .ok_or_else(|| SessionStoreError::new("session_page_invalid"))?,
        ),
        offset: offset
            .parse::<u32>()
            .map_err(|_| SessionStoreError::new("session_page_invalid"))?,
    })
}

fn decode_project_cursor(
    request: &PageRequest,
    project_id: &str,
) -> Result<DecodedCursor, SessionStoreError> {
    if request.limit == 0 || request.limit > MAX_PAGE_LIMIT {
        return Err(SessionStoreError::new("session_page_invalid"));
    }
    let Some(cursor) = request.cursor.as_deref() else {
        return Ok(DecodedCursor {
            generation: None,
            offset: 0,
        });
    };
    let encoded = cursor
        .strip_prefix(CURSOR_PREFIX)
        .ok_or_else(|| SessionStoreError::new("session_page_invalid"))?;
    let mut parts = encoded.split(':');
    let generation = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 1)
        .ok_or_else(|| SessionStoreError::new("session_page_invalid"))?;
    let cursor_project_id = parts
        .next()
        .filter(|value| *value == project_id)
        .ok_or_else(|| SessionStoreError::new("session_page_invalid"))?;
    let _ = cursor_project_id;
    let offset = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| SessionStoreError::new("session_page_invalid"))?;
    if parts.next().is_some() {
        return Err(SessionStoreError::new("session_page_invalid"));
    }
    Ok(DecodedCursor {
        generation: Some(generation),
        offset,
    })
}

fn id_text<T: serde::Serialize>(id: T) -> Result<String, SessionStoreError> {
    serde_json::to_value(id)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| SessionStoreError::new("session_contract_invalid"))
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

fn map_read_error(error: rusqlite::Error) -> SessionStoreError {
    if is_physical_corruption(&error) {
        SessionStoreError::new("session_index_corrupt")
    } else {
        SessionStoreError::new("session_index_unavailable")
    }
}

fn map_write_error(error: rusqlite::Error) -> SessionStoreError {
    map_read_error(error)
}

fn database_family_paths(path: &Path) -> [PathBuf; 3] {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_owned();
    shm.push("-shm");
    [path.to_path_buf(), wal.into(), shm.into()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebuildFault {
    AfterDelete,
    AfterInsert,
}

fn inject_fault(
    configured: Option<RebuildFault>,
    current: RebuildFault,
) -> Result<(), SessionStoreError> {
    if configured == Some(current) {
        Err(SessionStoreError::new("session_index_fault_injected"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{RebuildFault, SessionCatalog};
    use crate::{
        domain::{PageRequest, Project, Session},
        persistence::{ProjectStore, SessionStore},
    };

    fn records() -> (Project, Session) {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        (
            serde_json::from_value(fixture["project"].clone()).unwrap(),
            serde_json::from_value(fixture["session"].clone()).unwrap(),
        )
    }

    fn session(id: &str, title: &str, started_at: &str) -> Session {
        let (_, base) = records();
        let mut value = serde_json::to_value(base).unwrap();
        value["id"] = serde_json::json!(id);
        value["title"] = serde_json::json!(title);
        value["folderName"] = serde_json::json!(format!(
            "2026-08-11-{}--{}",
            title.to_ascii_lowercase().replace(' ', "-"),
            &id.replace('-', "")[..8]
        ));
        value["startedAt"] = serde_json::json!(started_at);
        value["updatedAt"] = serde_json::json!(started_at);
        serde_json::from_value(value).unwrap()
    }

    fn create_sessions(workspace: &std::path::Path, sessions: &[Session]) {
        let (project, _) = records();
        let projects = ProjectStore::open(workspace).unwrap();
        if !workspace.join("projects").exists() {
            projects.create_project(&project).unwrap();
        }
        let store = SessionStore::open(workspace).unwrap();
        for session in sessions {
            store.create_session(&project, session).unwrap();
        }
    }

    #[test]
    fn empty_projection_rebuilds_and_pages_in_meeting_time_order() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        create_sessions(
            workspace.path(),
            &[
                session(
                    "11111111-1111-4111-8111-111111111111",
                    "First",
                    "2026-08-11T10:00:00Z",
                ),
                session(
                    "22222222-2222-4222-8222-222222222222",
                    "Second",
                    "2026-08-11T12:00:00Z",
                ),
                session(
                    "33333333-3333-4333-8333-333333333333",
                    "Third",
                    "2026-08-11T11:00:00Z",
                ),
            ],
        );
        let catalog =
            SessionCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();

        let first = catalog
            .list_sessions(&PageRequest {
                cursor: None,
                limit: 2,
            })
            .unwrap();
        assert_eq!(
            first
                .items
                .iter()
                .map(|session| session.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Second", "Third"]
        );
        let cursor = first.next_cursor.clone();
        let second = catalog
            .list_sessions(&PageRequest {
                cursor: first.next_cursor,
                limit: 2,
            })
            .unwrap();
        assert_eq!(second.items[0].title, "First");
        assert!(second.next_cursor.is_none());

        catalog.rebuild_index().unwrap();
        assert_eq!(
            catalog
                .list_sessions(&PageRequest { cursor, limit: 2 })
                .unwrap_err()
                .code,
            "session_page_stale"
        );
    }

    #[test]
    fn physical_and_semantic_corruption_rebuild_only_from_markdown() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        create_sessions(
            workspace.path(),
            &[session(
                "44444444-4444-4444-8444-444444444444",
                "Recovered",
                "2026-08-11T12:00:00Z",
            )],
        );
        fs::write(app_data.path().join("session-index.sqlite3"), b"not sqlite").unwrap();
        let catalog =
            SessionCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();

        let report = catalog.rebuild_index().unwrap();
        assert!(report.quarantined_corrupt_index);
        assert_eq!(report.indexed_sessions, 1);
        assert!(fs::read_dir(app_data.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("session-index.sqlite3.corrupt-")
        }));

        let database = app_data.path().join("session-index.sqlite3");
        let connection = rusqlite::Connection::open(database).unwrap();
        connection
            .execute(
                "UPDATE session_index_entries SET session_json = 'invalid'",
                [],
            )
            .unwrap();
        drop(connection);
        let repaired = catalog
            .list_sessions(&PageRequest {
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(repaired.items[0].title, "Recovered");
    }

    #[test]
    fn failed_rebuild_rolls_back_the_previous_generation() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let first = session(
            "55555555-5555-4555-8555-555555555555",
            "Stable",
            "2026-08-11T10:00:00Z",
        );
        create_sessions(workspace.path(), std::slice::from_ref(&first));
        let catalog =
            SessionCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        catalog.rebuild_index().unwrap();
        create_sessions(
            workspace.path(),
            &[session(
                "66666666-6666-4666-8666-666666666666",
                "New",
                "2026-08-11T11:00:00Z",
            )],
        );

        assert_eq!(
            catalog
                .rebuild_index_with_fault(Some(RebuildFault::AfterInsert))
                .unwrap_err()
                .code,
            "session_index_fault_injected"
        );
        assert_eq!(
            catalog
                .list_sessions(&PageRequest {
                    cursor: None,
                    limit: 10,
                })
                .unwrap()
                .items,
            vec![first]
        );
        assert_eq!(catalog.rebuild_index().unwrap().generation, 2);
    }

    #[test]
    fn durable_session_write_with_failed_refresh_repairs_on_the_next_list() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (project, _) = records();
        ProjectStore::open(workspace.path())
            .unwrap()
            .create_project(&project)
            .unwrap();
        let created = session(
            "99999999-9999-4999-8999-999999999999",
            "Durable",
            "2026-08-11T13:00:00Z",
        );
        let catalog =
            SessionCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();

        assert_eq!(
            catalog
                .create_session_with_refresh_fault(&project, &created, RebuildFault::AfterInsert)
                .unwrap_err()
                .code,
            "session_projection_refresh_pending"
        );
        assert_eq!(
            SessionStore::open(workspace.path())
                .unwrap()
                .read_session_by_id(project.id, created.id)
                .unwrap()
                .session,
            created
        );
        assert_eq!(
            catalog
                .list_project_sessions(
                    project.id,
                    &PageRequest {
                        cursor: None,
                        limit: 12,
                    },
                )
                .unwrap()
                .items,
            vec![created]
        );
    }

    #[test]
    fn a_shared_app_data_projection_rebinds_without_storing_workspace_paths() {
        let first_workspace = tempfile::tempdir().unwrap();
        let second_workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        create_sessions(
            first_workspace.path(),
            &[session(
                "77777777-7777-4777-8777-777777777777",
                "First Workspace",
                "2026-08-11T10:00:00Z",
            )],
        );
        create_sessions(
            second_workspace.path(),
            &[session(
                "88888888-8888-4888-8888-888888888888",
                "Second Workspace",
                "2026-08-11T11:00:00Z",
            )],
        );
        SessionCatalog::open(first_workspace.path(), app_data.path().to_path_buf())
            .unwrap()
            .rebuild_index()
            .unwrap();
        let second =
            SessionCatalog::open(second_workspace.path(), app_data.path().to_path_buf()).unwrap();
        let page = second
            .list_sessions(&PageRequest {
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(page.items[0].title, "Second Workspace");

        let connection =
            rusqlite::Connection::open(app_data.path().join("session-index.sqlite3")).unwrap();
        let key: String = connection
            .query_row(
                "SELECT workspace_key FROM session_index_meta WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(key.len(), 64);
        assert!(!key.contains(&second_workspace.path().display().to_string()));
    }

    #[test]
    fn invalid_pages_are_rejected_before_database_access() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let catalog =
            SessionCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        for request in [
            PageRequest {
                cursor: None,
                limit: 0,
            },
            PageRequest {
                cursor: None,
                limit: 101,
            },
            PageRequest {
                cursor: Some("s1:not-a-number".to_owned()),
                limit: 10,
            },
            PageRequest {
                cursor: Some("p1:1:0".to_owned()),
                limit: 10,
            },
        ] {
            assert_eq!(
                catalog.list_sessions(&request).unwrap_err().code,
                "session_page_invalid"
            );
        }
    }

    #[test]
    fn a_future_schema_fails_without_replacing_the_database() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let database = app_data.path().join("session-index.sqlite3");
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        drop(connection);
        let original = fs::read(&database).unwrap();
        let catalog =
            SessionCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();

        assert_eq!(
            catalog.rebuild_index().unwrap_err().code,
            "session_index_future_schema"
        );
        assert_eq!(fs::read(database).unwrap(), original);
    }
}
