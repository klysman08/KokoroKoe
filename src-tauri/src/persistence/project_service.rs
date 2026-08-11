use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::domain::{
    AppError, CreateProjectInput, PageRequest, Project, ProjectId, ProjectPage, UpdateProjectInput,
    now_rfc3339,
};

use super::{ProjectCatalog, SettingsService};

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
}
