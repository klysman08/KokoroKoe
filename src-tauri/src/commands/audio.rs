use std::{thread, time::Duration};

use tauri::{Emitter, State, WebviewWindow};

use crate::{
    audio::{
        AudioDeviceList, AudioDeviceStatusChanged, AudioDeviceTestObservation,
        AudioDeviceTestService, AudioEventEnvelope, AudioLevelUpdated, ChannelHealth,
        ChannelHealthStatus, DeviceTestInput, DeviceTestStatus,
    },
    domain::{AppError, CommandError, RequestId, now_rfc3339},
    logging,
    security::authorize_main_window,
};

const AUDIO_LEVEL_UPDATED_EVENT: &str = "audio-level-updated";
const AUDIO_DEVICE_STATUS_CHANGED_EVENT: &str = "audio-device-status-changed";
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[tauri::command]
pub(crate) async fn list_audio_devices<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, AudioDeviceTestService>,
) -> Result<AudioDeviceList, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_audio_worker(move || service.list_devices())
        .await
        .map_err(record_error)
}

#[tauri::command]
pub(crate) async fn start_audio_device_test<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, AudioDeviceTestService>,
    input: DeviceTestInput,
    request_id: RequestId,
) -> Result<DeviceTestStatus, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    let status = run_audio_worker({
        let service = service.clone();
        move || service.start(input, request_id)
    })
    .await
    .map_err(record_error)?;
    spawn_device_test_events(service, webview_window, request_id);
    Ok(status)
}

#[tauri::command]
pub(crate) async fn stop_audio_device_test<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
    state: State<'_, AudioDeviceTestService>,
    request_id: RequestId,
) -> Result<DeviceTestStatus, CommandError> {
    let service =
        authorized(webview_window.label(), || state.inner().clone()).map_err(record_error)?;
    run_audio_worker(move || service.stop(request_id))
        .await
        .map_err(record_error)
}

fn spawn_device_test_events<R: tauri::Runtime>(
    service: AudioDeviceTestService,
    window: WebviewWindow<R>,
    request_id: RequestId,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let mut previous_health = starting_health();
        let mut previous_level_ms = None;
        while let Ok(observation) = service.observe(request_id) {
            emit_health_if_changed(&window, request_id, &mut previous_health, &observation);
            emit_level_if_changed(&window, request_id, &mut previous_level_ms, &observation);
            if observation.finished {
                break;
            }
            thread::sleep(EVENT_POLL_INTERVAL);
        }
    });
}

fn emit_health_if_changed<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
    request_id: RequestId,
    previous: &mut ChannelHealth,
    observation: &AudioDeviceTestObservation,
) {
    if health_semantically_equal(previous, &observation.health) {
        return;
    }
    let endpoint_changed = previous.endpoint_id != observation.health.endpoint_id;
    let payload = AudioDeviceStatusChanged {
        source: observation.status.source,
        previous: previous.clone(),
        current: observation.health.clone(),
        is_default_change: observation.follows_default
            && previous.endpoint_id.is_some()
            && endpoint_changed,
    };
    if let Ok(event) = AudioEventEnvelope::new(request_id, payload)
        && window
            .emit(AUDIO_DEVICE_STATUS_CHANGED_EVENT, event)
            .is_err()
    {
        tracing::warn!("audio device status event could not be delivered to the main window");
    }
    *previous = observation.health.clone();
}

fn emit_level_if_changed<R: tauri::Runtime>(
    window: &WebviewWindow<R>,
    request_id: RequestId,
    previous_level_ms: &mut Option<u64>,
    observation: &AudioDeviceTestObservation,
) {
    let Some(level) = &observation.latest_level else {
        return;
    };
    if *previous_level_ms == Some(level.at_ms) {
        return;
    }
    let payload = AudioLevelUpdated {
        test_id: request_id,
        source: observation.status.source,
        rms_dbfs: level.rms_dbfs,
        peak_dbfs: level.peak_dbfs,
        clipping: level.clipping,
        muted: level.rms_dbfs <= -90.0,
        at_ms: level.at_ms,
    };
    if let Ok(event) = AudioEventEnvelope::new(request_id, payload)
        && window.emit(AUDIO_LEVEL_UPDATED_EVENT, event).is_err()
    {
        tracing::warn!("audio level event could not be delivered to the main window");
    }
    *previous_level_ms = Some(level.at_ms);
}

fn starting_health() -> ChannelHealth {
    ChannelHealth {
        status: ChannelHealthStatus::Starting,
        endpoint_id: None,
        detail_code: None,
        updated_at: now_rfc3339().unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned()),
    }
}

fn health_semantically_equal(left: &ChannelHealth, right: &ChannelHealth) -> bool {
    left.status == right.status
        && left.endpoint_id == right.endpoint_id
        && left.detail_code == right.detail_code
}

fn authorized<T>(window_label: &str, operation: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_main_window(window_label)?;
    Ok(operation())
}

async fn run_audio_worker<T>(
    operation: impl FnOnce() -> Result<T, crate::audio::AudioPrototypeError> + Send + 'static,
) -> Result<T, AppError>
where
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| AppError::audio_operation_failed("audio_worker_failed"))?
        .map_err(|error| AppError::audio_operation_failed(error.code))
}

fn record_error(error: AppError) -> CommandError {
    logging::record_app_error(&error);
    error.into()
}

#[cfg(test)]
mod tests {
    use super::{authorized, health_semantically_equal};
    use crate::audio::{ChannelHealth, ChannelHealthStatus};

    #[test]
    fn audio_commands_authorize_before_accessing_the_service() {
        assert_eq!(authorized("main", || 7).unwrap(), 7);
        for label in ["transcript", "insights", "unknown"] {
            let error = authorized(label, || panic!("must not access state"))
                .expect_err("other windows must be denied");
            assert_eq!(error.code, "command_not_authorized");
        }
    }

    #[test]
    fn health_event_comparison_ignores_only_the_timestamp() {
        let first = ChannelHealth {
            status: ChannelHealthStatus::Active,
            endpoint_id: Some("endpoint".to_owned()),
            detail_code: None,
            updated_at: "2026-08-11T10:00:00Z".to_owned(),
        };
        let mut second = first.clone();
        second.updated_at = "2026-08-11T10:00:01Z".to_owned();
        assert!(health_semantically_equal(&first, &second));
        second.status = ChannelHealthStatus::Silent;
        assert!(!health_semantically_equal(&first, &second));
    }
}
