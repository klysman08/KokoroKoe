import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import sessionLifecycleFixture from "../../../fixtures/contracts/session-lifecycle-v1.json"
import { DetachedTranscriptWindow } from "./DetachedTranscriptWindow"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))

type Handler = (event: { payload: unknown }) => void

const listeners = new Map<string, Handler>()

describe("DetachedTranscriptWindow", () => {
  beforeEach(() => {
    listeners.clear()
    vi.mocked(invoke).mockReset()
    vi.mocked(listen).mockReset()
    vi.mocked(listen).mockImplementation(
      async (event: string, handler: unknown) => {
        listeners.set(event, handler as Handler)
        return () => listeners.delete(event)
      },
    )
  })

  it("subscribes to session events without invoking any command", async () => {
    render(<DetachedTranscriptWindow />)

    await waitFor(() => expect(listeners.size).toBe(5))
    expect(invoke).not.toHaveBeenCalled()
    expect(screen.getByText(/waiting for a session/i)).toBeInTheDocument()
  })

  it("renders a scoped final segment that arrives while it is open", async () => {
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))

    const handler = listeners.get("session-transcription-final")
    expect(handler).toBeDefined()
    handler?.({ payload: sessionLifecycleFixture.finalEvent })

    await waitFor(() =>
      expect(
        screen.getByText(
          sessionLifecycleFixture.finalEvent.event.payload.segment.text,
        ),
      ).toBeInTheDocument(),
    )
    expect(screen.getByText(/receiving/i)).toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalled()
  })

  /// The window holds no command permission, so its appearance can only reach
  /// it through the Rust-owned event.
  it("applies the appearance it receives without invoking a command", async () => {
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))

    listeners.get("transcript-window-appearance")?.({
      payload: {
        schemaVersion: 1,
        backgroundOpacity: 0.45,
        alwaysOnTop: true,
        compact: true,
      },
    })

    const surface = await screen.findByTestId("detached-transcript-window")
    await waitFor(() =>
      expect(surface).toHaveStyle({ "--transcript-window-opacity": "0.45" }),
    )
    expect(surface).toHaveAttribute("data-compact", "true")
    expect(invoke).not.toHaveBeenCalled()
  })

  it("ignores an appearance payload outside the readable range", async () => {
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))

    listeners.get("transcript-window-appearance")?.({
      payload: {
        schemaVersion: 1,
        backgroundOpacity: 0,
        alwaysOnTop: false,
        compact: false,
      },
    })

    const surface = await screen.findByTestId("detached-transcript-window")
    expect(surface).toHaveStyle({ "--transcript-window-opacity": "1" })
  })

  /// A click-through window looks exactly like a frozen one, so the badge is
  /// the only cue the user gets that the mouse is being passed through.
  it("shows a click-through indicator driven by the Rust-owned event", async () => {
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))
    expect(screen.queryByText(/clicks pass through/i)).not.toBeInTheDocument()

    listeners.get("transcript-window-interaction")?.({
      payload: { schemaVersion: 1, clickThrough: true },
    })
    expect(await screen.findByText(/clicks pass through/i)).toBeInTheDocument()

    listeners.get("transcript-window-interaction")?.({
      payload: { schemaVersion: 1, clickThrough: false },
    })
    await waitFor(() =>
      expect(
        screen.queryByText(/clicks pass through/i),
      ).not.toBeInTheDocument(),
    )
    expect(invoke).not.toHaveBeenCalled()
  })
})
