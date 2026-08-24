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

  /// The window holds no command permission, so its own opacity and compact
  /// layout can only reach it through the Rust-owned event.
  it("applies the appearance it receives without invoking a command", async () => {
    render(<DetachedInsightsWindow />)
    await waitFor(() =>
      expect(listeners.has("detached-window-appearance")).toBe(true),
    )

    listeners.get("detached-window-appearance")?.({
      payload: {
        window: "insights",
        schemaVersion: 1,
        backgroundOpacity: 0.45,
        alwaysOnTop: true,
        compact: true,
      },
    })

    const surface = await screen.findByTestId("detached-insights-window")
    await waitFor(() =>
      expect(surface).toHaveStyle({ "--insights-window-opacity": "0.45" }),
    )
    expect(surface).toHaveAttribute("data-compact", "true")
    expect(invoke).not.toHaveBeenCalled()
  })

  /// Both windows receive every event, so each must act only on its own.
  it("ignores state addressed to the other window", async () => {
    render(<DetachedInsightsWindow />)
    await waitFor(() =>
      expect(listeners.has("detached-window-appearance")).toBe(true),
    )

    listeners.get("detached-window-appearance")?.({
      payload: {
        window: "transcript",
        schemaVersion: 1,
        backgroundOpacity: 0.35,
        alwaysOnTop: false,
        compact: true,
      },
    })
    listeners.get("detached-window-interaction")?.({
      payload: { window: "transcript", schemaVersion: 1, clickThrough: true },
    })

    const surface = await screen.findByTestId("detached-insights-window")
    expect(surface).toHaveStyle({ "--insights-window-opacity": "1" })
    expect(surface).not.toHaveAttribute("data-compact")
    expect(screen.queryByText(/clicks pass through/i)).not.toBeInTheDocument()
  })

  it("shows a click-through indicator driven by the Rust-owned event", async () => {
    render(<DetachedInsightsWindow />)
    await waitFor(() =>
      expect(listeners.has("detached-window-interaction")).toBe(true),
    )

    listeners.get("detached-window-interaction")?.({
      payload: { window: "insights", schemaVersion: 1, clickThrough: true },
    })

    expect(await screen.findByText(/clicks pass through/i)).toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalled()
  })

  /// Pinning, dismissing, and copying are view-local, so none of them may
  /// invoke a command — the window holds no permission to run one.
  it("pins an insight so it survives the next batch, without a command", async () => {
    render(<DetachedInsightsWindow />)
    await waitFor(() => expect(listeners.has("session-insights")).toBe(true))
    listeners.get("session-insights")?.({ payload: publication() })
    await screen.findByText(insights[0]!.title)

    await userEvent.click(screen.getByRole("button", { name: /^pin$/i }))
    expect(
      await screen.findByRole("button", { name: /pinned/i }),
    ).toHaveAttribute("aria-pressed", "true")

    listeners.get("session-insights")?.({
      payload: publication([
        { ...insights[0], title: "A later observation", content: "later" },
      ]),
    })

    expect(await screen.findByText("2 of 2")).toBeInTheDocument()
    expect(screen.getByText("A later observation")).toBeInTheDocument()
    await userEvent.click(screen.getByRole("button", { name: /previous/i }))
    expect(await screen.findByText(insights[0]!.title)).toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalled()
  })

  /// The model has no memory of what it already said, so dismissing has to
  /// stick across batches or the control does nothing useful.
  it("dismisses an insight and keeps it from returning", async () => {
    render(<DetachedInsightsWindow />)
    await waitFor(() => expect(listeners.has("session-insights")).toBe(true))
    listeners.get("session-insights")?.({ payload: publication() })
    await screen.findByText(insights[0]!.title)

    await userEvent.click(screen.getByRole("button", { name: /dismiss/i }))

    expect(await screen.findByText("1 of 1")).toBeInTheDocument()
    expect(screen.queryByText(insights[0]!.title)).not.toBeInTheDocument()

    listeners.get("session-insights")?.({ payload: publication() })

    await waitFor(() => expect(screen.getByText("1 of 1")).toBeInTheDocument())
    expect(screen.queryByText(insights[0]!.title)).not.toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalled()
  })

  it("dismissing the last insight leaves the empty state, not a blank card", async () => {
    render(<DetachedInsightsWindow />)
    await waitFor(() => expect(listeners.has("session-insights")).toBe(true))
    listeners.get("session-insights")?.({ payload: publication([insights[0]]) })
    await screen.findByText(insights[0]!.title)

    await userEvent.click(screen.getByRole("button", { name: /dismiss/i }))

    expect(await screen.findByText(/waiting for insights/i)).toBeInTheDocument()
    expect(screen.queryByRole("button", { name: /dismiss/i })).toBeNull()
  })

  it("copies the insight text and reports a refused clipboard", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    })
    render(<DetachedInsightsWindow />)
    await waitFor(() => expect(listeners.has("session-insights")).toBe(true))
    listeners.get("session-insights")?.({ payload: publication() })
    await screen.findByText(insights[0]!.title)

    await userEvent.click(screen.getByRole("button", { name: /copy/i }))

    expect(await screen.findByRole("status")).toHaveTextContent(/copied/i)
    expect(writeText).toHaveBeenCalledWith(
      expect.stringContaining(insights[0]!.title),
    )

    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn().mockRejectedValue(new Error("denied")),
      },
    })
    await userEvent.click(screen.getByRole("button", { name: /copy/i }))

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        /clipboard is not available/i,
      ),
    )
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
