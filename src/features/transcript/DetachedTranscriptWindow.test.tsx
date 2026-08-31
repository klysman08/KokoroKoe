import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import sessionLifecycleFixture from "../../../fixtures/contracts/session-lifecycle-v1.json"
import { useShellStore } from "@/stores/shell-store"
import { segmentAsText } from "./segment-actions"
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

    listeners.get("detached-window-appearance")?.({
      payload: {
        window: "transcript",
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

    listeners.get("detached-window-appearance")?.({
      payload: {
        window: "transcript",
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

    listeners.get("detached-window-interaction")?.({
      payload: { window: "transcript", schemaVersion: 1, clickThrough: true },
    })
    expect(await screen.findByText(/clicks pass through/i)).toBeInTheDocument()

    listeners.get("detached-window-interaction")?.({
      payload: { window: "transcript", schemaVersion: 1, clickThrough: false },
    })
    await waitFor(() =>
      expect(
        screen.queryByText(/clicks pass through/i),
      ).not.toBeInTheDocument(),
    )
    expect(invoke).not.toHaveBeenCalled()
  })
})

describe("DetachedTranscriptWindow marking", () => {
  const scoped = sessionLifecycleFixture.finalEvent
  const segment = scoped.event.payload.segment

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

  /// The one command this window holds. The scope comes from the events it
  /// received, and the request carries a flag and three identifiers -- there is
  /// no field through which this window could write text into the record.
  it("marks a finalized segment through the single command it holds", async () => {
    const user = userEvent.setup()
    // Rust answers with the saved-transcript view of the segment, not the live
    // record, so the adapter validates it against that contract.
    vi.mocked(invoke).mockResolvedValue({
      id: segment.id,
      projectId: scoped.projectId,
      sessionId: scoped.sessionId,
      source: segment.source,
      startMs: segment.startMs,
      endMs: segment.endMs,
      text: segment.text,
      status: "final",
      language: segment.language,
      important: true,
    })
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))
    listeners.get("session-transcription-final")?.({ payload: scoped })
    await screen.findByText(segment.text)

    await user.click(screen.getByRole("button", { name: /mark important/i }))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("mark_transcript_segment", {
        request: {
          projectId: scoped.projectId,
          sessionId: scoped.sessionId,
          segmentId: segment.id,
          important: true,
        },
      }),
    )
    expect(vi.mocked(invoke).mock.calls).toHaveLength(1)
    expect(JSON.stringify(vi.mocked(invoke).mock.calls[0])).not.toContain(
      segment.text,
    )
    expect(
      await screen.findByRole("button", { name: "Important" }),
    ).toHaveAttribute("aria-pressed", "true")
  })

  it("offers nothing to mark on a partial or a gap", async () => {
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))

    listeners.get("session-transcription-partial")?.({
      payload: sessionLifecycleFixture.partialEvent,
    })
    listeners.get("session-transcription-gap")?.({
      payload: sessionLifecycleFixture.gapEvent,
    })

    await waitFor(() =>
      expect(screen.getByText(/receiving/i)).toBeInTheDocument(),
    )
    expect(
      screen.queryByRole("button", { name: /mark important/i }),
    ).not.toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalled()
  })

  it("surfaces a sanitized failure when a mark is refused", async () => {
    const user = userEvent.setup()
    vi.mocked(invoke).mockRejectedValue({
      error: {
        code: "command_not_authorized",
        userMessage: "KokoroKoe blocked an unauthorized request.",
        technicalDetail: "command_not_authorized",
        severity: "error",
        retryable: false,
        correlationId: "ffffffff-ffff-4fff-8fff-ffffffffffff",
      },
    })
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))
    listeners.get("session-transcription-final")?.({ payload: scoped })
    await screen.findByText(segment.text)

    await user.click(screen.getByRole("button", { name: /mark important/i }))

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /blocked an unauthorized request/i,
    )
  })
})

describe("DetachedTranscriptWindow theme", () => {
  beforeEach(() => {
    document.documentElement.classList.remove("dark")
    useShellStore.setState({ theme: "dark" })
    vi.mocked(listen).mockReset()
    vi.mocked(listen).mockImplementation(async () => () => undefined)
  })

  /// The window renders the same document under a different route, so it has
  /// to paint itself: nothing else in this webview applies the theme.
  it("wears the application theme", () => {
    render(<DetachedTranscriptWindow />)

    expect(document.documentElement).toHaveClass("dark")
  })
})

describe("DetachedTranscriptWindow copying", () => {
  const scoped = sessionLifecycleFixture.finalEvent
  const segment = scoped.event.payload.segment

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

  /// The clipboard is a browser API, so copying needs no command and widens
  /// nothing this window may do — which is why a display-only window can have
  /// it at all.
  it("copies a segment without invoking any command", async () => {
    const user = userEvent.setup()
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    })
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))
    listeners.get("session-transcription-final")?.({ payload: scoped })
    await screen.findByText(segment.text)

    await user.click(screen.getByRole("button", { name: "Copy" }))

    expect(writeText).toHaveBeenCalledWith(
      segmentAsText({
        source: segment.source as "microphone" | "system_output",
        startMs: segment.startMs,
        text: segment.text,
      }),
    )
    expect(invoke).not.toHaveBeenCalled()
    expect(
      await screen.findByRole("button", { name: "Copied" }),
    ).toBeInTheDocument()
  })

  it("reports a refused clipboard instead of claiming a copy", async () => {
    const user = userEvent.setup()
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
    })
    render(<DetachedTranscriptWindow />)
    await waitFor(() => expect(listeners.size).toBe(5))
    listeners.get("session-transcription-final")?.({ payload: scoped })
    await screen.findByText(segment.text)

    await user.click(screen.getByRole("button", { name: "Copy" }))

    expect(
      await screen.findByText(/the clipboard is not available/i),
    ).toBeInTheDocument()
    expect(
      screen.queryByRole("button", { name: "Copied" }),
    ).not.toBeInTheDocument()
  })
})
