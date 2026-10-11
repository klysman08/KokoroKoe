# ADR 0011: Project-Scoped Custom Presets

- Status: Accepted
- Date: 2026-09-04

## Context

P6-021 published the nine built-in presets as a compile-time catalog in Rust. Manifest section 8 also requires the other half: a user must be able to create, edit, duplicate, export, and delete presets of their own. That half needs storage, and the two documents that describe where it goes disagree.

Manifest section 11 draws `presets/` inside each project folder, beside `sessions/`. The architecture's command table declares `list_presets` with no argument and `update_preset`, `duplicate_preset`, `export_preset`, and `delete_custom_preset` addressed by a preset identifier alone — a shape that only works for a single library shared by every project. Neither document argues for its choice; the inconsistency has been there since Phase 1, and P6-021 did not have to resolve it because a compile-time catalog has no folder at all.

The repository's conflict order settles the precedence — the Manifest outranks the architecture — but the decision is worth recording rather than merely obeyed, because it is a product decision and not a filing one.

Two shapes were considered.

A workspace-wide library, in a `presets/` folder beside `projects/`, keeps the architecture's argument-free commands and lets a preset written once be used everywhere. Its cost lands where this application has been most careful: a project folder is the portable unit. The user is told the workspace is theirs, that Markdown is the source of truth, and that a project folder can be copied, restored by hand, and read in another editor. A project that depended on a preset stored outside itself would be a folder that no longer explains itself.

Per-project storage keeps that property. It costs reuse: a preset written for one project cannot be selected in another, and a user who wants it twice has to write it twice.

## Decision

Custom presets live in `projects/<project>/presets/`, as one Markdown document per preset, and every preset command is scoped by a project.

`list_presets` takes an optional `projectId`. Omitted, it returns the built-in catalog alone — which is what the project-creation form can offer, since no project exists yet to own a custom preset. Given one, it returns the built-ins followed by that project's custom presets. `create_preset`, `update_preset`, `duplicate_preset`, `export_preset`, and `delete_custom_preset` all carry a `projectId`, and a `Preset` record carries the project that owns it when it is not built in.

A built-in preset is never addressed by these write commands. `update_preset` and `delete_custom_preset` refuse a built-in identifier before touching the workspace, because the catalog is a compile-time constant and cannot be edited by anything at runtime; `duplicate_preset` accepts one, since duplicating a built-in into a project is the ordinary way to start a custom preset.

Deleting a custom preset moves its document to `trash/`, as P6-018 does for projects and sessions. Sessions are unaffected either way: a Session froze its own `PresetSnapshot` when it was created, so a deleted or edited preset cannot change how a past meeting reads.

## Consequences

- A project folder stays self-explaining. Copied to another machine or restored from the Trash, it carries the presets its sessions were built from, and the workspace rescan added by P6-019 finds them with everything else.
- The architecture's preset command signatures change, and the architecture is updated to match rather than left to disagree. This is the second time a command has taken a scope argument it was not specified with; ADR 0009 recorded the first, for the same reason — the narrower interface was specified before the shape of the data was known.
- A preset cannot be shared between projects inside the application. Copying the `.md` file into another project's `presets/` folder and rescanning is the manual route, and it works, because the document is portable Markdown and discovery reads the folder.
- Deleting a project takes its presets with it, into the same Trash folder as the rest of the project. Nothing is destroyed, and a restored project comes back with its presets.
- A preset that is edited or deleted does not reach backwards. Every Session that used it holds its own frozen copy, which is what P6-021 made true and this decision relies on.
- No new capability, window, or trust boundary. These are ordinary exact-`main` commands over the workspace Rust already owns, and the frontend still receives no path.

## Evidence

P6-022 implements this. Store tests assert a preset document is written inside its own project's `presets/` folder, that a document naming another project is rejected on read, that an update refuses a stale revision and an externally modified document, and that deletion moves the file to `trash/` rather than removing it. Command tests assert every preset command authorizes exact-`main` before any workspace access, and that `update_preset` and `delete_custom_preset` refuse a built-in identifier before opening the workspace. A frontend test asserts `list_presets` is invoked with a project scope wherever a preset is chosen for an existing project, and without one on the project-creation form.
