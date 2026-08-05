import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"

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
        name: /meetings stay understandable/i,
      }),
    ).toBeInTheDocument()

    await user.click(screen.getByRole("link", { name: "Settings" }))

    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeInTheDocument()
    expect(screen.getByText("No persisted settings")).toBeInTheDocument()
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
