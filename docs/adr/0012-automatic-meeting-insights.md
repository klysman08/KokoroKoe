# ADR 0012: Automatic Insights While a Meeting Runs

- Status: Accepted
- Date: 2026-10-11

## Context

P5-016 made insight generation an explicit, per-request click: "Nothing is generated in the background or on a timer." That rule was chosen because generation sends meeting text to a third party and costs money, so a person should decide each time it happens.

The first real use showed what the rule costs. A casual user started a meeting, opened the insights window, and saw nothing — the button that would have produced something lived inside **Manage sessions** on a project card, and the detached window could only say "Generate insights in the main window." They reported that insights did not work. An assistant whose output has to be asked for at the right moment is not assisting during the meeting, which is when the Manifest says insights are for.

The user decided, when asked, that insights should be produced automatically during a meeting with an option to switch them off.

## Decision

While a session is transcribing, the meeting view generates recent insights on its own, under these rules:

- It runs only in the main window and only through the existing exact-`main` `generate_recent_insights` command. Rust is unchanged: each generation is still one bounded request, priced, reserved against the session budget, validated, and recorded in the usage ledger like any other.
- It runs only when new finalized speech has arrived since the last generation, and at most once every 90 seconds, so a silent meeting costs nothing and a busy one costs a bounded amount.
- It never runs while the session is paused, stopping, or stopped, and never runs two requests at once.
- A visible **Automatic insights** switch sits beside the insights, on by default, with a one-line statement of what leaves the device. Switching it off stops generation immediately; **Generate now** remains for a single explicit request.
- A failure that will not fix itself — no credential, no insights model, no insight types in the preset, payment or authentication refused, the session budget reached — switches automatic generation off and says why, so a broken configuration cannot keep spending attempts. A transient failure leaves it on and is shown.
- The session spending limit is the ceiling. Rust already refuses a request the remaining budget cannot cover.

The switch is held in the meeting view for that meeting and starts on for each new one. Nothing about it is persisted.

## Consequences

- Insights appear during a meeting without the user finding a button, which is the product the Manifest describes.
- Meeting text now leaves the device on a timer while the switch is on. What leaves is unchanged — the last twelve finalized segments and the frozen project, session, and preset context, never audio — but it is no longer one click per request. The privacy documentation and the meeting view say so.
- Spending is bounded by the session limit rather than by clicks. A user who wants nothing sent switches automatic insights off, or does not configure an insights model.
- The project view keeps its explicit **Generate insights** panel; it is not on a timer.
- Because the per-session limit still resets at restart (see P6-023's limitations), a restarted session can spend its limit again. Seeding the budget baseline from the usage ledger closes that, and is recorded as the next follow-up.
