# ADR 0005: Local workspace selection and settings revisions

- Status: Accepted
- Date: 2026-08-08
- Task: P2-003

## Context

The main window needs to persist non-secret preferences and let the user choose a workspace without exposing a generic filesystem or dialog API to React. A displayed path is not proof that a directory remains safe: Windows junctions, symbolic links, removable/network volumes, folder synchronization, and replacement races can change where data is written.

## Decision

- Store non-secret application settings in bundled SQLite under the application-local data directory. Keep an append-only history of the latest three validated settings revisions.
- Require an optimistic `expectedRevision` for ordinary settings mutations. `workspacePath` is excluded from that patch and can change only through the Rust-owned native folder picker.
- Accept a selected workspace only when it is an existing directory beneath an absolute drive-letter path on a fixed local volume. Reject drive roots, relative/drive-relative paths, UNC/network paths, device or verbatim namespaces, alternate data streams, traversal components, reserved Windows names, and any symbolic link or reparse point found in its ancestor chain.
- Canonicalize the selected directory, perform a create/write/sync/delete probe, and query available space before persisting it. Only one picker may be active. Cancellation leaves settings unchanged.
- Run every command-side database, dialog, and filesystem operation outside the webview thread. Keep Tauri authorization restricted to the exact `main` label and the three settings commands.
- Treat `%USERPROFILE%\Documents\KokoroKoe` as the initial configured path. Rust validates the existing Documents parent, creates only the known-safe `KokoroKoe` leaf when it is absent, and applies the same canonicalization and write-health checks before storing it. If Documents fails the policy, no fallback location is selected and onboarding remains available. P2-003 does not write meeting content. Phase 4 must revalidate and pin the workspace directory identity before creating any project/session content.

## Consequences

- Redirected Documents folders, OneDrive-backed folders implemented through reparse points, network shares, removable media, and some enterprise folder-redirection configurations may be rejected. This is intentional for the Windows MVP.
- The write probe is point-in-time evidence, not a permanent sandbox. Canonical strings and `symlink_metadata` checks cannot eliminate every time-of-check/time-of-use race.
- Phase 4 persistence must use handle-based directory identity checks and repeat containment/reparse validation for each sensitive join/open. It must never trust the stored string alone.
- Windows or third-party synchronization software may still copy data from a syntactically local folder; the UI must warn users about that possibility.
