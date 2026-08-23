//! A system-wide show/hide hotkey for the transcript window.
//!
//! `RegisterHotKey` delivers `WM_HOTKEY` to the message queue of the thread
//! that registered it, so the binding is owned by a dedicated thread with its
//! own message loop rather than by Tauri's event loop.

use std::sync::{Arc, Mutex, mpsc};

use crate::domain::ParsedShortcut;

/// Identifies our hotkey within this process. Any value is fine as long as it
/// is stable and not shared with another registration we make.
const HOTKEY_ID: i32 = 1;

enum Request {
    Bind(Option<ParsedShortcut>, mpsc::Sender<bool>),
    Shutdown,
}

/// Owns the registration thread and the currently bound combination.
pub(crate) struct HotkeyController {
    requests: mpsc::Sender<Request>,
    thread_id: u32,
    handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl HotkeyController {
    /// Starts the registration thread. `on_pressed` runs on that thread, so it
    /// must not block; the caller dispatches real work elsewhere.
    pub(crate) fn start(on_pressed: Arc<dyn Fn() + Send + Sync + 'static>) -> Option<Arc<Self>> {
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
    pub(crate) fn bind(&self, shortcut: Option<ParsedShortcut>) -> bool {
        let (answer, reply) = mpsc::channel();
        if self.requests.send(Request::Bind(shortcut, answer)).is_err() {
            return false;
        }
        wake(self.thread_id);
        reply.recv().unwrap_or(false)
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
        GetMessageW, MSG, PostThreadMessageW, WM_APP, WM_HOTKEY,
    };

    use crate::domain::ParsedShortcut;

    use super::{HOTKEY_ID, Request};

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

    pub(super) fn run_message_loop(
        incoming: &std::sync::mpsc::Receiver<Request>,
        ready: &std::sync::mpsc::Sender<u32>,
        on_pressed: &(dyn Fn() + Send + Sync + 'static),
    ) {
        // Safety: reading this thread's own identifier.
        let thread_id = unsafe { GetCurrentThreadId() };
        if ready.send(thread_id).is_err() {
            return;
        }
        let mut bound = false;
        let mut message = MSG {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
        };
        loop {
            // Safety: a thread-message loop with a valid message buffer.
            let received = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
            if received == -1 {
                break;
            }
            if received == 0 {
                break;
            }
            if message.message == WM_HOTKEY && message.wParam as i32 == HOTKEY_ID {
                on_pressed();
                continue;
            }
            if message.message != WM_APP {
                continue;
            }
            let mut shutdown = false;
            while let Ok(request) = incoming.try_recv() {
                match request {
                    Request::Bind(shortcut, answer) => {
                        if bound {
                            // Safety: unregistering a hotkey this thread owns.
                            unsafe {
                                UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
                            }
                            bound = false;
                        }
                        let accepted = match shortcut {
                            Some(shortcut) => {
                                // Safety: registering a hotkey for this thread
                                // with validated modifier and key values.
                                let result = unsafe {
                                    RegisterHotKey(
                                        std::ptr::null_mut(),
                                        HOTKEY_ID,
                                        modifiers(&shortcut),
                                        u32::from(shortcut.virtual_key),
                                    )
                                };
                                bound = result != 0;
                                bound
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
        if bound {
            // Safety: unregistering a hotkey this thread owns.
            unsafe {
                UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
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
        _on_pressed: &(dyn Fn() + Send + Sync + 'static),
    ) {
        if ready.send(0).is_err() {
            return;
        }
        while let Ok(request) = incoming.recv() {
            match request {
                Request::Bind(_shortcut, answer) => {
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
        let controller = HotkeyController::start(Arc::new(move || {
            observed.fetch_add(1, Ordering::SeqCst);
        }))
        .expect("the hotkey thread should start");

        // Clearing a binding always succeeds, even where registration cannot.
        assert!(controller.bind(None));
        assert_eq!(presses.load(Ordering::SeqCst), 0);

        drop(controller);
    }

    #[cfg(windows)]
    #[test]
    fn a_validated_binding_is_accepted_and_can_be_replaced() {
        let controller =
            HotkeyController::start(Arc::new(|| {})).expect("the hotkey thread should start");

        // F24 combinations are unlikely to be owned by another application.
        let first = ParsedShortcut::parse("Ctrl+Alt+Shift+F24").unwrap();
        assert!(controller.bind(Some(first)), "an unused combination binds");

        let second = ParsedShortcut::parse("Ctrl+Alt+Shift+F23").unwrap();
        assert!(
            controller.bind(Some(second)),
            "rebinding releases the first"
        );

        assert!(controller.bind(None), "clearing releases the registration");
        drop(controller);
    }
}
