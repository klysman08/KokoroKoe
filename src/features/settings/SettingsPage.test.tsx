import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import { vi } from "vitest"

import commandErrorFixture from "../../../fixtures/contracts/command-error-v1.json"

import { SettingsPage } from "@/features/settings/SettingsPage"

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))

const invokeMock = vi.mocked(invoke)

describe("SettingsPage", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("shows a sanitized error and retries one retryable local failure", async () => {
    invokeMock.mockRejectedValue(commandErrorFixture)
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retryDelay: 0 } },
    })

    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /could not load the foundation settings/i,
    )
    await waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2))
    expect(screen.getByText("No persisted settings")).toBeInTheDocument()
  })
})
