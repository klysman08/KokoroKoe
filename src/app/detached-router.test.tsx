import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))

import App from "@/App"

describe("detached window routing", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
    vi.mocked(listen).mockReset()
    vi.mocked(listen).mockResolvedValue(() => {})
  })

  it("renders the standalone transcript surface without the application shell", async () => {
    window.location.hash = "#/transcript-window"

    render(<App />)

    await waitFor(() =>
      expect(screen.getByText("Live transcript")).toBeInTheDocument(),
    )
    expect(
      screen.queryByRole("complementary", { name: /primary navigation/i }),
    ).not.toBeInTheDocument()
    expect(screen.queryByRole("link", { name: "Settings" })).toBeNull()
    expect(invoke).not.toHaveBeenCalled()
  })

  it("renders the standalone insights surface without the application shell", async () => {
    window.location.hash = "#/insights-window"

    render(<App />)

    await waitFor(() =>
      expect(screen.getByText("Insights")).toBeInTheDocument(),
    )
    expect(screen.queryByText("Live transcript")).not.toBeInTheDocument()
    expect(
      screen.queryByRole("complementary", { name: /primary navigation/i }),
    ).not.toBeInTheDocument()
    expect(invoke).not.toHaveBeenCalled()
  })

  /// A hash the application does not own must fall back to the main shell, so a
  /// stray fragment can never strand the user on a blank surface.
  it("renders the application shell for an unknown hash", async () => {
    window.location.hash = "#/not-a-window"

    render(<App />)

    await waitFor(() =>
      expect(
        screen.getByRole("complementary", { name: /primary navigation/i }),
      ).toBeInTheDocument(),
    )
  })
})
