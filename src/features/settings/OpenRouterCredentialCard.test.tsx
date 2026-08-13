import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import modelFixture from "../../../fixtures/contracts/openrouter-model-v1.json"
import validationFixture from "../../../fixtures/contracts/openrouter-validation-v1.json"

import {
  credentialValidationSchema,
  openRouterModelSchema,
} from "@/contracts/openrouter"
import { OpenRouterCredentialCard } from "@/features/settings/OpenRouterCredentialCard"
import {
  deleteOpenRouterApiKey,
  getOpenRouterCredentialStatus,
  setOpenRouterApiKey,
} from "@/lib/tauri/credentials"
import {
  listOpenRouterModels,
  validateOpenRouterApiKey,
} from "@/lib/tauri/openrouter"

vi.mock("@/lib/tauri/credentials", () => ({
  deleteOpenRouterApiKey: vi.fn(),
  getOpenRouterCredentialStatus: vi.fn(),
  setOpenRouterApiKey: vi.fn(),
}))
vi.mock("@/lib/tauri/openrouter", () => ({
  listOpenRouterModels: vi.fn(),
  validateOpenRouterApiKey: vi.fn(),
}))
const deleteMock = vi.mocked(deleteOpenRouterApiKey)
const statusMock = vi.mocked(getOpenRouterCredentialStatus)
const setMock = vi.mocked(setOpenRouterApiKey)
const listModelsMock = vi.mocked(listOpenRouterModels)
const validateMock = vi.mocked(validateOpenRouterApiKey)
const validation = credentialValidationSchema.parse(validationFixture)
const model = openRouterModelSchema.parse(modelFixture)

function renderCard() {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <OpenRouterCredentialCard />
    </QueryClientProvider>,
  )
}

describe("OpenRouterCredentialCard", () => {
  beforeEach(() => {
    vi.resetAllMocks()
    statusMock.mockResolvedValue({ configured: true })
  })

  it("keeps the key local to the field, clears it after set, and removes idempotently", async () => {
    const user = userEvent.setup()
    setMock.mockResolvedValue({ configured: true })
    deleteMock.mockResolvedValue({ configured: false })
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
    expect(setMock).toHaveBeenCalledWith(canary)
    expect(document.body.textContent).not.toContain(canary)
    await user.click(screen.getByRole("button", { name: "Remove credential" }))
    expect(await screen.findByText("Not configured")).toBeInTheDocument()
    expect(deleteMock).toHaveBeenCalledOnce()
  })

  it("validates the stored key and shows the bounded ZDR model catalog", async () => {
    const user = userEvent.setup()
    validateMock.mockResolvedValue(validation)
    listModelsMock.mockResolvedValue([model])
    renderCard()

    await user.click(
      await screen.findByRole("button", { name: "Validate credential" }),
    )
    expect(await screen.findByText("Validated")).toBeInTheDocument()
    expect(screen.getByText(validationFixture.validatedAt)).toBeInTheDocument()
    expect(await screen.findByText(modelFixture.name)).toBeInTheDocument()
    expect(screen.getByText("ZDR")).toBeInTheDocument()
    expect(screen.getByText("Structured")).toBeInTheDocument()
    expect(validateMock).toHaveBeenCalledOnce()
    expect(listModelsMock).toHaveBeenCalledWith(false)

    await user.click(screen.getByRole("button", { name: "Refresh models" }))
    await waitFor(() => expect(listModelsMock).toHaveBeenCalledWith(true))
  })

  it("renders provider model metadata only as inert text", async () => {
    const hostileName = '<img src="https://invalid.example/pixel" onerror="x">'
    statusMock.mockResolvedValue({
      configured: true,
      validatedAt: validation.validatedAt,
    })
    listModelsMock.mockResolvedValue([{ ...model, name: hostileName }])
    renderCard()

    expect(await screen.findByText(hostileName)).toBeInTheDocument()
    expect(document.querySelector("img")).toBeNull()
  })
})
