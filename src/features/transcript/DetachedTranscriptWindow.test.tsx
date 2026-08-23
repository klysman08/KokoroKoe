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

    await waitFor(() => expect(listeners.size).toBe(3))
    expect(invoke).not.toHaveBeenCalled()
    expect(screen.getByText(/waiting for a session/i)).toBeInTheDocument()
  })

  it("renders a scoped final segment that arrives while it is open", async () => {
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(3))

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
})
