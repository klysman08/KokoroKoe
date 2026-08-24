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
        AppError, DetachedWindow, DetachedWindowAppearance, DetachedWindowGeometry,
        DetachedWindowInteraction, DetachedWindowShortcutStatus, DetachedWindowState,
        DetachedWindowView, SetDetachedWindowAppearanceRequest,
        SetDetachedWindowInteractionRequest, SetDetachedWindowShortcutRequest, now_rfc3339,
    },
    persistence::SettingsService,
};

use hotkey::HotkeyController;

/// Everything Rust owns about a detached window's identity.
///
/// Label, URL, title, and size are compile-time constants: React names a window
/// by choosing a `DetachedWindow` variant and never supplies a label, URL,
/// path, or dimension, so no frontend input can create or address an arbitrary
/// webview.
struct WindowSpec {
    url: &'static str,
    title: &'static str,
    width: f64,
    height: f64,
    minimum_width: f64,
    minimum_height: f64,
}

const fn spec(window: DetachedWindow) -> WindowSpec {
    match window {
        DetachedWindow::Transcript => WindowSpec {
            url: "index.html#/transcript-window",
            title: "KokoroKoe transcript",
            width: 520.0,
            height: 720.0,
            minimum_width: 360.0,
            minimum_height: 320.0,
        },
        DetachedWindow::Insights => WindowSpec {
            url: "index.html#/insights-window",
            title: "KokoroKoe insights",
            width: 440.0,
            height: 560.0,
            minimum_width: 320.0,
            minimum_height: 280.0,
        },
    }
}

/// Dragging a window emits a continuous stream of move events. Geometry is kept
/// in memory and written at most this often, plus once when the window closes.
const GEOMETRY_WRITE_INTERVAL: Duration = Duration::from_secs(2);

/// Carries the appearance to a detached window, which holds only event
/// permissions and therefore cannot request it. The payload names its window,
/// because every window receives every event.
pub(crate) const DETACHED_WINDOW_APPEARANCE_EVENT: &str = "detached-window-appearance";

/// Carries the pointer-interaction state so a click-through window can show
/// that it is not accepting input.
pub(crate) const DETACHED_WINDOW_INTERACTION_EVENT: &str = "detached-window-interaction";

/// The window that owns every detached-window control.
const MAIN_WINDOW_LABEL: &str = "main";

struct WindowRuntime {
    state: DetachedWindowState,
    loaded: bool,
    last_write: Option<Instant>,
    /// False when the system refused the binding, usually because another
    /// application already owns the combination.
    shortcut_registered: bool,
    /// Never persisted: see `DetachedWindowInteraction`.
    click_through: bool,
}

impl WindowRuntime {
    fn new(window: DetachedWindow) -> Self {
        Self {
            state: DetachedWindowState::default_for(window),
            loaded: false,
            last_write: None,
            shortcut_registered: false,
            click_through: false,
        }
    }
}

/// Rust-owned appearance, geometry, shortcut, and pointer state for every
/// detached window, keyed by window.
///
/// A detached window never invokes a command, so every value it renders arrives
/// through an event: on page load for the initial state, and on each accepted
/// change afterwards.
#[derive(Clone)]
pub(crate) struct DetachedWindowService {
    settings: SettingsService,
    runtimes: Arc<[Mutex<WindowRuntime>; DetachedWindow::ALL.len()]>,
    hotkey: Arc<Mutex<Option<Arc<HotkeyController>>>>,
}

impl DetachedWindowService {
    pub(crate) fn new(settings: SettingsService) -> Self {
        Self {
            settings,
            runtimes: Arc::new([
                Mutex::new(WindowRuntime::new(DetachedWindow::Transcript)),
                Mutex::new(WindowRuntime::new(DetachedWindow::Insights)),
            ]),
            hotkey: Arc::new(Mutex::new(None)),
        }
    }

    /// Starts the system-wide show/hide hotkeys and binds each stored shortcut.
    ///
    /// A refused registration is recorded and reported rather than retried:
    /// every window always remains reachable through the main window's control,
    /// so a taken combination degrades one shortcut and nothing else.
    pub(crate) fn install_shortcuts<R: Runtime>(&self, app: &AppHandle<R>) {
        let service = self.clone();
        let target = app.clone();
        let controller = HotkeyController::start(Arc::new(move |id| {
            if let Some(window) = window_for_hotkey(id) {
                service.toggle(&target, window);
            }
        }));
        let Some(controller) = controller else {
            tracing::warn!("the show/hide shortcuts could not be started");
            return;
        };
        if let Ok(mut slot) = self.hotkey.lock() {
            *slot = Some(controller);
        }
        for window in DetachedWindow::ALL {
            let _ = self.rebind_shortcut(window);
        }
    }

    /// Applies a window's stored shortcut to the platform, returning whether it
    /// was accepted. A disabled shortcut clears the registration.
    fn rebind_shortcut(&self, window: DetachedWindow) -> Result<bool, AppError> {
        let state = self.hydrated(window)?;
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
        let registered = controller.bind(hotkey_id(window), binding) && state.shortcut.enabled;
        if let Ok(mut runtime) = self.locked(window) {
            runtime.shortcut_registered = registered;
        }
        Ok(registered)
    }

    pub(crate) fn shortcut_status(
        &self,
        window: DetachedWindow,
    ) -> Result<DetachedWindowView<DetachedWindowShortcutStatus>, AppError> {
        let state = self.hydrated(window)?;
        let registered = self
            .locked(window)
            .map(|runtime| runtime.shortcut_registered)
            .unwrap_or(false);
        Ok(DetachedWindowView::new(
            window,
            DetachedWindowShortcutStatus {
                schema_version: 1,
                binding: state.shortcut.binding,
                enabled: state.shortcut.enabled,
                registered,
            },
        ))
    }

    pub(crate) fn set_shortcut(
        &self,
        request: SetDetachedWindowShortcutRequest,
    ) -> Result<DetachedWindowView<DetachedWindowShortcutStatus>, AppError> {
        let window = request.window;
        let shortcut = request.into_shortcut().map_err(AppError::window_error)?;
        self.hydrated(window)?;
        let state = {
            let mut runtime = self.locked(window)?;
            runtime.state.shortcut = shortcut;
            runtime.state.clone()
        };
        self.persist(window, state)?;
        self.rebind_shortcut(window)?;
        self.shortcut_status(window)
    }

    pub(crate) fn interaction(
        &self,
        window: DetachedWindow,
    ) -> Result<DetachedWindowView<DetachedWindowInteraction>, AppError> {
        Ok(DetachedWindowView::new(
            window,
            DetachedWindowInteraction {
                schema_version: 1,
                click_through: self.locked(window)?.click_through,
            },
        ))
    }

    /// Turns mouse pass-through on or off for one window.
    ///
    /// Enabling requires the main window to exist, because the main window is
    /// the surface that can turn it back off. A click-through window cannot be
    /// clicked, dragged, or closed, so it must never become the only remaining
    /// interactive surface.
    pub(crate) fn set_click_through<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        request: SetDetachedWindowInteractionRequest,
    ) -> Result<DetachedWindowView<DetachedWindowInteraction>, AppError> {
        if !click_through_is_recoverable(
            request.click_through,
            app.get_webview_window(MAIN_WINDOW_LABEL).is_some(),
        ) {
            return Err(AppError::window_error("window_click_through_unrecoverable"));
        }
        self.apply_click_through(app, request.window, request.click_through)?;
        self.interaction(request.window)
    }

    fn apply_click_through<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        window: DetachedWindow,
        click_through: bool,
    ) -> Result<(), AppError> {
        if let Some(webview) = app.get_webview_window(window.label()) {
            webview
                .set_ignore_cursor_events(click_through)
                .map_err(|_| AppError::window_error("window_click_through_failed"))?;
        }
        self.locked(window)?.click_through = click_through;
        self.publish_interaction(app, window)
    }

    fn publish_interaction<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        window: DetachedWindow,
    ) -> Result<(), AppError> {
        let interaction = self.interaction(window)?;
        app.emit(DETACHED_WINDOW_INTERACTION_EVENT, interaction)
            .map_err(|_| AppError::window_error("window_interaction_publish_failed"))
    }

    /// Restores pointer input for every window when the main window goes away.
    ///
    /// Without this a detached window could be left click-through with no
    /// surface able to turn it off, which is the failure the R-010 rule guards
    /// against.
    pub(crate) fn restore_interaction_without_main_window<R: Runtime>(&self, app: &AppHandle<R>) {
        for window in DetachedWindow::ALL {
            let engaged = self.locked(window).map(|runtime| runtime.click_through);
            if engaged.unwrap_or(false) {
                let _ = self.apply_click_through(app, window, false);
                tracing::warn!(
                    window = window.label(),
                    "click-through cleared because the main window closed"
                );
            }
        }
    }

    /// Show/hide toggle used by a window's shortcut.
    ///
    /// An absent window is created, so the same combination always brings the
    /// window back: a quick-hide the user cannot undo would be the same trap as
    /// a window hidden off-screen.
    fn toggle<R: Runtime>(&self, app: &AppHandle<R>, window: DetachedWindow) {
        match app.get_webview_window(window.label()) {
            Some(webview) => {
                if webview.is_visible().unwrap_or(true) {
                    let _ = webview.hide();
                } else {
                    let _ = webview.show();
                    let _ = webview.unminimize();
                    let _ = webview.set_focus();
                }
            }
            None => {
                let _ = self.open(app, window);
            }
        }
    }

    fn locked(
        &self,
        window: DetachedWindow,
    ) -> Result<std::sync::MutexGuard<'_, WindowRuntime>, AppError> {
        self.runtimes[window.index()]
            .lock()
            .map_err(|_| AppError::window_error("window_appearance_unavailable"))
    }

    /// Loads one window's persisted state once per process. A failed or absent
    /// read leaves that window's defaults in place: remembered state is a
    /// convenience and must never prevent a window from opening.
    fn hydrated(&self, window: DetachedWindow) -> Result<DetachedWindowState, AppError> {
        {
            let runtime = self.locked(window)?;
            if runtime.loaded {
                return Ok(runtime.state.clone());
            }
        }
        let stored = self
            .settings
            .load_window_state(window.label())
            .unwrap_or(None);
        let mut runtime = self.locked(window)?;
        if !runtime.loaded {
            if let Some(state) = stored {
                runtime.state = state;
            }
            runtime.loaded = true;
        }
        Ok(runtime.state.clone())
    }

    pub(crate) fn appearance(
        &self,
        window: DetachedWindow,
    ) -> Result<DetachedWindowView<DetachedWindowAppearance>, AppError> {
        Ok(DetachedWindowView::new(
            window,
            self.hydrated(window)?.appearance,
        ))
    }

    fn persist(&self, window: DetachedWindow, state: DetachedWindowState) -> Result<(), AppError> {
        let updated_at = now_rfc3339()?;
        self.settings
            .save_window_state(window.label(), &state, &updated_at)
    }

    /// Applies an accepted appearance: the native always-on-top flag directly,
    /// and the background opacity and compact layout through the event the
    /// window listens for.
    pub(crate) fn apply<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        request: SetDetachedWindowAppearanceRequest,
    ) -> Result<DetachedWindowView<DetachedWindowAppearance>, AppError> {
        request.validate().map_err(AppError::window_error)?;
        let window = request.window;
        self.hydrated(window)?;
        let state = {
            let mut runtime = self.locked(window)?;
            runtime.state.appearance = request.into_appearance();
            runtime.state.clone()
        };
        if let Some(webview) = app.get_webview_window(window.label()) {
            webview
                .set_always_on_top(state.appearance.always_on_top)
                .map_err(|_| AppError::window_error("window_always_on_top_failed"))?;
        }
        let view = DetachedWindowView::new(window, state.appearance);
        self.publish(app, view.clone())?;
        self.persist(window, state)?;
        Ok(view)
    }

    fn publish<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        appearance: DetachedWindowView<DetachedWindowAppearance>,
    ) -> Result<(), AppError> {
        app.emit(DETACHED_WINDOW_APPEARANCE_EVENT, appearance)
            .map_err(|_| AppError::window_error("window_appearance_publish_failed"))
    }

    /// Records observed geometry, writing through at most once per interval so
    /// a drag does not become a stream of database writes.
    fn record_geometry(
        &self,
        window: DetachedWindow,
        geometry: DetachedWindowGeometry,
        force: bool,
    ) {
        if geometry.validate().is_err() {
            return;
        }
        let Ok(mut runtime) = self.locked(window) else {
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
        let _ = self.persist(window, state);
    }

    /// Opens a detached window, or focuses it when it already exists.
    ///
    /// Reopening is deliberately idempotent: a user pressing the control twice
    /// must never end up with two webviews for the same window.
    pub(crate) fn open<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        window: DetachedWindow,
    ) -> Result<(), AppError> {
        let spec = spec(window);
        if let Some(existing) = app.get_webview_window(window.label()) {
            existing
                .show()
                .and_then(|()| existing.unminimize())
                .and_then(|()| existing.set_focus())
                .map_err(|_| AppError::window_error("window_focus_failed"))?;
            return Ok(());
        }
        let state = self.hydrated(window)?;
        let restored = state.geometry.filter(|geometry| reachable(app, *geometry));
        let publisher = self.clone();
        let published = app.clone();
        let observer = self.clone();

        // Built hidden so a restored position is applied before the window is
        // ever painted, avoiding a visible jump from the default placement.
        let webview =
            WebviewWindowBuilder::new(app, window.label(), WebviewUrl::App(spec.url.into()))
                .title(spec.title)
                .inner_size(spec.width, spec.height)
                .min_inner_size(spec.minimum_width, spec.minimum_height)
                .resizable(true)
                .visible(false)
                // Required for background opacity: the page paints its own
                // translucent background while text stays fully opaque.
                .transparent(true)
                .always_on_top(state.appearance.always_on_top)
                // The window cannot ask for its own state, so publish it once the
                // page is ready to receive the events. Interaction is included
                // because a window reopened during click-through must say so from
                // the first paint.
                .on_page_load(move |_webview, _payload| {
                    if let Ok(current) = publisher.appearance(window) {
                        let _ = publisher.publish(&published, current);
                    }
                    let _ = publisher.publish_interaction(&published, window);
                })
                .build()
                .map_err(|_| AppError::window_error("window_open_failed"))?;

        if let Some(geometry) = restored {
            let _ = webview.set_size(PhysicalSize::new(geometry.width, geometry.height));
            let _ = webview.set_position(PhysicalPosition::new(geometry.x, geometry.y));
        } else {
            let _ = webview.set_size(LogicalSize::new(spec.width, spec.height));
            let _ = webview.center();
        }
        // A window reopened during a click-through session must match the state
        // the user last chose, which the freshly built window does not inherit.
        if self.locked(window)?.click_through {
            let _ = webview.set_ignore_cursor_events(true);
        }
        webview
            .show()
            .map_err(|_| AppError::window_error("window_open_failed"))?;

        let tracked = webview.clone();
        webview.on_window_event(move |event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                if let Some(geometry) = current_geometry(&tracked) {
                    observer.record_geometry(window, geometry, false);
                }
            }
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => {
                if let Some(geometry) = current_geometry(&tracked) {
                    observer.record_geometry(window, geometry, true);
                }
            }
            _ => {}
        });
        Ok(())
    }

    /// Closes a detached window. Closing an absent window succeeds so the
    /// control stays usable after the user closes the window themselves.
    pub(crate) fn close<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        window: DetachedWindow,
    ) -> Result<(), AppError> {
        match app.get_webview_window(window.label()) {
            Some(existing) => existing
                .close()
                .map_err(|_| AppError::window_error("window_close_failed")),
            None => Ok(()),
        }
    }
}

/// Hotkey identifiers are process-wide, so each window gets its own. Zero is
/// avoided so an uninitialized value can never look like a valid registration.
const fn hotkey_id(window: DetachedWindow) -> i32 {
    window.index() as i32 + 1
}

fn window_for_hotkey(id: i32) -> Option<DetachedWindow> {
    DetachedWindow::ALL
        .into_iter()
        .find(|window| hotkey_id(*window) == id)
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
) -> Option<DetachedWindowGeometry> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(DetachedWindowGeometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// A remembered position is only restored when a currently attached monitor
/// still shows enough of the window to grab it. Otherwise the window would
/// reopen on a display that is no longer there.
fn reachable<R: Runtime>(app: &AppHandle<R>, geometry: DetachedWindowGeometry) -> bool {
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

    fn service() -> (tempfile::TempDir, DetachedWindowService) {
        let root = tempdir().unwrap();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");
        std::fs::create_dir_all(&documents).unwrap();
        let settings = SettingsService::open(app_data, documents).unwrap();
        (root, DetachedWindowService::new(settings))
    }

    fn request(
        window: &str,
        opacity: f64,
        always_on_top: bool,
    ) -> SetDetachedWindowAppearanceRequest {
        serde_json::from_value(serde_json::json!({
            "window": window,
            "backgroundOpacity": opacity,
            "alwaysOnTop": always_on_top,
            "compact": true
        }))
        .unwrap()
    }

    #[test]
    fn every_window_target_is_fixed_and_local() {
        for window in DetachedWindow::ALL {
            let spec = spec(window);
            assert!(spec.url.starts_with("index.html#/"));
            assert!(!spec.url.contains("://"));
            assert!(!spec.url.contains(".."));
            assert!(spec.minimum_width <= spec.width);
            assert!(spec.minimum_height <= spec.height);
        }
    }

    /// One combination cannot show and hide two windows, and the platform
    /// refuses a second registration of the same binding.
    #[test]
    fn each_window_has_its_own_hotkey_identity_and_default_binding() {
        assert_ne!(
            hotkey_id(DetachedWindow::Transcript),
            hotkey_id(DetachedWindow::Insights)
        );
        assert_ne!(
            DetachedWindow::Transcript.default_shortcut_binding(),
            DetachedWindow::Insights.default_shortcut_binding()
        );
        for window in DetachedWindow::ALL {
            assert_eq!(window_for_hotkey(hotkey_id(window)), Some(window));
        }
        assert_eq!(window_for_hotkey(0), None);
        assert_eq!(window_for_hotkey(99), None);
    }

    #[test]
    fn a_service_without_stored_state_starts_fully_opaque_and_unpinned() {
        let (_root, service) = service();

        for window in DetachedWindow::ALL {
            let view = service.appearance(window).unwrap();

            assert_eq!(view.window, window);
            assert_eq!(view.value.background_opacity, 1.0);
            assert!(!view.value.always_on_top);
            assert!(!view.value.compact);
        }
    }

    /// The whole point of keying state by window: changing one window's state
    /// must leave the other exactly as it was, in memory and on disk.
    #[test]
    fn each_window_keeps_its_own_state() {
        let (root, service) = service();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");

        {
            let mut runtime = service.locked(DetachedWindow::Transcript).unwrap();
            runtime.state.appearance = request("transcript", 0.55, true).into_appearance();
            runtime.loaded = true;
        }
        let transcript = service
            .locked(DetachedWindow::Transcript)
            .unwrap()
            .state
            .clone();
        service
            .persist(DetachedWindow::Transcript, transcript)
            .unwrap();
        let insights_geometry = DetachedWindowGeometry {
            x: 10,
            y: 20,
            width: 440,
            height: 560,
        };
        service.record_geometry(DetachedWindow::Insights, insights_geometry, true);

        assert_eq!(
            service.appearance(DetachedWindow::Insights).unwrap().value,
            DetachedWindowAppearance::default(),
            "one window's appearance must not follow the other"
        );
        assert!(
            service
                .locked(DetachedWindow::Transcript)
                .unwrap()
                .state
                .geometry
                .is_none(),
            "one window's geometry must not follow the other"
        );

        // A new process reads both rows back independently.
        let restarted =
            DetachedWindowService::new(SettingsService::open(app_data, documents).unwrap());

        assert_eq!(
            restarted
                .hydrated(DetachedWindow::Transcript)
                .unwrap()
                .appearance
                .background_opacity,
            0.55
        );
        let insights = restarted.hydrated(DetachedWindow::Insights).unwrap();
        assert_eq!(insights.geometry, Some(insights_geometry));
        assert_eq!(insights.appearance.background_opacity, 1.0);
    }

    #[test]
    fn geometry_and_appearance_survive_a_restart() {
        let (root, service) = service();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");
        let geometry = DetachedWindowGeometry {
            x: -1_400,
            y: 120,
            width: 640,
            height: 900,
        };

        {
            let mut runtime = service.locked(DetachedWindow::Transcript).unwrap();
            runtime.state.appearance = request("transcript", 0.55, true).into_appearance();
            runtime.loaded = true;
        }
        service.record_geometry(DetachedWindow::Transcript, geometry, true);

        // A new process reads the same database.
        let restarted =
            DetachedWindowService::new(SettingsService::open(app_data, documents).unwrap());
        let state = restarted.hydrated(DetachedWindow::Transcript).unwrap();

        assert_eq!(state.appearance.background_opacity, 0.55);
        assert!(state.appearance.always_on_top);
        assert!(state.appearance.compact);
        assert_eq!(state.geometry, Some(geometry));
    }

    #[test]
    fn repeated_moves_are_throttled_but_a_close_always_writes() {
        let (_root, service) = service();
        service.hydrated(DetachedWindow::Transcript).unwrap();

        for offset in 0..5 {
            service.record_geometry(
                DetachedWindow::Transcript,
                DetachedWindowGeometry {
                    x: offset,
                    y: 0,
                    width: 520,
                    height: 720,
                },
                false,
            );
        }
        let after_moves = service
            .locked(DetachedWindow::Transcript)
            .unwrap()
            .last_write;
        assert!(after_moves.is_some(), "the first move writes immediately");

        let final_geometry = DetachedWindowGeometry {
            x: 900,
            y: 40,
            width: 520,
            height: 720,
        };
        service.record_geometry(DetachedWindow::Transcript, final_geometry, true);

        assert_eq!(
            service
                .locked(DetachedWindow::Transcript)
                .unwrap()
                .state
                .geometry,
            Some(final_geometry)
        );
        assert_eq!(
            service
                .settings
                .load_window_state(DetachedWindow::Transcript.label())
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
        service.hydrated(DetachedWindow::Transcript).unwrap();

        service.record_geometry(
            DetachedWindow::Transcript,
            DetachedWindowGeometry {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            true,
        );

        assert!(
            service
                .locked(DetachedWindow::Transcript)
                .unwrap()
                .state
                .geometry
                .is_none()
        );
    }

    /// The R-010 rule allows click-through only while pointer control is
    /// always recoverable. Persisting it would survive a restart or a crash and
    /// leave a window that cannot be clicked, dragged, or closed.
    #[test]
    fn click_through_is_never_written_to_persisted_state() {
        let (root, service) = service();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");

        for window in DetachedWindow::ALL {
            service.hydrated(window).unwrap();
            service.locked(window).unwrap().click_through = true;
            let state = service.locked(window).unwrap().state.clone();
            service.persist(window, state).unwrap();

            let stored = service
                .settings
                .load_window_state(window.label())
                .unwrap()
                .unwrap();
            let serialized = serde_json::to_string(&stored).unwrap();
            assert!(
                !serialized.contains("clickThrough"),
                "persisted state must not carry click-through"
            );
        }

        let restarted =
            DetachedWindowService::new(SettingsService::open(app_data, documents).unwrap());
        for window in DetachedWindow::ALL {
            restarted.hydrated(window).unwrap();
            assert!(
                !restarted.interaction(window).unwrap().value.click_through,
                "a restart must always restore pointer input"
            );
        }
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

        for window in DetachedWindow::ALL {
            let view = service.interaction(window).unwrap();

            assert_eq!(view.window, window);
            assert_eq!(view.value.schema_version, 1);
            assert!(!view.value.click_through);
        }
    }

    #[test]
    fn unreadable_stored_state_falls_back_to_defaults() {
        let (root, service) = service();
        let app_data = root.path().join("app-data");
        let documents = root.path().join("documents");
        service.hydrated(DetachedWindow::Transcript).unwrap();
        service
            .settings
            .save_window_state(
                DetachedWindow::Transcript.label(),
                &DetachedWindowState::default_for(DetachedWindow::Transcript),
                "2026-08-23T10:00:00Z",
            )
            .unwrap();

        let restarted =
            DetachedWindowService::new(SettingsService::open(app_data, documents).unwrap());

        let state = restarted.hydrated(DetachedWindow::Transcript).unwrap();
        assert_eq!(state.appearance.background_opacity, 1.0);
        assert!(state.geometry.is_none());
    }
}
