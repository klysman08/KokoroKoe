use std::path::{Path, PathBuf};

use tauri::{State, WebviewWindow};

use crate::{
    domain::{
        AppError, CommandError, CreatePresetInput, Preset, PresetExport, PresetId, ProjectId,
        UpdatePresetInput,
    },
    logging,
    persistence::PresetService,
    presets,
    security::authorize_main_window,
};

/// Lists the preset library: the built-in catalog, and a project's own presets.
///
/// Without a project this reads no file and holds no state — the built-in
/// catalog is a compile-time constant. It stays an exact-`main` command in both
/// shapes: the detached windows have no business choosing what a Session
/// freezes.
#[tauri::command]
pub(crate) async fn list_presets<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, PresetService>,
    project_id: Option<ProjectId>,
) -> Result<Vec<Preset>, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    if project_id.is_none() {
        return Ok(presets::list());
    }
    run_blocking(move || service.list_presets(project_id))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn create_preset<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, PresetService>,
    project_id: ProjectId,
    value: CreatePresetInput,
) -> Result<Preset, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.create_preset(project_id, value))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn update_preset<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, PresetService>,
    project_id: ProjectId,
    preset_id: PresetId,
    expected_revision: u64,
    value: UpdatePresetInput,
) -> Result<Preset, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.update_preset(project_id, preset_id, expected_revision, value))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn duplicate_preset<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, PresetService>,
    project_id: ProjectId,
    preset_id: PresetId,
    name: String,
) -> Result<Preset, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.duplicate_preset(project_id, preset_id, name))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn delete_custom_preset<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, PresetService>,
    project_id: ProjectId,
    preset_id: PresetId,
    expected_revision: u64,
) -> Result<(), CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.delete_custom_preset(project_id, preset_id, expected_revision))
        .await
        .map_err(record_error)
}

/// Writes a copy of one custom preset wherever the user chooses.
///
/// The document that leaves the workspace is the one the workspace holds,
/// copied rather than re-rendered, so an exported preset is the same portable
/// Markdown a person would find in the project folder. The destination comes
/// from a Rust-owned save dialog: the frontend neither sends a path nor learns
/// one it did not just choose.
#[tauri::command]
pub(crate) async fn export_preset<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, PresetService>,
    project_id: ProjectId,
    preset_id: PresetId,
) -> Result<PresetExport, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || {
        let (preset, bytes) = service.read_preset_document(project_id, preset_id)?;
        let destination = pick_export_path(&preset.document_name)
            .ok_or_else(|| AppError::preset_error("preset_export_cancelled"))?;
        write_export(&destination, &bytes)?;
        Ok(PresetExport {
            path: destination.to_string_lossy().into_owned(),
            files: 1,
            bytes: bytes.len() as u64,
        })
    })
    .await
    .map_err(record_error)
}

fn pick_export_path(document_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export preset")
        .set_file_name(document_name)
        .add_filter("Markdown", &["md"])
        .save_file()
}

fn write_export(destination: &Path, bytes: &[u8]) -> Result<(), AppError> {
    std::fs::write(destination, bytes).map_err(|_| AppError::preset_error("preset_export_failed"))
}

fn authorized<T>(window_label: &str, access: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_main_window(window_label)?;
    Ok(access())
}

async fn run_blocking<T, F>(operation: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| AppError::preset_worker_failed())?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use super::authorized;

    #[test]
    fn preset_commands_authorize_before_reading_the_catalog() {
        assert_eq!(authorized("main", || 7).unwrap(), 7);
        for label in ["Main", "transcript", "insights", "unknown", ""] {
            let error = authorized(label, || panic!("must not reach the preset service"))
                .expect_err("other windows must be denied");
            assert_eq!(error.code, "command_not_authorized");
        }
    }
}
