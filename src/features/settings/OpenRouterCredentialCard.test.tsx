import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import { OpenRouterCredentialCard } from "@/features/settings/OpenRouterCredentialCard"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
const invokeMock = vi.mocked(invoke)

function renderCard() {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <OpenRouterCredentialCard />
    </QueryClientProvider>,
  )
}

describe("OpenRouterCredentialCard", () => {
  beforeEach(() => invokeMock.mockReset())

  it("keeps the key local to the field, clears it after set, and removes idempotently", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => ({
      configured: command !== "delete_openrouter_api_key",
    }))
    renderCard()
    expect(await screen.findByText("Configured")).toBeInTheDocument()
    const input = screen.getByLabelText("API key")
    const canary = "secret-canary-ui-credential-1234"
    await user.type(input, canary)
    expect(input).toHaveAttribute("type", "password")
    await user.click(screen.getByRole("button", { name: "Save credential" }))
    await waitFor(() => expect(input).toHaveValue(""))
    expect(
      await screen.findByText("Credential saved securely."),
    ).toBeInTheDocument()
    expect(invokeMock).toHaveBeenCalledWith("set_openrouter_api_key", {
      apiKey: canary,
    })
    expect(document.body.textContent).not.toContain(canary)
    await user.click(screen.getByRole("button", { name: "Remove credential" }))
    expect(await screen.findByText("Not configured")).toBeInTheDocument()
    expect(invokeMock).toHaveBeenCalledWith(
      "delete_openrouter_api_key",
      undefined,
    )
  })
})
