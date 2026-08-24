//! System-wide show/hide hotkeys for the detached windows.
//!
//! `RegisterHotKey` delivers `WM_HOTKEY` to the message queue of the thread
//! that registered it, so the binding is owned by a dedicated thread with its
//! own message loop rather than by Tauri's event loop.

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use crate::domain::ParsedShortcut;

/// How long a bind waits for the message-loop thread to answer.
const BIND_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

enum Request {
    Bind(i32, Option<ParsedShortcut>, mpsc::Sender<bool>),
    Shutdown,
}

/// Owns the registration thread and every combination currently bound on it.
///
/// One thread serves all windows: `RegisterHotKey` scopes registrations to the
/// registering thread, so a second thread would only add a second message loop
/// to keep alive for no benefit. Each window is told apart by its own id.
pub(crate) struct HotkeyController {
    requests: mpsc::Sender<Request>,
    thread_id: u32,
    handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl HotkeyController {
    /// Starts the registration thread. `on_pressed` runs on that thread, so it
    /// must not block; the caller dispatches real work elsewhere.
    pub(crate) fn start(on_pressed: Arc<dyn Fn(i32) + Send + Sync + 'static>) -> Option<Arc<Self>> {
        let (requests, incoming) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("kokorokoe-hotkey".to_owned())
            .spawn(move || run_message_loop(&incoming, &ready, on_pressed.as_ref()))
            .ok()?;
        let thread_id = started.recv().ok()?;
        Some(Arc::new(Self {
            requests,
            thread_id,
            handle: Mutex::new(Some(handle)),
        }))
    }

    /// Binds a combination, or clears the binding when `shortcut` is `None`.
    ///
    /// Returns whether the system accepted the registration; a combination
    /// already owned by another application is refused rather than silently
    /// doing nothing.
    pub(crate) fn bind(&self, id: i32, shortcut: Option<ParsedShortcut>) -> bool {
        let (answer, reply) = mpsc::channel();
        if self
            .requests
            .send(Request::Bind(id, shortcut, answer))
            .is_err()
        {
            return false;
        }
        wake(self.thread_id);
        // Bounded on purpose. Binding runs during application startup, so a
        // wake that never arrives must degrade to an unregistered shortcut the
        // user can see and retry, never to a frozen launch.
        reply.recv_timeout(BIND_REPLY_TIMEOUT).unwrap_or(false)
    }
}

impl Drop for HotkeyController {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Shutdown);
        wake(self.thread_id);
        if let Ok(mut handle) = self.handle.lock()
            && let Some(handle) = handle.take()
        {
            let _ = handle.join();
        }
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
        UnregisterHotKey,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY, WM_USER,
    };

    use crate::domain::ParsedShortcut;

    use super::Request;

    pub(super) fn modifiers(shortcut: &ParsedShortcut) -> HOT_KEY_MODIFIERS {
        let mut value = MOD_NOREPEAT;
        if shortcut.control {
            value |= MOD_CONTROL;
        }
        if shortcut.alt {
            value |= MOD_ALT;
        }
        if shortcut.shift {
            value |= MOD_SHIFT;
        }
        if shortcut.meta {
            value |= MOD_WIN;
        }
        value
    }

    pub(super) fn wake(thread_id: u32) {
        // Safety: posting a benign application message to our own thread.
        unsafe {
            PostThreadMessageW(thread_id, WM_APP, 0, 0);
        }
    }

    const fn empty_message() -> MSG {
        MSG {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
        }
    }

    pub(super) fn run_message_loop(
        incoming: &std::sync::mpsc::Receiver<Request>,
        ready: &std::sync::mpsc::Sender<u32>,
        on_pressed: &(dyn Fn(i32) + Send + Sync + 'static),
    ) {
        // Safety: reading this thread's own identifier.
        let thread_id = unsafe { GetCurrentThreadId() };
        // A thread has no message queue until it asks for one, and
        // `PostThreadMessageW` fails against a thread that has none. Announcing
        // readiness before the queue exists lets the very first `bind` post
        // into nothing and then block forever on its reply, so the queue is
        // forced into existence first. `PeekMessageW` is the documented way to
        // do that.
        let mut probe = empty_message();
        // Safety: peeking this thread's own queue with a valid buffer.
        unsafe {
            PeekMessageW(
                &mut probe,
                std::ptr::null_mut(),
                WM_USER,
                WM_USER,
                PM_NOREMOVE,
            );
        }
        if ready.send(thread_id).is_err() {
            return;
        }
        let mut bound: Vec<i32> = Vec::new();
        let mut message = empty_message();
        loop {
            // Safety: a thread-message loop with a valid message buffer.
            let received = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
            if received == -1 {
                break;
            }
            if received == 0 {
                break;
            }
            if message.message == WM_HOTKEY {
                on_pressed(message.wParam as i32);
                continue;
            }
            if message.message != WM_APP {
                continue;
            }
            let mut shutdown = false;
            while let Ok(request) = incoming.try_recv() {
                match request {
                    Request::Bind(id, shortcut, answer) => {
                        if let Some(position) = bound.iter().position(|held| *held == id) {
                            // Safety: unregistering a hotkey this thread owns.
                            unsafe {
                                UnregisterHotKey(std::ptr::null_mut(), id);
                            }
                            bound.swap_remove(position);
                        }
                        let accepted = match shortcut {
                            Some(shortcut) => {
                                // Safety: registering a hotkey for this thread
                                // with validated modifier and key values.
                                let result = unsafe {
                                    RegisterHotKey(
                                        std::ptr::null_mut(),
                                        id,
                                        modifiers(&shortcut),
                                        u32::from(shortcut.virtual_key),
                                    )
                                };
                                if result != 0 {
                                    bound.push(id);
                                }
                                result != 0
                            }
                            None => true,
                        };
                        let _ = answer.send(accepted);
                    }
                    Request::Shutdown => shutdown = true,
                }
            }
            if shutdown {
                break;
            }
        }
        for id in bound {
            // Safety: unregistering a hotkey this thread owns.
            unsafe {
                UnregisterHotKey(std::ptr::null_mut(), id);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::domain::ParsedShortcut;

    use super::Request;

    pub(super) fn wake(_thread_id: u32) {}

    pub(super) fn run_message_loop(
        incoming: &std::sync::mpsc::Receiver<Request>,
        ready: &std::sync::mpsc::Sender<u32>,
        _on_pressed: &(dyn Fn(i32) + Send + Sync + 'static),
    ) {
        if ready.send(0).is_err() {
            return;
        }
        while let Ok(request) = incoming.recv() {
            match request {
                Request::Bind(_id, _shortcut, answer) => {
                    let _ = answer.send(false);
                }
                Request::Shutdown => break,
            }
        }
    }

    pub(super) fn modifiers(_shortcut: &ParsedShortcut) -> u32 {
        0
    }
}

use platform::{run_message_loop, wake};

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn a_controller_starts_and_stops_cleanly() {
        let presses = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&presses);
        let controller = HotkeyController::start(Arc::new(move |_id| {
            observed.fetch_add(1, Ordering::SeqCst);
        }))
        .expect("the hotkey thread should start");

        // Clearing a binding always succeeds, even where registration cannot.
        assert!(controller.bind(1, None));
        assert_eq!(presses.load(Ordering::SeqCst), 0);

        drop(controller);
    }

    /// A thread has no message queue until it asks for one, and posting to a
    /// thread without one fails silently. Announcing readiness before the queue
    /// existed let the first bind post into nothing and block forever, so this
    /// binds immediately after start, repeatedly, to catch the race returning.
    #[test]
    fn the_first_bind_after_start_is_always_answered() {
        for _ in 0..25 {
            let controller =
                HotkeyController::start(Arc::new(|_id| {})).expect("the thread should start");

            assert!(
                controller.bind(1, None),
                "the first bind must be answered, not lost"
            );

            drop(controller);
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_validated_binding_is_accepted_and_can_be_replaced() {
        let controller =
            HotkeyController::start(Arc::new(|_id| {})).expect("the hotkey thread should start");

        // F24 combinations are unlikely to be owned by another application.
        let first = ParsedShortcut::parse("Ctrl+Alt+Shift+F24").unwrap();
        assert!(
            controller.bind(1, Some(first)),
            "an unused combination binds"
        );

        let second = ParsedShortcut::parse("Ctrl+Alt+Shift+F23").unwrap();
        assert!(
            controller.bind(1, Some(second)),
            "rebinding releases the first"
        );

        assert!(
            controller.bind(1, None),
            "clearing releases the registration"
        );
        drop(controller);
    }

    /// Two windows must be able to hold two different combinations at once,
    /// and releasing one must not release the other.
    #[cfg(windows)]
    #[test]
    fn two_windows_hold_independent_registrations() {
        let controller =
            HotkeyController::start(Arc::new(|_id| {})).expect("the hotkey thread should start");

        let first = ParsedShortcut::parse("Ctrl+Alt+Shift+F22").unwrap();
        let second = ParsedShortcut::parse("Ctrl+Alt+Shift+F21").unwrap();
        assert!(controller.bind(1, Some(first)));
        assert!(controller.bind(2, Some(second)));

        // Clearing the first leaves the second held, so rebinding the second's
        // combination onto the first id must be refused.
        assert!(controller.bind(1, None));
        assert!(
            !controller.bind(1, Some(second)),
            "a combination another registration owns is refused"
        );

        assert!(controller.bind(2, None));
        assert!(
            controller.bind(1, Some(second)),
            "the combination is available once its holder releases it"
        );
        assert!(controller.bind(1, None));
        drop(controller);
    }
}
