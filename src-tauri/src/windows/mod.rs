use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::domain::AppError;

/// The detached live-transcript window.
///
/// Its label, URL, title, and size are fixed here: React never supplies a
/// label, URL, path, or dimension, so no frontend input can create or address
/// an arbitrary webview.
pub(crate) const TRANSCRIPT_WINDOW_LABEL: &str = "transcript";
const TRANSCRIPT_WINDOW_URL: &str = "index.html#/transcript-window";
const TRANSCRIPT_WINDOW_TITLE: &str = "KokoroKoe transcript";
const TRANSCRIPT_WINDOW_WIDTH: f64 = 520.0;
const TRANSCRIPT_WINDOW_HEIGHT: f64 = 720.0;
const TRANSCRIPT_WINDOW_MIN_WIDTH: f64 = 360.0;
const TRANSCRIPT_WINDOW_MIN_HEIGHT: f64 = 320.0;

/// Opens the transcript window, or focuses it when it already exists.
///
/// Reopening is deliberately idempotent: a user pressing the control twice must
/// never end up with two transcript webviews.
pub(crate) fn open_transcript_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    if let Some(existing) = app.get_webview_window(TRANSCRIPT_WINDOW_LABEL) {
        existing
            .show()
            .and_then(|()| existing.unminimize())
            .and_then(|()| existing.set_focus())
            .map_err(|_| AppError::window_error("window_focus_failed"))?;
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        TRANSCRIPT_WINDOW_LABEL,
        WebviewUrl::App(TRANSCRIPT_WINDOW_URL.into()),
    )
    .title(TRANSCRIPT_WINDOW_TITLE)
    .inner_size(TRANSCRIPT_WINDOW_WIDTH, TRANSCRIPT_WINDOW_HEIGHT)
    .min_inner_size(TRANSCRIPT_WINDOW_MIN_WIDTH, TRANSCRIPT_WINDOW_MIN_HEIGHT)
    .resizable(true)
    .build()
    .map_err(|_| AppError::window_error("window_open_failed"))?;
    Ok(())
}

/// Closes the transcript window. Closing an absent window succeeds so the
/// control stays usable after the user closes the window themselves.
pub(crate) fn close_transcript_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    match app.get_webview_window(TRANSCRIPT_WINDOW_LABEL) {
        Some(existing) => existing
            .close()
            .map_err(|_| AppError::window_error("window_close_failed")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transcript_window_target_is_fixed_and_local() {
        assert_eq!(TRANSCRIPT_WINDOW_LABEL, "transcript");
        assert_eq!(TRANSCRIPT_WINDOW_URL, "index.html#/transcript-window");
        assert!(!TRANSCRIPT_WINDOW_URL.contains("://"));
        assert!(!TRANSCRIPT_WINDOW_URL.starts_with('/'));
        assert!(!TRANSCRIPT_WINDOW_URL.contains(".."));
    }
}
