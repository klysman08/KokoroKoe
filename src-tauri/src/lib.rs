mod audio;
mod commands;
mod domain;
mod logging;
mod persistence;
mod security;

use std::io;

use tauri::Manager;

use audio::AudioPrototypeService;
use persistence::SettingsService;

pub const PRODUCT_NAME: &str = "KokoroKoe";

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
            let settings = SettingsService::open(app_data_directory, documents_directory)
                .map_err(|_| io::Error::other("application settings initialization failed"))?;

            app.manage(settings);
            app.manage(AudioPrototypeService::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::choose_workspace,
            commands::audio::list_audio_devices,
            commands::audio::start_audio_capture_prototype,
            commands::audio::get_audio_capture_prototype_status,
            commands::audio::stop_audio_capture_prototype
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
