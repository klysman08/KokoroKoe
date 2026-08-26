# ADR 0010: A Mark-Only Command for the Detached Transcript Window

- Status: Accepted
- Date: 2026-08-25

## Context

ADR 0008 gave every window its own capability and granted the detached transcript window event subscription and nothing else. It also named the moment this decision would face: "A secondary window that later genuinely needs a command forces an explicit decision to widen its capability and to extend the authorization rule beyond the exact `main` label. That change should be recorded rather than made silently."

That moment is marking a moment. Manifest section 9.2 lists flagging a segment as important, and P6-013 made it work in the main window's in-session live view. But the detached transcript window exists precisely so the transcript can be watched while the main window is hidden, quick-hidden, or behind a video call. The action a user most wants there — flag what was just said — is unreachable from the one surface they are actually looking at.

Two routes were considered and one was rejected.

Routing the action through the main window means granting the transcript window `core:event:allow-emit` and having the main window listen and invoke. That fails twice. Tauri does not scope emit by event name, so the window could forge any event the application publishes, including transcript finals and insight batches that other surfaces treat as Rust-published truth; ADR 0008's boundary assumes events flow Rust to windows and never back. And it is functionally wrong: the main window may be hidden or closed, which is the exact situation the detached window exists for, so the mark would silently not happen when it is most wanted.

## Decision

Give the detached transcript window exactly one command, `mark_transcript_segment`, and make the narrowness of that command the reason it is acceptable.

The command takes `MarkTranscriptSegmentRequest`: a project id, a session id, a segment id, and a boolean. It is deliberately not `AnnotateTranscriptSegmentRequest`. The request type is the boundary — there is no field of any shape through which this window could write text into the meeting record — and both the Rust type and the Zod schema reject unknown fields, so an added field is a compile-time and a test-time event rather than a silent one.

Authorization is exact-`transcript`, not exact-`main` and not a set including it. The main window keeps marking through `annotate_transcript_segment`, which it already holds. Holding the rule to the single window whose capability file grants the command keeps the capability layer and the Rust authorization layer saying exactly the same thing, which is the independence ADR 0008 relies on; a rule that permitted a window the capability denies would make the two layers disagree even though the stricter one wins.

The `transcript` capability gains `allow-mark-transcript-segment` and nothing else. The `insights` capability is unchanged and remains display-only. The shared capability assertion now takes the list of commands a window may hold, so the display-only case is the empty list and any future addition has to be written down in the test.

## Consequences

- The transcript window can flag a moment while the main window is hidden, which is the situation it was built for.
- If that webview's content were ever compromised, the worst it can do is toggle importance flags on segments in a session it is already displaying. It still cannot write text, read credentials, touch the filesystem, manage models, drive session lifecycle, or reach OpenRouter.
- The window can only name the scope it was told about, because the project and session identifiers come from the events it received rather than from anything it composed. Rust independently rejects a segment id that is not in that session's journal.
- ADR 0008 is amended in one respect and upheld in the rest: a display-only window is no longer the only shape a secondary window may take, but a secondary window still holds only what its job requires, still gets its own capability file, and still cannot create windows or name them.
- The insights window is unaffected and stays display-only. Generating an alternative version of an insight remains the open case there, and it is not covered by this decision: it would need a command that spends money and reaches OpenRouter, which is nothing like flipping a flag.
- The event bus stays one-way. No window has emit permission, and nothing here grants any.

## Evidence

P6-014 added the command. Authorization tests assert that exact-`transcript` is admitted and that `main`, `insights`, `Transcript`, `transcript-1`, `transcripts`, a trailing-space variant, the empty label, and an unknown label are all rejected before any service access; a further test asserts the widening does not reach any other transcript command, which still rejects the `transcript` label. A contract test asserts a marking request carrying a `text` field fails to deserialize, on both sides of the boundary. Capability tests assert the transcript capability holds exactly the two event permissions plus `allow-mark-transcript-segment`, that the insights capability holds the two event permissions and nothing more, and that neither may contain `allow-annotate-` — the command that can write text — alongside the existing filesystem, HTTP, shell, process, dialog, and window-management exclusions. Frontend tests assert that marking invokes exactly one command with a payload carrying no transcript text, that a partial and a gap offer nothing to mark, and that a refused mark surfaces a sanitized panel.
