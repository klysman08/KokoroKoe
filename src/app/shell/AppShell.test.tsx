import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import projectSessionFixture from "../../../fixtures/contracts/project-session-v1.json"

import { AppShell } from "@/app/shell/AppShell"
import { sessionSchema } from "@/contracts/projects"
import { useActiveSessionStore } from "@/stores/active-session-store"
import { useNavigationStore } from "@/stores/navigation-store"
import { useShellStore } from "@/stores/shell-store"

vi.mock("@/features/home/HomePage", () => ({
  HomePage: () => <p>Home content</p>,
}))
vi.mock("@/features/settings/SettingsPage", () => ({
  SettingsPage: () => <p>Settings content</p>,
}))
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)

function renderShell() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <AppShell />
    </QueryClientProvider>,
  )
}

describe("AppShell session and appearance controls", () => {
  beforeEach(() => {
    globalThis.localStorage.clear()
    document.documentElement.classList.remove("dark")
    document.documentElement.style.colorScheme = ""
    useShellStore.setState({ navigationCollapsed: false, theme: "light" })
    useNavigationStore.getState().navigate("home")
    useActiveSessionStore.setState({ activeSession: undefined })
    invokeMock.mockReset()
  })

  it("shows pause and stop controls for the active transcribing session", () => {
    useActiveSessionStore
      .getState()
      .syncSession(sessionSchema.parse(projectSessionFixture.session))

    renderShell()

    expect(
      screen.getByRole("region", { name: "Active session" }),
    ).toHaveTextContent("Sprint planning")
    expect(screen.getByRole("button", { name: "Pause" })).toBeEnabled()
    expect(screen.getByRole("button", { name: "Stop" })).toBeEnabled()
  })

  it("switches and persists the semantic dark theme", async () => {
    const user = userEvent.setup()
    renderShell()

    await user.click(
      screen.getByRole("button", { name: "Switch to dark mode" }),
    )

    expect(document.documentElement).toHaveClass("dark")
    expect(document.documentElement.style.colorScheme).toBe("dark")
    expect(globalThis.localStorage.getItem("kokorokoe-theme")).toBe("dark")
    expect(
      screen.getByRole("button", { name: "Switch to light mode" }),
    ).toBeInTheDocument()
  })

  it("credits the author and opens the repository through Rust", async () => {
    invokeMock.mockResolvedValue(undefined)
    const user = userEvent.setup()
    renderShell()

    const repository = screen.getByRole("link", {
      name: "KokoroKoe on GitHub",
    })
    expect(repository).toHaveAttribute(
      "href",
      "https://github.com/klysman08/KokoroKoe",
    )
    expect(screen.getByText(/Made by/)).toHaveTextContent(
      `Made by klysman08 · v${__APP_VERSION__}`,
    )

    await user.click(repository)
    await user.click(screen.getByRole("link", { name: "klysman08" }))

    expect(invokeMock).toHaveBeenCalledWith("open_external_page", {
      page: "repository",
    })
    expect(invokeMock).toHaveBeenCalledWith("open_external_page", {
      page: "author",
    })
  })

  it("names the current page in the header", () => {
    useNavigationStore.getState().navigate("settings")
    renderShell()

    expect(screen.getByRole("banner")).toHaveTextContent("Settings")
    expect(screen.queryByText("Foundation checkpoint")).not.toBeInTheDocument()
  })
})
