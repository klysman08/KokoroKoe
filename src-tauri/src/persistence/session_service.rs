use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::domain::{
    AppError, CreateSessionSnapshotInput, PageRequest, ProjectId, Session, SessionId, SessionPage,
    UpdateSessionInput, now_rfc3339,
};

use super::{ProjectCatalog, SessionCatalog, SettingsService};

#[derive(Clone)]
pub(crate) struct SessionService {
    settings: SettingsService,
    app_data_directory: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

impl SessionService {
    pub(crate) fn new(settings: SettingsService, app_data_directory: PathBuf) -> Self {
        Self {
            settings,
            app_data_directory,
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn list_sessions(
        &self,
        project_id: ProjectId,
        request: PageRequest,
    ) -> Result<SessionPage, AppError> {
        self.with_catalogs(|projects, sessions| {
            projects
                .read_project(project_id)
                .map_err(|error| super::SessionStoreError::new(error.code))?;
            sessions.list_project_sessions(project_id, &request)
        })
    }

    pub(crate) fn get_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<Session, AppError> {
        self.with_catalogs(|projects, sessions| {
            projects
                .read_project(project_id)
                .map_err(|error| super::SessionStoreError::new(error.code))?;
            sessions
                .read_session(project_id, session_id)
                .map(|snapshot| snapshot.session)
        })
    }

    pub(crate) fn create_session(
        &self,
        project_id: ProjectId,
        input: CreateSessionSnapshotInput,
    ) -> Result<Session, AppError> {
        let created_at = now_rfc3339()?;
        self.with_catalogs(|projects, sessions| {
            let project = projects
                .read_project(project_id)
                .map_err(|error| super::SessionStoreError::new(error.code))?
                .project;
            let session = Session::create(project_id, input, created_at)
                .map_err(super::SessionStoreError::new)?;
            sessions.create_session(&project, &session)
        })
    }

    pub(crate) fn update_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected_revision: u64,
        input: UpdateSessionInput,
    ) -> Result<Session, AppError> {
        let updated_at = now_rfc3339()?;
        self.with_catalogs(|projects, sessions| {
            let project = projects
                .read_project(project_id)
                .map_err(|error| super::SessionStoreError::new(error.code))?
                .project;
            let current = sessions.read_session(project_id, session_id)?;
            if current.session.revision != expected_revision {
                return Err(super::SessionStoreError::new("session_revision_conflict"));
            }
            let updated = current
                .session
                .apply_update(input, updated_at)
                .map_err(super::SessionStoreError::new)?;
            sessions.update_session(&project, &updated, expected_revision, current.fingerprint)
        })
    }

    fn with_catalogs<T>(
        &self,
        operation: impl FnOnce(&ProjectCatalog, &SessionCatalog) -> Result<T, super::SessionStoreError>,
    ) -> Result<T, AppError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| AppError::session_worker_failed())?;
        self.settings.with_workspace_operation(|workspace| {
            let projects = ProjectCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::session_error(error.code))?;
            let sessions = SessionCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::session_error(error.code))?;
            operation(&projects, &sessions).map_err(|error| AppError::session_error(error.code))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{CreateProjectInput, CreateSessionSnapshotInput, PageRequest, UpdateSessionInput},
        persistence::{ProjectService, SettingsService},
    };

    use super::SessionService;

    fn project_input() -> CreateProjectInput {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-management-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["createInput"].clone()).unwrap()
    }

    fn session_input() -> CreateSessionSnapshotInput {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/session-management-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["createRequest"]["value"].clone()).unwrap()
    }

    #[test]
    fn session_commands_persist_project_scoped_list_read_update_and_restart() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let projects = ProjectService::new(settings.clone(), app_data.path().to_path_buf());
        let sessions = SessionService::new(settings.clone(), app_data.path().to_path_buf());
        let project = projects.create_project(project_input()).unwrap();

        let created = sessions
            .create_session(project.id, session_input())
            .unwrap();
        assert_eq!(created.revision, 1);
        assert_eq!(
            sessions
                .list_sessions(
                    project.id,
                    PageRequest {
                        cursor: None,
                        limit: 12,
                    },
                )
                .unwrap()
                .items,
            vec![created.clone()]
        );
        assert_eq!(
            sessions.get_session(project.id, created.id).unwrap(),
            created
        );

        let update: UpdateSessionInput = serde_json::from_value(serde_json::json!({
            "title": "Updated planning",
            "objective": "Confirm the final scope"
        }))
        .unwrap();
        let updated = sessions
            .update_session(project.id, created.id, created.revision, update)
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.folder_name, created.folder_name);
        assert_eq!(updated.title, "Updated planning");

        let restarted = SessionService::new(settings, app_data.path().to_path_buf());
        assert_eq!(
            restarted.get_session(project.id, created.id).unwrap(),
            updated
        );
    }

    #[test]
    fn stale_revisions_and_cross_project_reads_never_overwrite_or_leak_sessions() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let projects = ProjectService::new(settings.clone(), app_data.path().to_path_buf());
        let sessions = SessionService::new(settings, app_data.path().to_path_buf());
        let first_project = projects.create_project(project_input()).unwrap();
        let mut second_input = project_input();
        second_input.name = "Second project".to_owned();
        let second_project = projects.create_project(second_input).unwrap();
        let created = sessions
            .create_session(first_project.id, session_input())
            .unwrap();
        let update: UpdateSessionInput =
            serde_json::from_value(serde_json::json!({"title": "Acknowledged"})).unwrap();
        let saved = sessions
            .update_session(first_project.id, created.id, created.revision, update)
            .unwrap();
        let stale: UpdateSessionInput =
            serde_json::from_value(serde_json::json!({"title": "Stale"})).unwrap();
        assert_eq!(
            sessions
                .update_session(first_project.id, created.id, created.revision, stale)
                .unwrap_err()
                .code,
            "session_revision_conflict"
        );
        assert_eq!(
            sessions.get_session(first_project.id, created.id).unwrap(),
            saved
        );
        assert_eq!(
            sessions
                .get_session(second_project.id, created.id)
                .unwrap_err()
                .code,
            "session_not_found"
        );
        assert!(
            sessions
                .list_sessions(
                    second_project.id,
                    PageRequest {
                        cursor: None,
                        limit: 12,
                    },
                )
                .unwrap()
                .items
                .is_empty()
        );
    }
}
