use tauri::WebviewWindow;

use crate::{
    domain::{AppError, CommandError, Preset},
    logging, presets,
    security::authorize_main_window,
};

/// Lists the built-in preset library.
///
/// The catalog is a compile-time constant, so this reads no file, holds no
/// service state, and cannot fail once the window is authorized. It stays an
/// exact-`main` command anyway: the detached windows have no business choosing
/// what a Session freezes.
#[tauri::command]
pub(crate) fn list_presets<R: tauri::Runtime>(
    webview_window: WebviewWindow<R>,
) -> Result<Vec<Preset>, CommandError> {
    authorized(webview_window.label(), presets::list).map_err(record_error)
}

fn authorized<T>(window_label: &str, access: impl FnOnce() -> T) -> Result<T, AppError> {
    authorize_main_window(window_label)?;
    Ok(access())
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
            let error = authorized(label, || panic!("must not read the catalog"))
                .expect_err("other windows must be denied");
            assert_eq!(error.code, "command_not_authorized");
        }
    }
}
