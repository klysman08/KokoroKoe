import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import recentInsightsFixture from "../../../fixtures/contracts/recent-insights-v1.json"
import { DetachedInsightsWindow } from "./DetachedInsightsWindow"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))

type Handler = (event: { payload: unknown }) => void

const listeners = new Map<string, Handler>()

const { requestId, projectId, sessionId, insights } =
  recentInsightsFixture.response

function publication(batch: unknown[] = insights) {
  return {
    schemaVersion: 1,
    requestId,
    projectId,
    sessionId,
    insights: batch,
  }
}

describe("DetachedInsightsWindow", () => {
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

  /// The window holds no command permission, so the only way it learns anything
  /// is the Rust-owned event.
  it("subscribes to insight batches without invoking any command", async () => {
    render(<DetachedInsightsWindow />)

    await waitFor(() => expect(listeners.has("session-insights")).toBe(true))
    expect(invoke).not.toHaveBeenCalled()
    expect(screen.getByText(/waiting for insights/i)).toBeInTheDocument()
  })

  it("renders one insight at a time and navigates between them", async () => {
    expect(insights.length).toBeGreaterThan(1)
    render(<DetachedInsightsWindow />)
    await waitFor(() => expect(listeners.has("session-insights")).toBe(true))

    listeners.get("session-insights")?.({ payload: publication() })

    expect(await screen.findByText(insights[0]!.title)).toBeInTheDocument()
    expect(screen.queryByText(insights[1]!.title)).not.toBeInTheDocument()
    expect(screen.getByText(`1 of ${insights.length}`)).toBeInTheDocument()

    await userEvent.click(screen.getByRole("button", { name: /next/i }))

    expect(await screen.findByText(insights[1]!.title)).toBeInTheDocument()
    expect(screen.queryByText(insights[0]!.title)).not.toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalled()
  })

  /// Insights describe the last few minutes, so a newer batch must replace the
  /// older one rather than leaving stale cards behind.
  it("replaces the previous batch and returns to the first insight", async () => {
    render(<DetachedInsightsWindow />)
    await waitFor(() => expect(listeners.has("session-insights")).toBe(true))

    listeners.get("session-insights")?.({ payload: publication() })
    await screen.findByText(insights[0]!.title)
    await userEvent.click(screen.getByRole("button", { name: /next/i }))
    await screen.findByText(insights[1]!.title)

    listeners.get("session-insights")?.({
      payload: publication([insights[0]]),
    })

    expect(await screen.findByText("1 of 1")).toBeInTheDocument()
    expect(screen.queryByText(insights[1]!.title)).not.toBeInTheDocument()
    // A single insight needs no navigation, so the controls disappear.
    expect(screen.queryByRole("button", { name: /next/i })).toBeNull()
  })

  it("ignores a batch that does not match the contract", async () => {
    render(<DetachedInsightsWindow />)
    await waitFor(() => expect(listeners.has("session-insights")).toBe(true))

    listeners.get("session-insights")?.({
      payload: { ...publication(), schemaVersion: 2 },
    })

    expect(screen.getByText(/waiting for insights/i)).toBeInTheDocument()
  })
})
