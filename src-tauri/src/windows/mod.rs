use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};

mod hotkey;

use crate::{
    domain::{
        AppError, SetTranscriptWindowAppearanceRequest, SetTranscriptWindowInteractionRequest,
        SetTranscriptWindowShortcutRequest, TranscriptWindowAppearance, TranscriptWindowGeometry,
        TranscriptWindowInteraction, TranscriptWindowShortcutStatus, TranscriptWindowState,
        now_rfc3339,
    },
    persistence::SettingsService,
};

use hotkey::HotkeyController;

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

/// Dragging a window emits a continuous stream of move events. Geometry is kept
/// in memory and written at most this often, plus once when the window closes.
const GEOMETRY_WRITE_INTERVAL: Duration = Duration::from_secs(2);

/// Carries the appearance to the transcript window, which holds only event
/// permissions and therefore cannot request it.
pub(crate) const TRANSCRIPT_WINDOW_APPEARANCE_EVENT: &str = "transcript-window-appearance";

/// Carries the pointer-interaction state to the transcript window so a
/// click-through window can show that it is not accepting input.
pub(crate) const TRANSCRIPT_WINDOW_INTERACTION_EVENT: &str = "transcript-window-interaction";

/// The window that owns every transcript-window control.
const MAIN_WINDOW_LABEL: &str = "main";

#[derive(Default)]
struct WindowRuntime {
    state: TranscriptWindowState,
    loaded: bool,
    last_write: Option<Instant>,
    /// False when the system refused the binding, usually because another
    /// application already owns the combination.
    shortcut_registered: bool,
    /// Never persisted: see `TranscriptWindowInteraction`.
    click_through: bool,
}

/// Rust-owned appearance and geometry for the transcript window.
///
/// The transcript window never invokes a command, so every value it renders
/// arrives through the appearance event: on page load for the initial state,
/// and on each accepted change afterwards.
#[derive(Clone)]
pub(crate) struct TranscriptWindowService {
    settings: SettingsService,
    runtime: Arc<Mutex<WindowRuntime>>,
    hotkey: Arc<Mutex<Option<Arc<HotkeyController>>>>,
}

impl TranscriptWindowService {
    pub(crate) fn new(settings: SettingsService) -> Self {
        Self {
            settings,
            runtime: Arc::new(Mutex::new(WindowRuntime::default())),
            hotkey: Arc::new(Mutex::new(None)),
        }
    }

    /// Starts the system-wide show/hide hotkey and binds the stored shortcut.
    ///
    /// A refused registration is recorded and reported rather than retried: the
    /// window always remains reachable through the main window's control, so a
    /// taken combination degrades the shortcut and nothing else.
    pub(crate) fn install_shortcut<R: Runtime>(&self, app: &AppHandle<R>) {
        let service = self.clone();
        let target = app.clone();
        let controller = HotkeyController::start(Arc::new(move || {
            service.toggle(&target);
        }));
        let Some(controller) = controller else {
            tracing::warn!("the show/hide shortcut could not be started");
            return;
        };
        if let Ok(mut slot) = self.hotkey.lock() {
            *slot = Some(controller);
        }
        let _ = self.rebind_shortcut();
    }

    /// Applies the stored shortcut to the platform, returning whether it was
    /// accepted. A disabled shortcut clears the registration.
    fn rebind_shortcut(&self) -> Result<bool, AppError> {
        let state = self.hydrated()?;
        let controller = self
            .hotkey
            .lock()
            .map_err(|_| AppError::window_error("window_shortcut_unavailable"))?
            .clone();
        let Some(controller) = controller else {
            return Ok(false);
        };
        let binding = if state.shortcut.enabled {
            Some(state.shortcut.parsed().map_err(AppError::window_error)?)
        } else {
            None
        };
        let registered = controller.bind(binding) && state.shortcut.enabled;
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.shortcut_registered = registered;
        }
        Ok(registered)
    }

    pub(crate) fn shortcut_status(&self) -> Result<TranscriptWindowShortcutStatus, AppError> {
        let state = self.hydrated()?;
        let registered = self
            .runtime
            .lock()
            .map(|runtime| runtime.shortcut_registered)
            .unwrap_or(false);
        Ok(TranscriptWindowShortcutStatus {
            schema_version: 1,
            binding: state.shortcut.binding,
            enabled: state.shortcut.enabled,
            registered,
        })
    }

    pub(crate) fn set_shortcut(
        &self,
        request: SetTranscriptWindowShortcutRequest,
    ) -> Result<TranscriptWindowShortcutStatus, AppError> {
        let shortcut = request.into_shortcut().map_err(AppError::window_error)?;
        self.hydrated()?;
        let state = {
            let mut runtime = self.locked()?;
            runtime.state.shortcut = shortcut;
            runtime.state.clone()
        };
        self.persist(state)?;
        self.rebind_shortcut()?;
        self.shortcut_status()
    }

    pub(crate) fn interaction(&self) -> Result<TranscriptWindowInteraction, AppError> {
        Ok(TranscriptWindowInteraction {
            schema_version: 1,
            click_through: self.locked()?.click_through,
        })
    }

    /// Turns mouse pass-through on or off for the transcript window.
    ///
    /// Enabling requires the main window to exist, because the main window is
    /// the surface that can turn it back off. A click-through window cannot be
    /// clicked, dragged, or closed, so it must never become the only remaining
    /// interactive surface.
    pub(crate) fn set_click_through<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        request: SetTranscriptWindowInteractionRequest,
    ) -> Result<TranscriptWindowInteraction, AppError> {
        if !click_through_is_recoverable(
            request.click_through,
            app.get_webview_window(MAIN_WINDOW_LABEL).is_some(),
        ) {
            return Err(AppError::window_error("window_click_through_unrecoverable"));
        }
        self.apply_click_through(app, request.click_through)?;
        self.interaction()
    }

    fn apply_click_through<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        click_through: bool,
    ) -> Result<(), AppError> {
        if let Some(window) = app.get_webview_window(TRANSCRIPT_WINDOW_LABEL) {
            window
                .set_ignore_cursor_events(click_through)
                .map_err(|_| AppError::window_error("window_click_through_failed"))?;
        }
        self.locked()?.click_through = click_through;
        let interaction = TranscriptWindowInteraction {
            schema_version: 1,
            click_through,
        };
        app.emit(TRANSCRIPT_WINDOW_INTERACTION_EVENT, interaction)
            .map_err(|_| AppError::window_error("window_interaction_publish_failed"))
    }

    /// Restores pointer input when the main window goes away.
    ///
    /// Without this the transcript window could be left click-through with no
    /// surface able to turn it off, which is the failure the R-010 rule guards
    /// against.
    pub(crate) fn restore_interaction_without_main_window<R: Runtime>(&self, app: &AppHandle<R>) {
        let engaged = self.runtime.lock().map(|runtime| runtime.click_through);
        if engaged.unwrap_or(false) {
            let _ = self.apply_click_through(app, false);
            tracing::warn!("click-through cleared because the main window closed");
        }
    }

    /// Show/hide toggle used by the shortcut.
    ///
    /// An absent window is created, so the same combination always brings the
    /// transcript back: a quick-hide the user cannot undo would be the same
    /// trap as a window hidden off-screen.
    fn toggle<R: Runtime>(&self, app: &AppHandle<R>) {
        match app.get_webview_window(TRANSCRIPT_WINDOW_LABEL) {
            Some(window) => {
                if window.is_visible().unwrap_or(true) {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            None => {
                let _ = self.open(app);
            }
        }
    }

    fn locked(&self) -> Result<std::sync::MutexGuard<'_, WindowRuntime>, AppError> {
        self.runtime
            .lock()
            .map_err(|_| AppError::window_error("window_appearance_unavailable"))
    }

    /// Loads persisted state once per process. A failed or absent read leaves
    /// the defaults in place: remembered state is a convenience and must never
    /// prevent the window from opening.
    fn hydrated(&self) -> Result<TranscriptWindowState, AppError> {
        {
            let runtime = self.locked()?;
            if runtime.loaded {
                return Ok(runtime.state.clone());
            }
        }
        let stored = self
            .settings
            .load_window_state(TRANSCRIPT_WINDOW_LABEL)
            .unwrap_or(None);
        let mut runtime = self.locked()?;
        if !runtime.loaded {
            if let Some(state) = stored {
                runtime.state = state;
            }
            runtime.loaded = true;
        }
        Ok(runtime.state.clone())
    }

    pub(crate) fn appearance(&self) -> Result<TranscriptWindowAppearance, AppError> {
        Ok(self.hydrated()?.appearance)
    }

    fn persist(&self, state: TranscriptWindowState) -> Result<(), AppError> {
        let updated_at = now_rfc3339()?;
        self.settings
            .save_window_state(TRANSCRIPT_WINDOW_LABEL, &state, &updated_at)
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
        self.hydrated()?;
        let state = {
            let mut runtime = self.locked()?;
            runtime.state.appearance = request.into_appearance();
            runtime.state.clone()
        };
        if let Some(window) = app.get_webview_window(TRANSCRIPT_WINDOW_LABEL) {
            window
                .set_always_on_top(state.appearance.always_on_top)
                .map_err(|_| AppError::window_error("window_always_on_top_failed"))?;
        }
        let appearance = state.appearance;
        self.publish(app, appearance)?;
        self.persist(state)?;
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

    /// Records observed geometry, writing through at most once per interval so
    /// a drag does not become a stream of database writes.
    fn record_geometry(&self, geometry: TranscriptWindowGeometry, force: bool) {
        if geometry.validate().is_err() {
            return;
        }
        let Ok(mut runtime) = self.runtime.lock() else {
            return;
        };
        if runtime.state.geometry == Some(geometry) && !force {
            return;
        }
        runtime.state.geometry = Some(geometry);
        let due = force
            || runtime
                .last_write
                .is_none_or(|last| last.elapsed() >= GEOMETRY_WRITE_INTERVAL);
        if !due {
            return;
        }
        runtime.last_write = Some(Instant::now());
        let state = runtime.state.clone();
        drop(runtime);
        let _ = self.persist(state);
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
        let state = self.hydrated()?;
        let restored = state.geometry.filter(|geometry| reachable(app, *geometry));
        let publisher = self.clone();
        let published = app.clone();
        let observer = self.clone();

        // Built hidden so a restored position is applied before the window is
        // ever painted, avoiding a visible jump from the default placement.
        let window = WebviewWindowBuilder::new(
            app,
            TRANSCRIPT_WINDOW_LABEL,
            WebviewUrl::App(TRANSCRIPT_WINDOW_URL.into()),
        )
        .title(TRANSCRIPT_WINDOW_TITLE)
        .inner_size(TRANSCRIPT_WINDOW_WIDTH, TRANSCRIPT_WINDOW_HEIGHT)
        .min_inner_size(TRANSCRIPT_WINDOW_MIN_WIDTH, TRANSCRIPT_WINDOW_MIN_HEIGHT)
        .resizable(true)
        .visible(false)
        // Required for background opacity: the page paints its own translucent
        // background while text stays fully opaque.
        .transparent(true)
        .always_on_top(state.appearance.always_on_top)
        // The window cannot ask for its own state, so publish it once the page
        // is ready to receive the events. Interaction is included because a
        // window reopened during click-through must say so from the first paint.
        .on_page_load(move |_window, _payload| {
            if let Ok(current) = publisher.appearance() {
                let _ = publisher.publish(&published, current);
            }
            if let Ok(current) = publisher.interaction() {
                let _ = published.emit(TRANSCRIPT_WINDOW_INTERACTION_EVENT, current);
            }
        })
        .build()
        .map_err(|_| AppError::window_error("window_open_failed"))?;

        if let Some(geometry) = restored {
            let _ = window.set_size(PhysicalSize::new(geometry.width, geometry.height));
            let _ = window.set_position(PhysicalPosition::new(geometry.x, geometry.y));
        } else {
            let _ = window.set_size(LogicalSize::new(
                TRANSCRIPT_WINDOW_WIDTH,
                TRANSCRIPT_WINDOW_HEIGHT,
            ));
            let _ = window.center();
        }
        // A window reopened during a click-through session must match the state
        // the user last chose, which the freshly built window does not inherit.
        if self.locked()?.click_through {
            let _ = window.set_ignore_cursor_events(true);
        }
        window
            .show()
            .map_err(|_| AppError::window_error("window_open_failed"))?;

        let tracked = window.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                if let Some(geometry) = current_geometry(&tracked) {
                    observer.record_geometry(geometry, false);
                }
            }
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => {
                if let Some(geometry) = current_geometry(&tracked) {
                    observer.record_geometry(geometry, true);
                }
            }
            _ => {}
        });
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

/// The R-010 recovery rule, stated once so it can be checked directly.
///
/// Turning click-through on is allowed only while a surface that can turn it
/// off is present; turning it off is always allowed.
const fn click_through_is_recoverable(requested: bool, main_window_present: bool) -> bool {
    !requested || main_window_present
}

fn current_geometry<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Option<TranscriptWindowGeometry> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(TranscriptWindowGeometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// A remembered position is only restored when a currently attached monitor
/// still shows enough of the window to grab it. Otherwise the window would
/// reopen on a display that is no longer there.
fn reachable<R: Runtime>(app: &AppHandle<R>, geometry: TranscriptWindowGeometry) -> bool {
    let Ok(monitors) = app.available_monitors() else {
        return false;
    };
    monitors.iter().any(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        geometry.is_reachable_on(position.x, position.y, size.width, size.height)
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn service() -> (tempfile::TempDir, TranscriptWindowService) {
        let root = tempdir().unwrap();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");
        std::fs::create_dir_all(&documents).unwrap();
        let settings = SettingsService::open(app_data, documents).unwrap();
        (root, TranscriptWindowService::new(settings))
    }

    fn request(opacity: f64, always_on_top: bool) -> SetTranscriptWindowAppearanceRequest {
        serde_json::from_value(serde_json::json!({
            "backgroundOpacity": opacity,
            "alwaysOnTop": always_on_top,
            "compact": true
        }))
        .unwrap()
    }

    #[test]
    fn the_transcript_window_target_is_fixed_and_local() {
        assert_eq!(TRANSCRIPT_WINDOW_LABEL, "transcript");
        assert_eq!(TRANSCRIPT_WINDOW_URL, "index.html#/transcript-window");
        assert!(!TRANSCRIPT_WINDOW_URL.contains("://"));
        assert!(!TRANSCRIPT_WINDOW_URL.starts_with('/'));
        assert!(!TRANSCRIPT_WINDOW_URL.contains(".."));
    }

    #[test]
    fn a_service_without_stored_state_starts_fully_opaque_and_unpinned() {
        let (_root, service) = service();

        let appearance = service.appearance().unwrap();

        assert_eq!(appearance.background_opacity, 1.0);
        assert!(!appearance.always_on_top);
        assert!(!appearance.compact);
    }

    #[test]
    fn geometry_and_appearance_survive_a_restart() {
        let (root, service) = service();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");
        let geometry = TranscriptWindowGeometry {
            x: -1_400,
            y: 120,
            width: 640,
            height: 900,
        };

        {
            let mut runtime = service.locked().unwrap();
            runtime.state.appearance = request(0.55, true).into_appearance();
            runtime.loaded = true;
        }
        service.record_geometry(geometry, true);
        let stored = service.persist(service.locked().unwrap().state.clone());
        stored.unwrap();

        // A new process reads the same database.
        let restarted =
            TranscriptWindowService::new(SettingsService::open(app_data, documents).unwrap());
        let state = restarted.hydrated().unwrap();

        assert_eq!(state.appearance.background_opacity, 0.55);
        assert!(state.appearance.always_on_top);
        assert!(state.appearance.compact);
        assert_eq!(state.geometry, Some(geometry));
    }

    #[test]
    fn repeated_moves_are_throttled_but_a_close_always_writes() {
        let (_root, service) = service();
        service.hydrated().unwrap();

        for offset in 0..5 {
            service.record_geometry(
                TranscriptWindowGeometry {
                    x: offset,
                    y: 0,
                    width: 520,
                    height: 720,
                },
                false,
            );
        }
        let after_moves = service.locked().unwrap().last_write;
        assert!(after_moves.is_some(), "the first move writes immediately");

        let final_geometry = TranscriptWindowGeometry {
            x: 900,
            y: 40,
            width: 520,
            height: 720,
        };
        service.record_geometry(final_geometry, true);

        assert_eq!(
            service.locked().unwrap().state.geometry,
            Some(final_geometry)
        );
        assert_eq!(
            service
                .settings
                .load_window_state(TRANSCRIPT_WINDOW_LABEL)
                .unwrap()
                .unwrap()
                .geometry,
            Some(final_geometry),
            "closing must persist the last position even mid-throttle"
        );
    }

    #[test]
    fn absurd_geometry_is_never_recorded() {
        let (_root, service) = service();
        service.hydrated().unwrap();

        service.record_geometry(
            TranscriptWindowGeometry {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            true,
        );

        assert!(service.locked().unwrap().state.geometry.is_none());
    }

    /// The R-010 rule allows click-through only while pointer control is
    /// always recoverable. Persisting it would survive a restart or a crash and
    /// leave a window that cannot be clicked, dragged, or closed.
    #[test]
    fn click_through_is_never_written_to_persisted_state() {
        let (root, service) = service();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");
        service.hydrated().unwrap();

        service.locked().unwrap().click_through = true;
        let state = service.locked().unwrap().state.clone();
        service.persist(state).unwrap();

        let stored = service
            .settings
            .load_window_state(TRANSCRIPT_WINDOW_LABEL)
            .unwrap()
            .unwrap();
        let serialized = serde_json::to_string(&stored).unwrap();
        assert!(
            !serialized.contains("clickThrough"),
            "persisted state must not carry click-through"
        );

        let restarted =
            TranscriptWindowService::new(SettingsService::open(app_data, documents).unwrap());
        restarted.hydrated().unwrap();
        assert!(
            !restarted.interaction().unwrap().click_through,
            "a restart must always restore pointer input"
        );
    }

    /// The main window is the only surface able to turn click-through off, so
    /// engaging it without one would produce exactly the trap R-010 forbids.
    /// Turning it off must never be blocked by the same rule.
    #[test]
    fn click_through_is_only_recoverable_with_a_main_window() {
        assert!(!click_through_is_recoverable(true, false));
        assert!(click_through_is_recoverable(true, true));
        assert!(click_through_is_recoverable(false, false));
        assert!(click_through_is_recoverable(false, true));
    }

    #[test]
    fn a_new_service_accepts_pointer_input() {
        let (_root, service) = service();

        let interaction = service.interaction().unwrap();

        assert_eq!(interaction.schema_version, 1);
        assert!(!interaction.click_through);
    }

    #[test]
    fn unreadable_stored_state_falls_back_to_defaults() {
        let (root, service) = service();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");
        service.hydrated().unwrap();
        service
            .settings
            .save_window_state(
                TRANSCRIPT_WINDOW_LABEL,
                &TranscriptWindowState::default(),
                "2026-08-23T10:00:00Z",
            )
            .unwrap();

        let restarted =
            TranscriptWindowService::new(SettingsService::open(app_data, documents).unwrap());

        let state = restarted.hydrated().unwrap();
        assert_eq!(state.appearance.background_opacity, 1.0);
        assert!(state.geometry.is_none());
    }
}
