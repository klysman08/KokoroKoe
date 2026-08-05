# ADR 0002: Independent Windows WASAPI Capture

- Status: Accepted
- Date: 2026-08-05

## Context

The MVP must capture a selected microphone and selected system output separately, preserve a common timeline, detect device changes, and recover one channel without stopping the other.

## Decision

Use independent event-driven WASAPI shared-mode capture threads. Capture the microphone from an input endpoint and system output through loopback on a render endpoint. Timestamp packets using WASAPI QPC timestamps relative to one session epoch. Keep per-source processing, resampling, levels, VAD, status, and recovery state.

Represent device choice as following a default role or using a fixed endpoint ID. Default selections follow default-device changes; fixed selections retry the same endpoint and never silently switch.

## Consequences

- Windows-specific Core Audio control is isolated behind an audio adapter for future platforms.
- Loopback captures the entire selected endpoint mix and cannot guarantee protected/exclusive-mode audio.
- Source labels describe capture origin, not speaker identity.
- Device/format/QPC behavior requires hardware prototype gates before Phase 3 acceptance.
