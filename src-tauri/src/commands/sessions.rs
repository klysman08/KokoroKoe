use tauri::{State, WebviewWindow};

use crate::{
    domain::{
        AppError, AudioDeviceSnapshot, CommandError, CreateSessionSnapshotInput, LlmRoleModels,
        PageRequest, PresetSnapshot, ProjectId, Session, SessionId, SessionPage,
        UpdateSessionInput,
    },
    logging,
    persistence::SessionService,
    security::authorize_main_window,
};

#[tauri::command]
pub(crate) async fn list_sessions<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, SessionService>,
    project_id: ProjectId,
    cursor: Option<String>,
    limit: u16,
) -> Result<SessionPage, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.list_sessions(project_id, PageRequest { cursor, limit }))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_session<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, SessionService>,
    project_id: ProjectId,
    session_id: SessionId,
) -> Result<Session, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.get_session(project_id, session_id))
        .await
        .map_err(record_error)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn create_session<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, SessionService>,
    project_id: ProjectId,
    title: String,
    objective: String,
    session_context: String,
    preset: PresetSnapshot,
    language: String,
    microphone: AudioDeviceSnapshot,
    system_output: AudioDeviceSnapshot,
    transcription_model_id: String,
    llm_models: LlmRoleModels,
    retain_audio: bool,
) -> Result<Session, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    let input = CreateSessionSnapshotInput {
        title,
        objective,
        session_context,
        preset,
        language,
        microphone,
        system_output,
        transcription_model_id,
        llm_models,
        retain_audio,
    };
    run_blocking(move || service.create_session(project_id, input))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn update_session<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, SessionService>,
    project_id: ProjectId,
    session_id: SessionId,
    expected_revision: u64,
    value: UpdateSessionInput,
) -> Result<Session, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_blocking(move || service.update_session(project_id, session_id, expected_revision, value))
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
        .map_err(|_| AppError::session_worker_failed())?
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn every_session_command_uses_the_exact_main_window_authorizer() {
        let side_effects = AtomicUsize::new(0);
        for label in ["transcript", "insights", "unknown"] {
            let result = super::authorized(label, || side_effects.fetch_add(1, Ordering::SeqCst));
            assert_eq!(result.unwrap_err().code, "command_not_authorized");
        }
        assert_eq!(side_effects.load(Ordering::SeqCst), 0);
        assert!(super::authorized("main", || ()).is_ok());
    }
}
