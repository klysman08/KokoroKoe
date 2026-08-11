import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import appSettingsFixture from "../../fixtures/contracts/app-settings-v1.json"

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((command: string) => {
    if (command === "get_settings") return Promise.resolve(appSettingsFixture)
    if (command === "list_projects") return Promise.resolve({ items: [] })
    return Promise.reject(new Error(`Unexpected command: ${command}`))
  }),
}))

import App from "@/App"
import { useNavigationStore } from "@/stores/navigation-store"
import { useShellStore } from "@/stores/shell-store"

describe("application routing", () => {
  beforeEach(() => {
    window.location.hash = "#/"
    useNavigationStore.setState({ activeRoute: "home" })
    useShellStore.setState({ navigationCollapsed: false })
  })

  it("renders the home route and navigates to settings", async () => {
    const user = userEvent.setup()

    render(<App />)

    expect(
      screen.getByRole("heading", {
        name: /keep every meeting grounded in context/i,
      }),
    ).toBeInTheDocument()

    await user.click(screen.getByRole("link", { name: "Settings" }))

    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeInTheDocument()
    expect(await screen.findByText("Local SQLite settings")).toBeInTheDocument()
    expect(screen.getByText("No external telemetry")).toBeInTheDocument()
  })

  it("collapses and expands primary navigation", async () => {
    const user = userEvent.setup()

    render(<App />)

    await user.click(
      screen.getByRole("button", { name: "Collapse navigation" }),
    )
    expect(
      screen.getByRole("button", { name: "Expand navigation" }),
    ).toBeInTheDocument()
  })
})
