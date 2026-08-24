# ADR 0009: Window-Keyed Detached Window State

- Status: Accepted
- Date: 2026-08-24

## Context

Manifest section 10 requires the transcription and insights windows to support **independent** opacity, always-on-top, resizing, persistent position and dimensions, compact mode, quick-hide, configurable show/hide shortcuts, and optional click-through. P6-002 through P6-005 built all of that for exactly one window: the service was named for the transcript window, held a single runtime, and the hotkey controller registered a single combination. P6-006 then added the insights window with none of it, which is the gap this decision closes.

ADR 0008 established that Rust owns secondary window creation, and stated the mechanism plainly: "the opening command takes no argument, so the frontend cannot name, address, or size an arbitrary webview." That worked while each window had its own dedicated command pair. With two windows and eight state commands, per-window command names would mean sixteen commands that differ only in a constant — a surface that grows with every window and that a reviewer has to check one by one.

The alternative is a window argument, which reopens the question ADR 0008 answered: can the frontend then name something it should not?

## Decision

Key every piece of detached-window state by window, and let the window-management commands take a window argument — as a **closed enum**, never a label.

`DetachedWindow` has exactly two variants. The frontend chooses a variant; it never supplies a label, URL, title, or size. An unknown value fails to deserialize before any command body runs, so the property ADR 0008 protects is preserved by the type rather than by the absence of an argument. Label, URL, title, and default size remain compile-time constants in the `windows` module, one entry per variant, and the label is single-sourced from the enum so the webview label, the persisted state key, and the capability file name cannot drift apart.

One service holds one runtime per window. Appearance, geometry, shortcut, and click-through are all per-window. The `window_state` table was already keyed by window label, so no migration is required.

Every value crossing to the frontend names its window, because every window receives every event and must ignore the ones that are not its own. The adapter additionally rejects a reply that names a different window than the one asked about: that would silently render one window's state under the other's controls.

One hotkey thread serves both windows. `RegisterHotKey` scopes registrations to the registering thread, so a second thread would only add a second message loop to keep alive. Each window has its own hotkey id and its own default binding; a shared default would mean the second registration is always refused.

The three P6-005 click-through recovery rails hold per window and are unchanged: never persisted, never engageable without the main window, and cleared for every window when the main window closes.

## Consequences

- Adding a third window is a new enum variant, a new spec entry, a new capability file, and a configuration entry. No new command, no new event, and no new control component.
- The command surface shrank: ten window commands became eight, and the main capability grants two fewer permissions than before.
- A window argument is now part of the trust boundary. It is a closed set, and a test asserts that `main`, a mis-cased variant, a path-like string, and an extra field are all rejected. That test is load-bearing and must not be weakened when a window is added.
- Every window sees every window's appearance and interaction events. This is the same trust level ADR 0008 already accepted for transcript events, and these payloads carry no meeting content — but a future per-window payload must not assume privacy from the other window.
- The type names changed from `TranscriptWindow*` to `DetachedWindow*`. Keeping the old names would have described the insights window's opacity with the transcript window's name.
- Two system-wide hotkeys are reserved while KokoroKoe runs instead of one. Both still require Ctrl, Alt, or Win, both can be disabled, and a refused registration is reported rather than retried.

## Evidence

P6-007 tests prove that changing one window's appearance leaves the other's untouched in memory and that both rows read back independently after a restart; that each window has a distinct hotkey id and a distinct default binding, and that an unknown id maps to no window; that two ids hold independent registrations, that releasing one does not release the other, and that a combination another registration owns is refused until its holder releases it; that click-through is absent from both windows' persisted state and restored for both after a restart; that only the windows Rust owns can be named; that the adapter sends nothing but the window name and rejects a reply about a different window; and that each detached surface ignores appearance and interaction events addressed to the other.
