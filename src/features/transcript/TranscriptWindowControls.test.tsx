import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { setTranscriptWindowAppearanceRequestSchema } from "@/contracts/windows"
import { TranscriptWindowControls } from "./TranscriptWindowControls"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)

const defaultAppearance = {
  schemaVersion: 1,
  backgroundOpacity: 1,
  alwaysOnTop: false,
  compact: false,
}

function respondWithStoredAppearance() {
  invokeMock.mockImplementation(async (command, args) => {
    if (command === "get_transcript_window_appearance") return defaultAppearance
    if (command === "set_transcript_window_appearance") {
      const request = setTranscriptWindowAppearanceRequestSchema.parse(
        (args as { request: unknown }).request,
      )
      return { schemaVersion: 1, ...request }
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
