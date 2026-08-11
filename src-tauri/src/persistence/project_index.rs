use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::domain::Project;

use super::project_store::{ProjectDiscoveryReport, ProjectStore, ProjectStoreError};

const PROJECT_INDEX_MIGRATION: &str = include_str!("../../migrations/project_index_v1.sql");
const PROJECT_INDEX_DATABASE_NAME: &str = "project-index.sqlite3";
const PROJECT_INDEX_SCHEMA_VERSION: u32 = 1;
const MAX_PAGE_LIMIT: u16 = 100;
const CURSOR_PREFIX: &str = "p1:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectPageRequest {
    pub(crate) cursor: Option<String>,
    pub(crate) limit: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectPage {
    pub(crate) items: Vec<Project>,
    pub(crate) next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectIndexRebuildReport {
    pub(crate) generation: u64,
    pub(crate) indexed_projects: u32,
    pub(crate) discovery: ProjectDiscoveryReport,
    pub(crate) quarantined_corrupt_index: bool,
}

#[allow(dead_code)]
pub(crate) struct ProjectCatalog {
    store: ProjectStore,
    app_data_directory: PathBuf,
    database_path: PathBuf,
    operation_lock: Mutex<()>,
}

#[allow(dead_code)]
impl ProjectCatalog {
    pub(crate) fn open(
        workspace_path: &Path,
        app_data_directory: PathBuf,
    ) -> Result<Self, ProjectStoreError> {
        let store = ProjectStore::open(workspace_path)?;
        Ok(Self {
            database_path: app_data_directory.join(PROJECT_INDEX_DATABASE_NAME),
            app_data_directory,
            store,
            operation_lock: Mutex::new(()),
        })
    }

    pub(crate) fn rebuild_index(&self) -> Result<ProjectIndexRebuildReport, ProjectStoreError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| ProjectStoreError::new("project_index_unavailable"))?;
        self.rebuild_index_with_fault(None)
    }

    pub(crate) fn list_projects(
        &self,
        request: &ProjectPageRequest,
    ) -> Result<ProjectPage, ProjectStoreError> {
        let cursor = decode_cursor(request)?;
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| ProjectStoreError::new("project_index_unavailable"))?;
        match self.list_projects_locked(cursor, request.limit) {
            Err(error) if error.code == "project_index_corrupt" => {
                self.quarantine_corrupt_index()?;
                self.rebuild_index_with_fault(None)?;
                self.list_projects_locked(cursor, request.limit)
            }
            Err(error) if error.code == "project_index_invalid" => {
                self.rebuild_index_with_fault(None)?;
                self.list_projects_locked(cursor, request.limit)
            }
            result => result,
        }
    }

    fn list_projects_locked(
        &self,
        cursor: DecodedCursor,
        limit: u16,
    ) -> Result<ProjectPage, ProjectStoreError> {
        let connection = self.open_connection_without_recovery()?;
        if !index_has_projection(&connection)? {
            drop(connection);
            self.rebuild_index_with_fault(None)?;
        }
        let connection = self.open_connection_without_recovery()?;
        read_page(&connection, cursor, limit)
    }

    fn rebuild_index_with_fault(
        &self,
        fault: Option<RebuildFault>,
    ) -> Result<ProjectIndexRebuildReport, ProjectStoreError> {
        let discovery = self.store.discover_projects()?;
        let (mut connection, mut quarantined_corrupt_index) =
            match self.open_connection_without_recovery() {
                Ok(connection) => (connection, false),
                Err(error) if error.code == "project_index_corrupt" => {
                    self.quarantine_corrupt_index()?;
                    (self.open_connection_without_recovery()?, true)
                }
                Err(error) => return Err(error),
            };
        let generation = match rebuild_on_connection(&mut connection, &discovery, fault) {
            Err(error) if error.code == "project_index_corrupt" && !quarantined_corrupt_index => {
                drop(connection);
                self.quarantine_corrupt_index()?;
                quarantined_corrupt_index = true;
                let mut recovered = self.open_connection_without_recovery()?;
                rebuild_on_connection(&mut recovered, &discovery, fault)?
            }
            result => result?,
        };
        Ok(ProjectIndexRebuildReport {
            generation,
            indexed_projects: discovery.projects.len() as u32,
            discovery,
            quarantined_corrupt_index,
        })
    }

    fn open_connection_without_recovery(&self) -> Result<Connection, ProjectStoreError> {
        fs::create_dir_all(&self.app_data_directory)
            .map_err(|_| ProjectStoreError::new("project_index_unavailable"))?;
        let mut connection = Connection::open(&self.database_path)
            .map_err(|_| ProjectStoreError::new("project_index_unavailable"))?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|_| ProjectStoreError::new("project_index_unavailable"))?;
        let version = raw_schema_version(&connection).map_err(map_index_read_error)?;
        ensure_schema(&mut connection, version)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
            )
            .map_err(map_index_read_error)?;
        Ok(connection)
    }

    fn quarantine_corrupt_index(&self) -> Result<(), ProjectStoreError> {
        let suffix = format!(".corrupt-{}", uuid::Uuid::new_v4());
        for source in database_family_paths(&self.database_path) {
            if source.exists() {
                let mut destination = source.as_os_str().to_owned();
                destination.push(&suffix);
                fs::rename(&source, PathBuf::from(destination))
                    .map_err(|_| ProjectStoreError::new("project_index_unavailable"))?;
            }
        }
        tracing::warn!("physically corrupt rebuildable project index quarantined");
        Ok(())
    }
}

fn ensure_schema(
    connection: &mut Connection,
    initial_version: u32,
) -> Result<(), ProjectStoreError> {
    if initial_version > PROJECT_INDEX_SCHEMA_VERSION {
        return Err(ProjectStoreError::new("project_index_future_schema"));
    }
    if initial_version == PROJECT_INDEX_SCHEMA_VERSION {
        return Ok(());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_index_write_error)?;
    transaction
        .execute_batch(PROJECT_INDEX_MIGRATION)
        .and_then(|()| {
            transaction.pragma_update(None, "user_version", PROJECT_INDEX_SCHEMA_VERSION)
        })
        .map_err(map_index_write_error)?;
    transaction.commit().map_err(map_index_write_error)
}

fn raw_schema_version(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn index_has_projection(connection: &Connection) -> Result<bool, ProjectStoreError> {
    connection
        .query_row(
            "SELECT generation FROM project_index_meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map(|generation| generation.is_some_and(|generation| generation >= 1))
        .map_err(map_index_read_error)
}

fn rebuild_on_connection(
    connection: &mut Connection,
    discovery: &ProjectDiscoveryReport,
    fault: Option<RebuildFault>,
) -> Result<u64, ProjectStoreError> {
    let current_generation = connection
        .query_row(
            "SELECT generation FROM project_index_meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(map_index_read_error)?
        .unwrap_or(0);
    let generation = current_generation
        .checked_add(1)
        .ok_or_else(|| ProjectStoreError::new("project_index_generation_exhausted"))?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_index_write_error)?;
    transaction
        .execute("DELETE FROM project_index_entries", [])
        .map_err(map_index_write_error)?;
    inject_rebuild_fault(fault, RebuildFault::AfterDelete)?;

    for (rank, snapshot) in discovery.projects.iter().enumerate() {
        let project_json = serde_json::to_string(&snapshot.project)
            .map_err(|_| ProjectStoreError::new("project_index_invalid"))?;
        let project_id = serde_json::to_value(snapshot.project.id)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| ProjectStoreError::new("project_index_invalid"))?;
        transaction
            .execute(
                "INSERT INTO project_index_entries (
                    project_id, folder_name, sort_rank, project_json, snapshot_sha256
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    project_id,
                    snapshot.project.folder_name,
                    rank as u32,
                    project_json,
                    snapshot.fingerprint.as_bytes().as_slice(),
                ],
            )
            .map_err(map_index_write_error)?;
    }
    inject_rebuild_fault(fault, RebuildFault::AfterInsert)?;
    transaction
        .execute(
            "INSERT INTO project_index_meta (singleton, generation) VALUES (1, ?1)
             ON CONFLICT(singleton) DO UPDATE SET generation = excluded.generation",
            params![generation],
        )
        .map_err(map_index_write_error)?;
    transaction.commit().map_err(map_index_write_error)?;
    u64::try_from(generation).map_err(|_| ProjectStoreError::new("project_index_invalid"))
}

fn read_page(
    connection: &Connection,
    cursor: DecodedCursor,
    limit: u16,
) -> Result<ProjectPage, ProjectStoreError> {
    let generation = connection
        .query_row(
            "SELECT generation FROM project_index_meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_index_read_error)?;
    let generation =
        u64::try_from(generation).map_err(|_| ProjectStoreError::new("project_index_invalid"))?;
    if cursor
        .generation
        .is_some_and(|cursor_generation| cursor_generation != generation)
    {
        return Err(ProjectStoreError::new("project_page_stale"));
    }
    let fetch_limit = u32::from(limit) + 1;
    let mut statement = connection
        .prepare(
            "SELECT project_json FROM project_index_entries
             WHERE sort_rank >= ?1 ORDER BY sort_rank ASC LIMIT ?2",
        )
        .map_err(map_index_read_error)?;
    let rows = statement
        .query_map(params![cursor.offset, fetch_limit], |row| {
            row.get::<_, String>(0)
        })
        .map_err(map_index_read_error)?;
    let mut items = Vec::with_capacity(fetch_limit as usize);
    for row in rows {
        let json = row.map_err(map_index_read_error)?;
        let project = serde_json::from_str(&json)
            .map_err(|_| ProjectStoreError::new("project_index_invalid"))?;
        items.push(project);
    }
    let has_more = items.len() > usize::from(limit);
    if has_more {
        items.pop();
    }
    let next_cursor = has_more.then(|| {
        format!(
            "{CURSOR_PREFIX}{generation}:{}",
            cursor.offset + u32::from(limit)
        )
    });
    Ok(ProjectPage { items, next_cursor })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodedCursor {
    generation: Option<u64>,
    offset: u32,
}

fn decode_cursor(request: &ProjectPageRequest) -> Result<DecodedCursor, ProjectStoreError> {
    if request.limit == 0 || request.limit > MAX_PAGE_LIMIT {
        return Err(ProjectStoreError::new("project_page_invalid"));
    }
    let Some(cursor) = request.cursor.as_deref() else {
        return Ok(DecodedCursor {
            generation: None,
            offset: 0,
        });
    };
    let encoded = cursor
        .strip_prefix(CURSOR_PREFIX)
        .ok_or_else(|| ProjectStoreError::new("project_page_invalid"))?;
    let (generation, offset) = encoded
        .split_once(':')
        .ok_or_else(|| ProjectStoreError::new("project_page_invalid"))?;
    let generation = generation
        .parse::<u64>()
        .ok()
        .filter(|generation| *generation >= 1)
        .ok_or_else(|| ProjectStoreError::new("project_page_invalid"))?;
    let offset = offset
        .parse::<u32>()
        .map_err(|_| ProjectStoreError::new("project_page_invalid"))?;
    Ok(DecodedCursor {
        generation: Some(generation),
        offset,
    })
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

fn map_index_read_error(error: rusqlite::Error) -> ProjectStoreError {
    if is_physical_corruption(&error) {
        ProjectStoreError::new("project_index_corrupt")
    } else {
        ProjectStoreError::new("project_index_unavailable")
    }
}

fn map_index_write_error(error: rusqlite::Error) -> ProjectStoreError {
    map_index_read_error(error)
}

fn database_family_paths(database_path: &Path) -> [PathBuf; 3] {
    let mut wal = database_path.as_os_str().to_owned();
    wal.push("-wal");
    let mut shm = database_path.as_os_str().to_owned();
    shm.push("-shm");
    [database_path.to_path_buf(), wal.into(), shm.into()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebuildFault {
    AfterDelete,
    AfterInsert,
}

fn inject_rebuild_fault(
    configured: Option<RebuildFault>,
    current: RebuildFault,
) -> Result<(), ProjectStoreError> {
    if configured == Some(current) {
        Err(ProjectStoreError::new("project_index_fault_injected"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ProjectCatalog, ProjectPageRequest, RebuildFault};
    use crate::{domain::Project, persistence::ProjectStore};

    fn project(id: &str, name: &str, updated_at: &str) -> Project {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        let mut value = fixture["project"].clone();
        value["id"] = serde_json::json!(id);
        value["name"] = serde_json::json!(name);
        value["folderName"] = serde_json::json!(format!(
            "{}--{}",
            name.to_ascii_lowercase().replace(' ', "-"),
            id.replace('-', "")[..8].to_owned()
        ));
        value["updatedAt"] = serde_json::json!(updated_at);
        serde_json::from_value(value).unwrap()
    }

    fn create_projects(workspace: &std::path::Path, projects: &[Project]) {
        let store = ProjectStore::open(workspace).unwrap();
        for project in projects {
            store.create_project(project).unwrap();
        }
    }

    #[test]
    fn empty_projection_rebuilds_from_markdown_and_pages_in_stable_order() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        create_projects(
            workspace.path(),
            &[
                project(
                    "11111111-1111-4111-8111-111111111111",
                    "First Project",
                    "2026-08-11T10:00:00Z",
                ),
                project(
                    "22222222-2222-4222-8222-222222222222",
                    "Second Project",
                    "2026-08-11T12:00:00Z",
                ),
                project(
                    "33333333-3333-4333-8333-333333333333",
                    "Third Project",
                    "2026-08-11T11:00:00Z",
                ),
            ],
        );
        let catalog =
            ProjectCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();

        let first = catalog
            .list_projects(&ProjectPageRequest {
                cursor: None,
                limit: 2,
            })
            .unwrap();
        assert_eq!(
            first
                .items
                .iter()
                .map(|project| project.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Second Project", "Third Project"]
        );
        let first_cursor = first.next_cursor.clone();
        let second = catalog
            .list_projects(&ProjectPageRequest {
                cursor: first.next_cursor,
                limit: 2,
            })
            .unwrap();
        assert_eq!(second.items[0].name, "First Project");
        assert!(second.next_cursor.is_none());

        catalog.rebuild_index().unwrap();
        assert_eq!(
            catalog
                .list_projects(&ProjectPageRequest {
                    cursor: first_cursor,
                    limit: 2,
                })
                .unwrap_err()
                .code,
            "project_page_stale"
        );
    }

    #[test]
    fn corrupt_projection_is_quarantined_and_rebuilt_only_from_markdown() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        create_projects(
            workspace.path(),
            &[project(
                "44444444-4444-4444-8444-444444444444",
                "Recovered Project",
                "2026-08-11T12:00:00Z",
            )],
        );
        fs::write(app_data.path().join("project-index.sqlite3"), b"not sqlite").unwrap();
        let catalog =
            ProjectCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();

        let report = catalog.rebuild_index().unwrap();

        assert!(report.quarantined_corrupt_index);
        assert_eq!(report.indexed_projects, 1);
        assert!(fs::read_dir(app_data.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("project-index.sqlite3.corrupt-")
        }));
        let page = catalog
            .list_projects(&ProjectPageRequest {
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(page.items[0].name, "Recovered Project");

        let connection =
            rusqlite::Connection::open(app_data.path().join("project-index.sqlite3")).unwrap();
        connection
            .execute(
                "UPDATE project_index_entries SET project_json = 'invalid'",
                [],
            )
            .unwrap();
        drop(connection);
        let repaired = catalog
            .list_projects(&ProjectPageRequest {
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(repaired.items[0].name, "Recovered Project");
    }

    #[test]
    fn failed_rebuild_rolls_back_the_previous_projection() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let first = project(
            "55555555-5555-4555-8555-555555555555",
            "Stable Project",
            "2026-08-11T10:00:00Z",
        );
        create_projects(workspace.path(), std::slice::from_ref(&first));
        let catalog =
            ProjectCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        catalog.rebuild_index().unwrap();
        create_projects(
            workspace.path(),
            &[project(
                "66666666-6666-4666-8666-666666666666",
                "New Project",
                "2026-08-11T11:00:00Z",
            )],
        );

        let error = catalog
            .rebuild_index_with_fault(Some(RebuildFault::AfterDelete))
            .unwrap_err();
        assert_eq!(error.code, "project_index_fault_injected");
        let page = catalog
            .list_projects(&ProjectPageRequest {
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(page.items, vec![first]);

        let report = catalog.rebuild_index().unwrap();
        assert_eq!(report.generation, 2);
        assert_eq!(report.indexed_projects, 2);
    }

    #[test]
    fn invalid_page_requests_are_rejected_before_database_access() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let catalog =
            ProjectCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        for request in [
            ProjectPageRequest {
                cursor: None,
                limit: 0,
            },
            ProjectPageRequest {
                cursor: None,
                limit: 101,
            },
            ProjectPageRequest {
                cursor: Some("p1:not-a-number".to_owned()),
                limit: 10,
            },
            ProjectPageRequest {
                cursor: Some("p2:1:0".to_owned()),
                limit: 10,
            },
        ] {
            assert_eq!(
                catalog.list_projects(&request).unwrap_err().code,
                "project_page_invalid"
            );
        }
    }
}
