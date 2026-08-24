use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::domain::AppError;

/// The detached insights window.
///
/// As with the transcript window, its label, URL, title, and size are fixed
/// here: React never supplies a label, URL, path, or dimension, so no frontend
/// input can create or address an arbitrary webview.
pub(crate) const INSIGHTS_WINDOW_LABEL: &str = "insights";
const INSIGHTS_WINDOW_URL: &str = "index.html#/insights-window";
const INSIGHTS_WINDOW_TITLE: &str = "KokoroKoe insights";
const INSIGHTS_WINDOW_WIDTH: f64 = 440.0;
const INSIGHTS_WINDOW_HEIGHT: f64 = 560.0;
const INSIGHTS_WINDOW_MIN_WIDTH: f64 = 320.0;
const INSIGHTS_WINDOW_MIN_HEIGHT: f64 = 280.0;

/// Opens and closes the detached insights window.
///
/// It holds no state of its own. The window renders whatever insight batch Rust
/// publishes and nothing else, so unlike the transcript window it has no
/// appearance, geometry, shortcut, or pointer state to own yet.
#[derive(Clone, Default)]
pub(crate) struct InsightsWindowService;

impl InsightsWindowService {
    pub(crate) const fn new() -> Self {
        Self
    }

    /// Opens the insights window, or focuses it when it already exists.
    ///
    /// Reopening is idempotent for the same reason the transcript window's is:
    /// pressing the control twice must never produce two insight webviews.
    pub(crate) fn open<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), AppError> {
        if let Some(existing) = app.get_webview_window(INSIGHTS_WINDOW_LABEL) {
            return existing
                .show()
                .and_then(|()| existing.unminimize())
                .and_then(|()| existing.set_focus())
                .map_err(|_| AppError::window_error("window_focus_failed"));
        }
        WebviewWindowBuilder::new(
            app,
            INSIGHTS_WINDOW_LABEL,
            WebviewUrl::App(INSIGHTS_WINDOW_URL.into()),
        )
        .title(INSIGHTS_WINDOW_TITLE)
        .inner_size(INSIGHTS_WINDOW_WIDTH, INSIGHTS_WINDOW_HEIGHT)
        .min_inner_size(INSIGHTS_WINDOW_MIN_WIDTH, INSIGHTS_WINDOW_MIN_HEIGHT)
        .resizable(true)
        .build()
        .map(|_| ())
        .map_err(|_| AppError::window_error("window_open_failed"))
    }

    /// Closing a window that is not open succeeds: the user asked for it to be
    /// gone, and it is.
    pub(crate) fn close<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), AppError> {
        match app.get_webview_window(INSIGHTS_WINDOW_LABEL) {
            Some(existing) => existing
                .close()
                .map_err(|_| AppError::window_error("window_close_failed")),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{INSIGHTS_WINDOW_LABEL, INSIGHTS_WINDOW_URL};

    /// The window target is a compile-time constant so no frontend value can
    /// redirect the webview at a remote origin.
    #[test]
    fn the_insights_window_target_is_fixed_and_local() {
        assert_eq!(INSIGHTS_WINDOW_LABEL, "insights");
        assert!(INSIGHTS_WINDOW_URL.starts_with("index.html#/"));
        for forbidden in ["http://", "https://", "//", ".."] {
            assert!(
                !INSIGHTS_WINDOW_URL.contains(forbidden),
                "the insights window URL must stay local"
            );
        }
    }
}
