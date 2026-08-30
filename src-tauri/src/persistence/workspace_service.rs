use std::path::PathBuf;

use crate::domain::{AppError, WorkspaceRescanReport};

use super::{ProjectCatalog, SessionCatalog, SettingsService, TranscriptSearchCatalog};

/// Rebuilds the derived indexes from the workspace on disk.
///
/// The architecture says Markdown is the source of truth and SQLite is a
/// rebuildable index. That is only true from the user's side if something can
/// actually rebuild it: until now nothing in the product did, so a folder
/// restored from the Trash — or a `project.md` edited in another editor — stayed
/// invisible until an unrelated write happened to trigger a refresh.
///
/// This is that missing action, and it is explicit rather than automatic. The
/// workspace is the user's own Documents folder; scanning it is something they
/// ask for, not something the application does behind them.
#[derive(Clone)]
pub(crate) struct WorkspaceService {
    settings: SettingsService,
    app_data_directory: PathBuf,
}

impl WorkspaceService {
    pub(crate) const fn new(settings: SettingsService, app_data_directory: PathBuf) -> Self {
        Self {
            settings,
            app_data_directory,
        }
    }

    pub(crate) fn rescan(&self) -> Result<WorkspaceRescanReport, AppError> {
        self.settings.with_workspace_operation(|workspace| {
            let projects = ProjectCatalog::open(workspace, self.app_data_directory.clone())
                .and_then(|catalog| catalog.rebuild_index())
                .map_err(|error| AppError::workspace_error(error.code))?;
            let sessions = SessionCatalog::open(workspace, self.app_data_directory.clone())
                .and_then(|catalog| catalog.rebuild_index())
                .map_err(|error| AppError::workspace_error(error.code))?;
            let transcripts =
                TranscriptSearchCatalog::open(workspace, self.app_data_directory.clone())
                    .and_then(|catalog| catalog.rebuild_index())
                    .map_err(|error| AppError::workspace_error(error.code))?;
            // Every folder the scan could not read, from all three passes. A
            // truncated issue list still means at least what it reported.
            let issues = projects.discovery.issues.len()
                + sessions.discovery.issues.len()
                + transcripts.discovery.issues.len();
            let report = WorkspaceRescanReport {
                schema_version: 1,
                projects: projects.indexed_projects,
                sessions: sessions.indexed_sessions,
                transcripts: transcripts.indexed_transcripts,
                segments: transcripts.indexed_segments,
                issues: u32::try_from(issues).unwrap_or(u32::MAX),
            };
            report.validate().map_err(AppError::workspace_error)?;
            Ok(report)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{CreateProjectInput, PageRequest},
        persistence::{ProjectService, SettingsService},
    };

    use super::WorkspaceService;

    fn project_input() -> CreateProjectInput {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-management-v1.json"
        ))
        .unwrap();
        serde_json::from_value(fixture["createInput"].clone()).unwrap()
    }

    /// The gap this exists to close: a folder restored from the Trash by hand
    /// stayed invisible, because nothing in the product rebuilt the index.
    /// Deleting promises the files are recoverable, and this is what makes
    /// putting one back actually work.
    #[test]
    fn a_project_restored_by_hand_reappears_after_a_rescan() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let projects = ProjectService::new(settings.clone(), app_data.path().to_path_buf());
        let workspace = WorkspaceService::new(settings.clone(), app_data.path().to_path_buf());
        let created = projects.create_project(project_input()).unwrap();
        let root =
            std::path::Path::new(settings.get_settings().unwrap().workspace_path()).to_path_buf();
        let live = root.join("projects").join(&created.folder_name);

        projects.delete_project(created.id).unwrap();
        assert_eq!(
            projects.get_project(created.id).unwrap_err().code,
            "project_not_found"
        );

        // The user drags the folder back out of the Trash in Explorer.
        let trashed = std::fs::read_dir(root.join("trash").join("projects"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::rename(&trashed, &live).unwrap();

        // Still invisible until asked: the application does not watch the disk.
        assert_eq!(
            projects.get_project(created.id).unwrap_err().code,
            "project_not_found"
        );

        let report = workspace.rescan().unwrap();

        assert_eq!(report.projects, 1);
        assert_eq!(report.issues, 0);
        assert_eq!(projects.get_project(created.id).unwrap().id, created.id);
        assert_eq!(
            projects
                .list_projects(PageRequest {
                    cursor: None,
                    limit: 24,
                })
                .unwrap()
                .items
                .len(),
            1
        );
    }

    /// A rescan reads the workspace and writes nothing to it. The Markdown is
    /// the user's, and rebuilding a cache is no reason to touch it.
    #[test]
    fn a_rescan_leaves_every_document_untouched() {
        let app_data = tempfile::tempdir().unwrap();
        let documents = tempfile::tempdir().unwrap();
        let settings = SettingsService::open(
            app_data.path().to_path_buf(),
            documents.path().to_path_buf(),
        )
        .unwrap();
        let projects = ProjectService::new(settings.clone(), app_data.path().to_path_buf());
        let workspace = WorkspaceService::new(settings.clone(), app_data.path().to_path_buf());
        let created = projects.create_project(project_input()).unwrap();
        let document = std::path::Path::new(settings.get_settings().unwrap().workspace_path())
            .join("projects")
            .join(&created.folder_name)
            .join("project.md");
        let before = std::fs::read(&document).unwrap();

        workspace.rescan().unwrap();
        workspace.rescan().unwrap();

        assert_eq!(std::fs::read(&document).unwrap(), before);
        assert_eq!(projects.get_project(created.id).unwrap(), created);
    }
}
