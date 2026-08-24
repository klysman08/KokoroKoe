# ADR 0008: Per-Window Capability Boundary for Secondary Windows

- Status: Accepted
- Date: 2026-08-23

## Context

Phase 6 requires separate transcription and insights windows with independent opacity, always-on-top, compact mode, persistent geometry, and quick-hide. Until now KokoroKoe shipped exactly one webview, and every Tauri command authorized the literal `main` window label before touching service state. That single-window assumption is what made the trust boundary easy to reason about: one capability file, one window, one authorization rule.

A second webview weakens that reasoning unless its authority is decided deliberately. A naive implementation would reuse the `main` capability for every window, which would silently hand a secondary window the credential, filesystem, model-management, session-lifecycle, and OpenRouter command surface it has no need for. Tauri also allows the frontend to create windows directly through `core:webview:allow-create-webview-window`, which would let React choose a window label and URL.

The secondary transcription window's actual job is narrow: display transcript records as they are produced. Rust already emits project- and session-scoped partial, final, gap, and persistence events globally, so that job needs no command at all.

## Decision

Give every window its own capability file scoped to exactly that window, and grant each capability only what that window's job requires.

The `main` capability keeps the product command surface. The `transcript` capability grants `core:event:allow-listen` and `core:event:allow-unlisten` and nothing else: no command, no window management, no filesystem, HTTP, shell, or process permission. A secondary window that needs data must receive it through an event Rust already emits, not by gaining a command.

Keep `authorize_main_window` as the authorization rule for every existing command, unchanged. Because secondary windows hold no command permissions, the capability layer and the Rust authorization layer independently reject the same calls; neither is load-bearing alone.

Rust owns secondary window creation. The label, URL, title, and initial size are constants in the `windows` module, and the opening command takes no argument, so the frontend cannot name, address, or size an arbitrary webview. Do not grant `core:webview:allow-create-webview-window` to any window. Opening an already-open window focuses it instead of creating a second one, and closing an absent window succeeds.

Only the `main` window is declared in `tauri.conf.json` and created at startup. Secondary windows exist only after an explicit user action.

## Consequences

- A secondary window cannot read credentials, touch the filesystem, manage models, drive session lifecycle, or reach OpenRouter, even if its webview content were compromised.
- Adding a future window is a deliberate act: it requires a new capability file, a new entry in the configuration, and a scoping test, rather than inheriting authority.
- A secondary window that later genuinely needs a command forces an explicit decision to widen its capability and to extend the authorization rule beyond the exact `main` label. That change should be recorded rather than made silently.
- Secondary windows see every globally emitted event, including transcript text. This is the same trust level as the main window and carries no credential material, but it means event payloads remain the boundary that must never carry secrets.
- Detached windows show only records that arrive while they are open, because live records are view-local by the P5-014 decision. The saved transcript remains the record of a whole Session.
- Window creation failure is a sanitized, retryable error; it never affects capture, transcription, or persistence.

## Evidence

P6-001 added the `transcript` window and capability. Capability tests assert that the transcript capability grants exactly the two event permissions and contains no `core:`, command, dialog, filesystem, HTTP, shell, or process grant; that each capability is scoped to exactly one window; and that the configuration declares only the main window at startup. A command test proves the `transcript` label is rejected before any window operation runs, and a frontend test proves the detached surface subscribes to events and invokes no command. The adapter test proves the open and close calls carry no label, URL, path, or dimension.

P6-006 added the `insights` window as the second application of this decision, and nothing about the decision had to change. The insights window is display-only for the same reason the transcript window is: Rust publishes each generated insight batch as an event, so the window needs no command to show one. The capability assertions are now a single shared check run against both display-only capabilities, so a third window cannot pass by being tested more loosely than the first two. The published batch is deliberately narrower than the command's reply — it carries the insights and their scope but no cost, budget, or retry accounting — which keeps the event boundary at the minimum the window actually renders.
