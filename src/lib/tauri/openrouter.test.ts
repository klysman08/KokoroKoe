import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import modelFixture from "../../../fixtures/contracts/openrouter-model-v1.json"
import validationFixture from "../../../fixtures/contracts/openrouter-validation-v1.json"

import { requestIdSchema } from "@/contracts/models"
import {
  listOpenRouterModels,
  validateOpenRouterApiKey,
} from "@/lib/tauri/openrouter"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
const invokeMock = vi.mocked(invoke)

describe("OpenRouter product adapters", () => {
  beforeEach(() => invokeMock.mockReset())

  it("uses exact commands, fresh request IDs, and strict responses", async () => {
    invokeMock
      .mockResolvedValueOnce(validationFixture)
      .mockResolvedValueOnce([modelFixture])
    await expect(validateOpenRouterApiKey()).resolves.toEqual(validationFixture)
    await expect(listOpenRouterModels(true)).resolves.toEqual([modelFixture])

    const validationCall = invokeMock.mock.calls[0]!
    const catalogCall = invokeMock.mock.calls[1]!
    const validationArgs = validationCall[1] as Record<string, unknown>
    const catalogArgs = catalogCall[1] as Record<string, unknown>
    expect(validationCall[0]).toBe("validate_openrouter_api_key")
    expect(catalogCall[0]).toBe("list_openrouter_models")
    expect(requestIdSchema.safeParse(validationArgs.requestId).success).toBe(
      true,
    )
    expect(requestIdSchema.safeParse(catalogArgs.requestId).success).toBe(true)
    expect(catalogArgs).toMatchObject({ forceRefresh: true })
    expect(validationArgs.requestId).not.toBe(catalogArgs.requestId)
  })

  it("rejects malformed backend responses", async () => {
    invokeMock.mockResolvedValueOnce({
      ...validationFixture,
      apiKey: "leaked",
    })
    await expect(validateOpenRouterApiKey()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
    invokeMock.mockResolvedValueOnce([
      { ...modelFixture, dataCollection: "allow" },
    ])
    await expect(listOpenRouterModels(false)).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })
})
