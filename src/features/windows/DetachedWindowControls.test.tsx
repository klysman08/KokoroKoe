import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
  setDetachedWindowAppearanceRequestSchema,
  setDetachedWindowInteractionRequestSchema,
  setDetachedWindowShortcutRequestSchema,
  type DetachedWindow,
} from "@/contracts/windows"
import { DetachedWindowControls } from "./DetachedWindowControls"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)

const defaultShortcuts: Record<DetachedWindow, string> = {
  transcript: "Ctrl+Shift+T",
  insights: "Ctrl+Shift+I",
}

/**
 * Answers as Rust does: every reply names the window it is about, and the
 * stored values differ per window so a cross-window leak would be visible.
 */
function respondPerWindow(overrides: { registered?: boolean } = {}) {
  invokeMock.mockImplementation(async (command, args) => {
    const request = (args as { request: Record<string, unknown> }).request
    const window = request.window as DetachedWindow
    if (command === "get_detached_window_appearance")
      return {
        window,
        schemaVersion: 1,
        backgroundOpacity: window === "transcript" ? 1 : 0.5,
        alwaysOnTop: false,
        compact: false,
      }
    if (command === "get_detached_window_shortcut")
      return {
        window,
        schemaVersion: 1,
        binding: defaultShortcuts[window],
        enabled: true,
        registered: overrides.registered ?? true,
      }
    if (command === "get_detached_window_interaction")
      return { window, schemaVersion: 1, clickThrough: false }
    if (command === "set_detached_window_appearance") {
      const parsed = setDetachedWindowAppearanceRequestSchema.parse(request)
      return { schemaVersion: 1, ...parsed }
    }
    if (command === "set_detached_window_shortcut") {
      const parsed = setDetachedWindowShortcutRequestSchema.parse(request)
      return { schemaVersion: 1, ...parsed, registered: parsed.enabled }
    }
    if (command === "set_detached_window_interaction") {
      const parsed = setDetachedWindowInteractionRequestSchema.parse(request)
      return { schemaVersion: 1, ...parsed }
    }
    return undefined
  })
}

function renderControls(window: DetachedWindow = "transcript") {
  return render(<DetachedWindowControls collapsed={false} window={window} />)
}

describe("DetachedWindowControls", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("reads the Rust-owned state for the window it controls", async () => {
    respondPerWindow()

    renderControls("insights")

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "get_detached_window_appearance",
        { request: { window: "insights" } },
      ),
    )
    expect(screen.getByText(/50%/)).toBeInTheDocument()
    expect(screen.getByRole("button", { name: /on top/i })).toHaveAttribute(
      "aria-pressed",
      "false",
    )
  })

  /// The whole point of keying state by window: each panel must read and write
  /// only its own window.
  it("keeps the two windows' settings apart", async () => {
    respondPerWindow()

    const transcript = renderControls("transcript")
    await waitFor(() => expect(screen.getByText(/100%/)).toBeInTheDocument())
    transcript.unmount()
    invokeMock.mockClear()

    renderControls("insights")

    await waitFor(() => expect(screen.getByText(/50%/)).toBeInTheDocument())
    for (const [, args] of invokeMock.mock.calls) {
      expect((args as { request: { window: string } }).request.window).toBe(
        "insights",
      )
    }
  })

  it("renders nothing while the sidebar is collapsed", () => {
    respondPerWindow()

    render(<DetachedWindowControls collapsed window="transcript" />)

    expect(invokeMock).not.toHaveBeenCalled()
    expect(screen.queryByText(/background opacity/i)).toBeNull()
  })

  it("sends a validated appearance when always-on-top is toggled", async () => {
    respondPerWindow()
    renderControls()
    await waitFor(() => expect(invokeMock).toHaveBeenCalled())

    await userEvent.click(screen.getByRole("button", { name: /on top/i }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "set_detached_window_appearance",
        {
          request: {
            window: "transcript",
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
    respondPerWindow()
    renderControls()
    await waitFor(() => expect(invokeMock).toHaveBeenCalled())

    const slider = screen.getByLabelText(/background opacity/i)

    expect(slider).toHaveAttribute("min", "0.3")
    expect(slider).toHaveAttribute("max", "1")
  })

  it("applies a rebinding and shows the canonical result", async () => {
    respondPerWindow()
    renderControls("insights")
    await waitFor(() =>
      expect(screen.getByLabelText(/show\/hide shortcut/i)).toHaveValue(
        "Ctrl+Shift+I",
      ),
    )

    const field = screen.getByLabelText(/show\/hide shortcut/i)
    await userEvent.clear(field)
    await userEvent.type(field, "Alt+F9")
    await userEvent.click(screen.getByRole("button", { name: /apply/i }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_detached_window_shortcut", {
        request: { window: "insights", binding: "Alt+F9", enabled: true },
      }),
    )
  })

  it("warns when the system refused the binding", async () => {
    respondPerWindow({ registered: false })

    renderControls()

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        /is probably using it/i,
      ),
    )
  })

  it("keeps the window reachable when the shortcut is turned off", async () => {
    respondPerWindow()
    renderControls()
    await waitFor(() =>
      expect(
        screen.getByLabelText(/use this shortcut system-wide/i),
      ).toBeChecked(),
    )

    await userEvent.click(
      screen.getByLabelText(/use this shortcut system-wide/i),
    )

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_detached_window_shortcut", {
        request: {
          window: "transcript",
          binding: "Ctrl+Shift+T",
          enabled: false,
        },
      }),
    )
    // The visible recovery path must survive a disabled shortcut.
    expect(screen.getByRole("button", { name: /pop out/i })).toBeEnabled()
  })

  it("sends a validated click-through request and reflects the result", async () => {
    respondPerWindow()
    renderControls("insights")
    const toggle = await screen.findByLabelText(/let clicks pass through/i)
    await waitFor(() => expect(toggle).toBeEnabled())

    await userEvent.click(toggle)

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "set_detached_window_interaction",
        { request: { window: "insights", clickThrough: true } },
      ),
    )
    await waitFor(() => expect(toggle).toBeChecked())
  })

  /// The off switch lives in the main window, which never becomes
  /// click-through, so the user can always take pointer input back.
  it("keeps the click-through switch usable while click-through is on", async () => {
    respondPerWindow()
    renderControls()
    const toggle = await screen.findByLabelText(/let clicks pass through/i)
    await waitFor(() => expect(toggle).toBeEnabled())
    await userEvent.click(toggle)
    await waitFor(() => expect(toggle).toBeChecked())

    await userEvent.click(toggle)

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "set_detached_window_interaction",
        { request: { window: "transcript", clickThrough: false } },
      ),
    )
    await waitFor(() => expect(toggle).not.toBeChecked())
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

    renderControls()

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /could not complete the window operation/i,
      ),
    )
  })
})
