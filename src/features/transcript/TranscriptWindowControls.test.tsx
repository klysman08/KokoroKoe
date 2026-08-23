import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
  setTranscriptWindowAppearanceRequestSchema,
  setTranscriptWindowShortcutRequestSchema,
} from "@/contracts/windows"
import { TranscriptWindowControls } from "./TranscriptWindowControls"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)

const defaultAppearance = {
  schemaVersion: 1,
  backgroundOpacity: 1,
  alwaysOnTop: false,
  compact: false,
}

const defaultShortcut = {
  schemaVersion: 1,
  binding: "Ctrl+Shift+T",
  enabled: true,
  registered: true,
}

function respondWithStoredAppearance(
  shortcut: Record<string, unknown> = defaultShortcut,
) {
  invokeMock.mockImplementation(async (command, args) => {
    if (command === "get_transcript_window_appearance") return defaultAppearance
    if (command === "get_transcript_window_shortcut") return shortcut
    if (command === "set_transcript_window_appearance") {
      const request = setTranscriptWindowAppearanceRequestSchema.parse(
        (args as { request: unknown }).request,
      )
      return { schemaVersion: 1, ...request }
    }
    if (command === "set_transcript_window_shortcut") {
      const request = setTranscriptWindowShortcutRequestSchema.parse(
        (args as { request: unknown }).request,
      )
      return { schemaVersion: 1, ...request, registered: request.enabled }
    }
    return undefined
  })
}

describe("TranscriptWindowControls", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("reads the Rust-owned appearance on mount", async () => {
    respondWithStoredAppearance()

    render(<TranscriptWindowControls collapsed={false} />)

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "get_transcript_window_appearance",
      ),
    )
    expect(screen.getByText(/100%/)).toBeInTheDocument()
    expect(screen.getByRole("button", { name: /on top/i })).toHaveAttribute(
      "aria-pressed",
      "false",
    )
  })

  it("renders nothing while the sidebar is collapsed", () => {
    respondWithStoredAppearance()

    render(<TranscriptWindowControls collapsed />)

    expect(invokeMock).not.toHaveBeenCalled()
    expect(screen.queryByText(/background opacity/i)).toBeNull()
  })

  it("sends a validated appearance when always-on-top is toggled", async () => {
    respondWithStoredAppearance()
    render(<TranscriptWindowControls collapsed={false} />)
    await waitFor(() => expect(invokeMock).toHaveBeenCalled())

    await userEvent.click(screen.getByRole("button", { name: /on top/i }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "set_transcript_window_appearance",
        {
          request: {
            backgroundOpacity: 1,
            alwaysOnTop: true,
            compact: false,
          },
        },
      ),
    )
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /on top/i })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    )
  })

  it("keeps the opacity slider inside the readable range", async () => {
    respondWithStoredAppearance()
    render(<TranscriptWindowControls collapsed={false} />)
    await waitFor(() => expect(invokeMock).toHaveBeenCalled())

    const slider = screen.getByLabelText(/background opacity/i)

    expect(slider).toHaveAttribute("min", "0.3")
    expect(slider).toHaveAttribute("max", "1")
  })

  it("applies a rebinding and shows the canonical result", async () => {
    respondWithStoredAppearance()
    render(<TranscriptWindowControls collapsed={false} />)
    await waitFor(() =>
      expect(screen.getByLabelText(/show\/hide shortcut/i)).toHaveValue(
        "Ctrl+Shift+T",
      ),
    )

    const field = screen.getByLabelText(/show\/hide shortcut/i)
    await userEvent.clear(field)
    await userEvent.type(field, "Alt+F9")
    await userEvent.click(screen.getByRole("button", { name: /apply/i }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "set_transcript_window_shortcut",
        { request: { binding: "Alt+F9", enabled: true } },
      ),
    )
  })

  it("warns when the system refused the binding", async () => {
    respondWithStoredAppearance({
      schemaVersion: 1,
      binding: "Ctrl+Shift+T",
      enabled: true,
      registered: false,
    })

    render(<TranscriptWindowControls collapsed={false} />)

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        /another application is probably using it/i,
      ),
    )
  })

  it("keeps the window reachable when the shortcut is turned off", async () => {
    respondWithStoredAppearance()
    render(<TranscriptWindowControls collapsed={false} />)
    await waitFor(() =>
      expect(
        screen.getByLabelText(/use this shortcut system-wide/i),
      ).toBeChecked(),
    )

    await userEvent.click(
      screen.getByLabelText(/use this shortcut system-wide/i),
    )

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "set_transcript_window_shortcut",
        { request: { binding: "Ctrl+Shift+T", enabled: false } },
      ),
    )
    // The visible recovery path must survive a disabled shortcut.
    expect(screen.getByRole("button", { name: /pop out/i })).toBeEnabled()
  })

  it("surfaces a sanitized failure without changing the shown state", async () => {
    invokeMock.mockRejectedValue({
      error: {
        code: "window_appearance_unavailable",
        userMessage: "KokoroKoe could not complete the window operation.",
        technicalDetail: "window_appearance_unavailable",
        severity: "error",
        retryable: true,
        correlationId: "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
      },
    })

    render(<TranscriptWindowControls collapsed={false} />)

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /could not complete the window operation/i,
      ),
    )
  })
})
