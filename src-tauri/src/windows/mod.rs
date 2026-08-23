use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::domain::{AppError, SetTranscriptWindowAppearanceRequest, TranscriptWindowAppearance};

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

/// Carries the appearance to the transcript window, which holds only event
/// permissions and therefore cannot request it.
pub(crate) const TRANSCRIPT_WINDOW_APPEARANCE_EVENT: &str = "transcript-window-appearance";

/// Rust-owned appearance state for the transcript window.
///
/// The transcript window never invokes a command, so every value it renders
/// arrives through the appearance event: on page load for the initial state,
/// and on each accepted change afterwards.
#[derive(Clone, Default)]
pub(crate) struct TranscriptWindowService {
    appearance: Arc<Mutex<TranscriptWindowAppearance>>,
}

impl TranscriptWindowService {
    pub(crate) fn appearance(&self) -> Result<TranscriptWindowAppearance, AppError> {
        self.appearance
            .lock()
            .map(|appearance| *appearance)
            .map_err(|_| AppError::window_error("window_appearance_unavailable"))
    }

    fn store(
        &self,
        appearance: TranscriptWindowAppearance,
    ) -> Result<TranscriptWindowAppearance, AppError> {
        let mut current = self
            .appearance
            .lock()
            .map_err(|_| AppError::window_error("window_appearance_unavailable"))?;
        *current = appearance;
        Ok(appearance)
    }

    /// Applies an accepted appearance: the native always-on-top flag directly,
    /// and the background opacity and compact layout through the event the
    /// window listens for.
    pub(crate) fn apply<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        request: SetTranscriptWindowAppearanceRequest,
    ) -> Result<TranscriptWindowAppearance, AppError> {
        request.validate().map_err(AppError::window_error)?;
        let appearance = self.store(request.into_appearance())?;
        if let Some(window) = app.get_webview_window(TRANSCRIPT_WINDOW_LABEL) {
            window
                .set_always_on_top(appearance.always_on_top)
                .map_err(|_| AppError::window_error("window_always_on_top_failed"))?;
        }
        self.publish(app, appearance)?;
        Ok(appearance)
    }

    fn publish<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        appearance: TranscriptWindowAppearance,
    ) -> Result<(), AppError> {
        app.emit(TRANSCRIPT_WINDOW_APPEARANCE_EVENT, appearance)
            .map_err(|_| AppError::window_error("window_appearance_publish_failed"))
    }

    /// Opens the transcript window, or focuses it when it already exists.
    ///
    /// Reopening is deliberately idempotent: a user pressing the control twice
    /// must never end up with two transcript webviews.
    pub(crate) fn open<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), AppError> {
        if let Some(existing) = app.get_webview_window(TRANSCRIPT_WINDOW_LABEL) {
            existing
                .show()
                .and_then(|()| existing.unminimize())
                .and_then(|()| existing.set_focus())
                .map_err(|_| AppError::window_error("window_focus_failed"))?;
            return Ok(());
        }
        let appearance = self.appearance()?;
        let service = self.clone();
        let published = app.clone();
        WebviewWindowBuilder::new(
            app,
            TRANSCRIPT_WINDOW_LABEL,
            WebviewUrl::App(TRANSCRIPT_WINDOW_URL.into()),
        )
        .title(TRANSCRIPT_WINDOW_TITLE)
        .inner_size(TRANSCRIPT_WINDOW_WIDTH, TRANSCRIPT_WINDOW_HEIGHT)
        .min_inner_size(TRANSCRIPT_WINDOW_MIN_WIDTH, TRANSCRIPT_WINDOW_MIN_HEIGHT)
        .resizable(true)
        // Required for background opacity: the page paints its own translucent
        // background while text stays fully opaque.
        .transparent(true)
        .always_on_top(appearance.always_on_top)
        // The window cannot ask for its appearance, so publish it once the page
        // is ready to receive the event.
        .on_page_load(move |_window, _payload| {
            if let Ok(current) = service.appearance() {
                let _ = service.publish(&published, current);
            }
        })
        .build()
        .map_err(|_| AppError::window_error("window_open_failed"))?;
        Ok(())
    }

    /// Closes the transcript window. Closing an absent window succeeds so the
    /// control stays usable after the user closes the window themselves.
    pub(crate) fn close<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), AppError> {
        match app.get_webview_window(TRANSCRIPT_WINDOW_LABEL) {
            Some(existing) => existing
                .close()
                .map_err(|_| AppError::window_error("window_close_failed")),
            None => Ok(()),
        }
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

    #[test]
    fn a_new_service_starts_fully_opaque_and_unpinned() {
        let service = TranscriptWindowService::default();

        let appearance = service.appearance().unwrap();

        assert_eq!(appearance.background_opacity, 1.0);
        assert!(!appearance.always_on_top);
        assert!(!appearance.compact);
    }

    #[test]
    fn stored_appearance_survives_for_the_next_window_that_opens() {
        let service = TranscriptWindowService::default();
        let request: SetTranscriptWindowAppearanceRequest = serde_json::from_value(
            serde_json::json!({"backgroundOpacity": 0.5, "alwaysOnTop": true, "compact": true}),
        )
        .unwrap();

        let stored = service.store(request.into_appearance()).unwrap();

        assert_eq!(stored, service.appearance().unwrap());
        assert_eq!(service.appearance().unwrap().background_opacity, 0.5);
        assert!(service.appearance().unwrap().always_on_top);
    }
}
