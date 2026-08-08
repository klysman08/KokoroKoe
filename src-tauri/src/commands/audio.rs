use tauri::{State, WebviewWindow};

use crate::{
    audio::{
        AudioDeviceList, AudioPrototypeService, AudioPrototypeStartRequest, AudioPrototypeStatus,
    },
    domain::{AppError, CommandError},
    logging,
    security::authorize_main_window,
};

#[tauri::command]
pub(crate) async fn list_audio_devices<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, AudioPrototypeService>,
) -> Result<AudioDeviceList, CommandError> {
    let service =
        authorized_service(webview_window.label(), state.inner()).map_err(record_error)?;
    run_audio_worker(move || service.list_devices())
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn start_audio_capture_prototype<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, AudioPrototypeService>,
    request: AudioPrototypeStartRequest,
) -> Result<AudioPrototypeStatus, CommandError> {
    let service =
        authorized_service(webview_window.label(), state.inner()).map_err(record_error)?;
    run_audio_worker(move || service.start(request))
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn get_audio_capture_prototype_status<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, AudioPrototypeService>,
) -> Result<AudioPrototypeStatus, CommandError> {
    let service =
        authorized_service(webview_window.label(), state.inner()).map_err(record_error)?;
    run_audio_worker(move || service.status())
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn stop_audio_capture_prototype<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, AudioPrototypeService>,
) -> Result<AudioPrototypeStatus, CommandError> {
    let service =
        authorized_service(webview_window.label(), state.inner()).map_err(record_error)?;
    run_audio_worker(move || service.stop())
        .await
        .map_err(record_error)
}

fn authorized_service(
    window_label: &str,
    service: &AudioPrototypeService,
) -> Result<AudioPrototypeService, AppError> {
    authorize_main_window(window_label)?;
    Ok(service.clone())
}

async fn run_audio_worker<T>(
    operation: impl FnOnce() -> Result<T, crate::audio::AudioPrototypeError> + Send + 'static,
) -> Result<T, AppError>
where
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| AppError::audio_prototype_failed("audio_prototype_worker_failed"))?
        .map_err(|error| AppError::audio_prototype_failed(error.code))
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use super::authorized_service;
    use crate::audio::AudioPrototypeService;

    #[test]
    fn audio_commands_authorize_before_cloning_the_service() {
        let service = AudioPrototypeService::default();
        assert!(authorized_service("main", &service).is_ok());
        for label in ["transcript", "insights", "unknown"] {
            let error = authorized_service(label, &service)
                .err()
                .expect("other windows must be denied");
            assert_eq!(error.code, "command_not_authorized");
        }
    }
}
