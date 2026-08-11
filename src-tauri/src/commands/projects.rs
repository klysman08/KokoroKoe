use tauri::{State, WebviewWindow};

use crate::{
    domain::{
        AppError, CommandError, CreateProjectInput, LlmRoleModels, PageRequest, Project, ProjectId,
        ProjectPage, UpdateProjectInput,
    },
    logging,
    persistence::ProjectService,
    security::authorize_main_window,
};

#[tauri::command]
pub(crate) async fn list_projects<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ProjectService>,
    cursor: Option<String>,
    limit: u16,
) -> Result<ProjectPage, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.list_projects(PageRequest { cursor, limit }))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_project<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ProjectService>,
    project_id: ProjectId,
) -> Result<Project, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.get_project(project_id))
        .await
        .map_err(record_error)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn create_project<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ProjectService>,
    name: String,
    description: String,
    global_context: String,
    participants: Vec<String>,
    tags: Vec<String>,
    default_preset_id: crate::domain::PresetId,
    default_transcription_model_id: String,
    preferred_llm_models: LlmRoleModels,
) -> Result<Project, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    let input = CreateProjectInput {
        name,
        description,
        global_context,
        participants,
        tags,
        default_preset_id,
        default_transcription_model_id,
        preferred_llm_models,
    };
    run_blocking(move || service.create_project(input))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn update_project<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, ProjectService>,
    project_id: ProjectId,
    expected_revision: u64,
    value: UpdateProjectInput,
) -> Result<Project, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.update_project(project_id, expected_revision, value))
        .await
        .map_err(record_error)
}

fn authorized<T>(label: &str, operation: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_main_window(label)?;
    Ok(operation())
}

async fn run_blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| AppError::project_worker_failed())?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn every_project_command_uses_the_exact_main_window_authorizer() {
        let side_effects = AtomicUsize::new(0);
        for label in ["transcript", "insights", "unknown"] {
            let result = super::authorized(label, || side_effects.fetch_add(1, Ordering::SeqCst));
            assert_eq!(result.unwrap_err().code, "command_not_authorized");
        }
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);
        assert!(super::authorized("main", || ()).is_ok());
    }
}
