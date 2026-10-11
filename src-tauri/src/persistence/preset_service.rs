use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    domain::{
        AppError, CreatePresetInput, CustomPreset, Preset, PresetId, ProjectId, UpdatePresetInput,
        now_rfc3339,
    },
    presets,
};

use super::{
    ProjectCatalog, SettingsService,
    preset_store::{PresetStore, PresetStoreError},
    project_store::ProjectLocator,
};

/// Reads the built-in catalog and the custom presets of one project.
///
/// The library the user chooses from is one list with two sources, and this is
/// where they meet: the built-ins come from the binary and never change, the
/// custom ones come from the project's own folder. Nothing here writes a
/// built-in — the catalog is a compile-time constant, and a request naming one
/// is refused before the workspace is opened.
#[derive(Clone)]
pub(crate) struct PresetService {
    settings: SettingsService,
    app_data_directory: PathBuf,
    operation_lock: Arc<Mutex<()>>,
}

impl PresetService {
    pub(crate) fn new(settings: SettingsService, app_data_directory: PathBuf) -> Self {
        Self {
            settings,
            app_data_directory,
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    /// The library, built-ins first.
    ///
    /// Without a project there is nothing but the built-ins, which is the honest
    /// answer for the project-creation form: no project exists yet to own a
    /// custom preset.
    pub(crate) fn list_presets(
        &self,
        project_id: Option<ProjectId>,
    ) -> Result<Vec<Preset>, AppError> {
        let mut library = presets::list();
        let Some(project_id) = project_id else {
            return Ok(library);
        };
        let custom =
            self.with_project(project_id, |store, locator| store.list(locator, project_id))?;
        library.extend(custom.into_iter().map(CustomPreset::into_preset));
        Ok(library)
    }

    pub(crate) fn create_preset(
        &self,
        project_id: ProjectId,
        input: CreatePresetInput,
    ) -> Result<Preset, AppError> {
        input.validate().map_err(AppError::preset_error)?;
        let preset = CustomPreset::create(input, project_id, now_rfc3339()?)
            .map_err(AppError::preset_error)?;
        self.with_project(project_id, |store, locator| {
            store.create(locator, &preset)?;
            Ok(preset.clone())
        })
        .map(CustomPreset::into_preset)
    }

    pub(crate) fn update_preset(
        &self,
        project_id: ProjectId,
        preset_id: PresetId,
        expected_revision: u64,
        input: UpdatePresetInput,
    ) -> Result<Preset, AppError> {
        refuse_built_in(&preset_id)?;
        let updated_at = now_rfc3339()?;
        self.with_project(project_id, |store, locator| {
            let current = store.read(locator, project_id, &preset_id)?;
            if current.preset.revision != expected_revision {
                return Err(PresetStoreError::new("preset_revision_conflict"));
            }
            let updated = current
                .preset
                .apply_update(input.clone(), updated_at.clone())
                .map_err(PresetStoreError::new)?;
            store
                .update(locator, &updated, expected_revision, current.fingerprint)
                .map(|record| record.preset)
        })
        .map(CustomPreset::into_preset)
    }

    /// Copies any preset in the library into this project as a new custom one.
    ///
    /// A built-in is an ordinary source here, and duplicating one is how a user
    /// starts from something that already works.
    pub(crate) fn duplicate_preset(
        &self,
        project_id: ProjectId,
        preset_id: PresetId,
        name: String,
    ) -> Result<Preset, AppError> {
        let created_at = now_rfc3339()?;
        let source = self
            .list_presets(Some(project_id))?
            .into_iter()
            .find(|preset| preset.id == preset_id)
            .ok_or_else(|| AppError::preset_error("preset_not_found"))?;
        let copy = CustomPreset::duplicate_from(&source, name, project_id, created_at)
            .map_err(AppError::preset_error)?;
        self.with_project(project_id, |store, locator| {
            store.create(locator, &copy)?;
            Ok(copy.clone())
        })
        .map(CustomPreset::into_preset)
    }

    pub(crate) fn delete_custom_preset(
        &self,
        project_id: ProjectId,
        preset_id: PresetId,
        expected_revision: u64,
    ) -> Result<(), AppError> {
        refuse_built_in(&preset_id)?;
        self.with_project(project_id, |store, locator| {
            store
                .trash(locator, project_id, &preset_id, expected_revision)
                .map(|_| ())
        })
    }

    /// The stored document, for the caller to write wherever the user chose.
    pub(crate) fn read_preset_document(
        &self,
        project_id: ProjectId,
        preset_id: PresetId,
    ) -> Result<(CustomPreset, Vec<u8>), AppError> {
        refuse_built_in(&preset_id)?;
        self.with_project(project_id, |store, locator| {
            store.read_document_bytes(locator, project_id, &preset_id)
        })
    }

    fn with_project<T>(
        &self,
        project_id: ProjectId,
        operation: impl FnOnce(&PresetStore, &ProjectLocator) -> Result<T, PresetStoreError>,
    ) -> Result<T, AppError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| AppError::preset_worker_failed())?;
        self.settings.with_workspace_operation(|workspace| {
            let locator = self.locate_project(workspace, project_id)?;
            let store =
                PresetStore::open(workspace).map_err(|error| AppError::preset_error(error.code))?;
            operation(&store, &locator).map_err(|error| AppError::preset_error(error.code))
        })
    }

    /// Resolves the project a preset belongs to, through the authoritative
    /// project record rather than anything the caller supplied.
    fn locate_project(
        &self,
        workspace: &Path,
        project_id: ProjectId,
    ) -> Result<ProjectLocator, AppError> {
        let projects = ProjectCatalog::open(workspace, self.app_data_directory.clone())
            .map_err(|error| AppError::preset_error(error.code))?;
        let project = projects
            .read_project(project_id)
            .map_err(|_| AppError::preset_error("preset_project_missing"))?;
        ProjectLocator::from_project(&project.project)
            .map_err(|_| AppError::preset_error("preset_project_missing"))
    }
}

/// A built-in preset is a compile-time constant, so nothing at runtime may
/// write to it. The refusal happens before the workspace is opened, because
/// there is nothing on disk this request could possibly be about.
fn refuse_built_in(preset_id: &PresetId) -> Result<(), AppError> {
    if presets::is_built_in(preset_id) {
        return Err(AppError::preset_error("preset_built_in_read_only"));
    }
    Ok(())
}
