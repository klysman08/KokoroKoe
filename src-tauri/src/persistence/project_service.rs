use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::domain::{
    AppError, CreateProjectInput, PageRequest, Project, ProjectId, ProjectPage, UpdateProjectInput,
    now_rfc3339,
};

use super::{ProjectCatalog, SessionCatalog, SettingsService, session_service::session_is_running};

/// Sessions are scanned only for their state, so a wide page keeps it to one read.
const SESSION_SCAN_PAGE: u16 = 100;

#[derive(Clone)]
pub(crate) struct ProjectService {
    settings: SettingsService,
    app_data_directory: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

impl ProjectService {
    pub(crate) fn new(settings: SettingsService, app_data_directory: PathBuf) -> Self {
        Self {
            settings,
            app_data_directory,
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn list_projects(&self, request: PageRequest) -> Result<ProjectPage, AppError> {
        request.validate().map_err(AppError::project_error)?;
        self.with_catalog(|catalog| catalog.list_projects(&request))
    }

    pub(crate) fn get_project(&self, project_id: ProjectId) -> Result<Project, AppError> {
        self.with_catalog(|catalog| {
            catalog
                .read_project(project_id)
                .map(|snapshot| snapshot.project)
        })
    }

    pub(crate) fn create_project(&self, input: CreateProjectInput) -> Result<Project, AppError> {
        let project = Project::create(input, now_rfc3339()?).map_err(AppError::project_error)?;
        self.with_catalog(|catalog| catalog.create_project(&project))
    }

    pub(crate) fn update_project(
        &self,
        project_id: ProjectId,
        expected_revision: u64,
        input: UpdateProjectInput,
    ) -> Result<Project, AppError> {
        let updated_at = now_rfc3339()?;
        self.with_catalog(|catalog| {
            let current = catalog.read_project(project_id)?;
            if current.project.revision != expected_revision {
                return Err(super::ProjectStoreError::new("project_revision_conflict"));
            }
            let updated = current
                .project
                .apply_update(input, updated_at)
                .map_err(super::ProjectStoreError::new)?;
            catalog.update_project(&updated, expected_revision, current.fingerprint)
        })
    }

    /// Removes one project from the application, keeping its files.
    ///
    /// Deleting a project takes its sessions with it, so a run in progress
    /// refuses the whole thing rather than moving a folder a writer is still
    /// appending to. The check reads the sessions of this project only.
    pub(crate) fn delete_project(&self, project_id: ProjectId) -> Result<(), AppError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| AppError::project_worker_failed())?;
        self.settings.with_workspace_operation(|workspace| {
            let projects = ProjectCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::project_error(error.code))?;
            let sessions = SessionCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::project_error(error.code))?;
            projects
                .read_project(project_id)
                .map_err(|error| AppError::project_error(error.code))?;
            if project_has_running_session(&sessions, project_id)? {
                return Err(AppError::project_error("project_delete_while_running"));
            }
            projects
                .delete_project(project_id)
                .map_err(|error| AppError::project_error(error.code))
        })
    }

    fn with_catalog<T>(
        &self,
        operation: impl FnOnce(&ProjectCatalog) -> Result<T, super::ProjectStoreError>,
    ) -> Result<T, AppError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| AppError::project_worker_failed())?;
        self.settings.with_workspace_operation(|workspace| {
            let catalog = ProjectCatalog::open(workspace, self.app_data_directory.clone())
                .map_err(|error| AppError::project_error(error.code))?;
            operation(&catalog).map_err(|error| AppError::project_error(error.code))
        })
    }
}

/// Whether any session in this project may still have a writer.
///
/// Paged rather than read whole, because a long-lived project can hold many
/// sessions and only the states matter. A page that cannot be read is treated
/// as a refusal rather than an all-clear: not knowing is not the same as
/// knowing nothing is running.
fn project_has_running_session(
    sessions: &SessionCatalog,
    project_id: ProjectId,
) -> Result<bool, AppError> {
    let mut cursor = None;
    loop {
        let page = sessions
            .list_project_sessions(
                project_id,
                &PageRequest {
                    cursor,
                    limit: SESSION_SCAN_PAGE,
                },
            )
            .map_err(|error| AppError::project_error(error.code))?;
        if page
            .items
            .iter()
            .any(|session| session_is_running(session.state))
        {
            return Ok(true);
        }
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        domain::{CreateProjectInput, PageRequest, UpdateProjectInput},
        persistence::SettingsService,
    };

    use super::ProjectService;

    fn input() -> CreateProjectInput {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-management-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["createInput"].clone()).unwrap()
    }

    fn service() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        SettingsService,
        ProjectService,
    ) {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let projects = ProjectService::new(settings.clone(), app_data.path().to_path_buf());
        (app_data, documents, settings, projects)
    }

    #[test]
    fn project_commands_persist_list_read_update_and_survive_restart() {
        let (app_data, _documents, settings, projects) = service();
        let created = projects.create_project(input()).unwrap();
        assert_eq!(created.revision, 1);

        let page = projects
            .list_projects(PageRequest {
                cursor: None,
                limit: 24,
            })
            .unwrap();
        assert_eq!(page.items, vec![created.clone()]);
        assert!(page.next_cursor.is_none());
        assert_eq!(projects.get_project(created.id).unwrap(), created);

        let update: UpdateProjectInput = serde_json::from_value(serde_json::json!({
            "name": "Customer discovery review",
            "tags": ["research", "review"]
        }))
        .unwrap();
        let updated = projects
            .update_project(created.id, created.revision, update)
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.folder_name, created.folder_name);
        assert_eq!(updated.name, "Customer discovery review");

        let workspace = settings.get_settings().unwrap();
        let document = Path::new(workspace.workspace_path())
            .join("projects")
            .join(&created.folder_name)
            .join("project.md");
        assert!(document.is_file());
        assert!(app_data.path().join("project-index.sqlite3").is_file());

        let restarted = ProjectService::new(settings, app_data.path().to_path_buf());
        assert_eq!(restarted.get_project(created.id).unwrap(), updated);
        assert_eq!(
            restarted
                .list_projects(PageRequest {
                    cursor: None,
                    limit: 24,
                })
                .unwrap()
                .items,
            vec![updated]
        );
    }

    #[test]
    fn stale_revision_never_overwrites_the_acknowledged_project() {
        let (_app_data, _documents, _settings, projects) = service();
        let created = projects.create_project(input()).unwrap();
        let first: UpdateProjectInput =
            serde_json::from_value(serde_json::json!({"description": "First update"})).unwrap();
        let saved = projects
            .update_project(created.id, created.revision, first)
            .unwrap();
        let stale: UpdateProjectInput =
            serde_json::from_value(serde_json::json!({"description": "Stale update"})).unwrap();

        let error = projects
            .update_project(created.id, created.revision, stale)
            .unwrap_err();

        assert_eq!(error.code, "project_revision_conflict");
        assert_eq!(projects.get_project(created.id).unwrap(), saved);
    }

    #[test]
    fn workspace_change_rebuilds_the_shared_index_for_the_new_workspace() {
        let (_app_data, _documents, settings, projects) = service();
        let created = projects.create_project(input()).unwrap();
        assert_eq!(projects.get_project(created.id).unwrap(), created);
        let other_workspace = tempfile::tempdir().unwrap();
        settings.choose_workspace(other_workspace.path()).unwrap();

        let page = projects
            .list_projects(PageRequest {
                cursor: None,
                limit: 24,
            })
            .unwrap();

        assert!(page.items.is_empty());
        assert_eq!(
            projects.get_project(created.id).unwrap_err().code,
            "project_not_found"
        );
    }

    /// The decision this feature rests on: deleting removes a project from the
    /// application, not from the disk. Every file must still be there
    /// afterwards, somewhere the user can reach it.
    #[test]
    fn deleting_a_project_moves_it_to_the_trash_intact() {
        let (_app_data, _documents, settings, projects) = service();
        let created = projects.create_project(input()).unwrap();
        let workspace = settings.get_settings().unwrap();
        let root = Path::new(workspace.workspace_path()).to_path_buf();
        let document = root
            .join("projects")
            .join(&created.folder_name)
            .join("project.md");
        let before = std::fs::read_to_string(&document).unwrap();

        projects.delete_project(created.id).unwrap();

        // Gone from the application.
        assert_eq!(
            projects
                .list_projects(PageRequest {
                    cursor: None,
                    limit: 24,
                })
                .unwrap()
                .items,
            vec![]
        );
        assert!(!document.exists(), "it left projects/");

        // And still on disk, byte for byte.
        let trashed = std::fs::read_dir(root.join("trash").join("projects"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(trashed.len(), 1);
        assert!(
            trashed[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&created.folder_name)
        );
        assert_eq!(
            std::fs::read_to_string(trashed[0].join("project.md")).unwrap(),
            before
        );
    }

    /// Trash is a place, not a bin that overwrites itself. Deleting, dragging
    /// the folder back, and deleting again is the whole point of a recoverable
    /// removal — and it must not cost the first copy.
    #[test]
    fn restoring_from_the_trash_and_deleting_again_keeps_both_copies() {
        let (_app_data, _documents, settings, projects) = service();
        let root = Path::new(settings.get_settings().unwrap().workspace_path()).to_path_buf();
        let created = projects.create_project(input()).unwrap();
        let live = root.join("projects").join(&created.folder_name);

        projects.delete_project(created.id).unwrap();
        let first_trashed = std::fs::read_dir(root.join("trash").join("projects"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();

        // The user drags it back out of the trash. The index does not watch the
        // workspace, so it is rediscovered on the next rebuild rather than
        // immediately — creating anything forces one, which is the documented
        // limitation of restoring by hand.
        copy_tree(&first_trashed, &live);
        projects.create_project(input()).unwrap();
        assert_eq!(projects.get_project(created.id).unwrap().id, created.id);

        projects.delete_project(created.id).unwrap();

        assert!(first_trashed.exists(), "the first copy is untouched");
        assert_eq!(
            std::fs::read_dir(root.join("trash").join("projects"))
                .unwrap()
                .count(),
            2
        );
    }

    fn copy_tree(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}
