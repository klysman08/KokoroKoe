mod audio;
mod commands;
mod domain;
mod logging;
mod models;
mod persistence;
mod security;
#[allow(dead_code)] // P3-004 is an intentionally unwired runtime prototype.
mod transcription;

use std::io;

use tauri::Manager;

use audio::AudioDeviceTestService;
use models::ModelService;
use persistence::SettingsService;
#[cfg(windows)]
use transcription::LiveTranscriptionService;

pub const PRODUCT_NAME: &str = "KokoroKoe";

#[cfg(windows)]
pub fn run_transcription_worker_if_requested() -> bool {
    let requested = std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == transcription::WORKER_MODE_ARGUMENT);
    if requested {
        transcription::run_worker_if_requested();
    }
    requested
}

#[cfg(not(windows))]
pub const fn run_transcription_worker_if_requested() -> bool {
    false
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();

    tauri::Builder::default()
        .setup(|app| {
            let app_data_directory = app
                .path()
                .app_local_data_dir()
                .map_err(|_| io::Error::other("application settings directory is unavailable"))?;
            let documents_directory = app
                .path()
                .document_dir()
                .map_err(|_| io::Error::other("Windows Documents directory is unavailable"))?;
            let settings =
                SettingsService::open(app_data_directory.clone(), documents_directory)
                    .map_err(|_| io::Error::other("application settings initialization failed"))?;
            let models = ModelService::open(&app_data_directory, settings.clone())
                .map_err(|_| io::Error::other("model service initialization failed"))?;

            app.manage(settings);
            app.manage(models);
            app.manage(AudioDeviceTestService::default());
            #[cfg(windows)]
            {
                let adapter_path = std::env::current_exe()
                    .ok()
                    .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
                    .unwrap_or_else(|| app_data_directory.clone())
                    .join("kokorokoe_whisper_adapter.dll");
                let models = app.state::<ModelService>().inner().clone();
                app.manage(LiveTranscriptionService::new(models, adapter_path));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::choose_workspace,
            commands::models::list_transcription_models,
            commands::models::download_transcription_model,
            commands::models::cancel_model_download,
            commands::models::resume_model_download,
            commands::models::delete_transcription_model,
            commands::models::set_default_transcription_model,
            commands::audio::list_audio_devices,
            commands::audio::start_audio_device_test,
            commands::audio::stop_audio_device_test,
            #[cfg(windows)]
            commands::transcription::start_live_transcription,
            #[cfg(windows)]
            commands::transcription::get_live_transcription_status,
            #[cfg(windows)]
            commands::transcription::stop_live_transcription
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::PRODUCT_NAME;

    #[test]
    fn product_name_is_stable() {
        assert_eq!(PRODUCT_NAME, "KokoroKoe");
    }
}
