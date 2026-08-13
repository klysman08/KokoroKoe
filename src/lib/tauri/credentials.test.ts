import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import fixture from "../../../fixtures/contracts/credential-status-v1.json"

import {
  deleteOpenRouterApiKey,
  getOpenRouterCredentialStatus,
  setOpenRouterApiKey,
} from "@/lib/tauri/credentials"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
const invokeMock = vi.mocked(invoke)

describe("credential adapters", () => {
  beforeEach(() => invokeMock.mockReset())

  it("uses the three exact commands and returns status only", async () => {
    invokeMock.mockResolvedValue(fixture)
    await expect(getOpenRouterCredentialStatus()).resolves.toEqual(fixture)
    await expect(
      setOpenRouterApiKey("secret-canary-adapter-1234"),
    ).resolves.toEqual(fixture)
    await expect(deleteOpenRouterApiKey()).resolves.toEqual(fixture)
    expect(invokeMock.mock.calls).toEqual([
      ["get_openrouter_credential_status", undefined],
      ["set_openrouter_api_key", { apiKey: "secret-canary-adapter-1234" }],
      ["delete_openrouter_api_key", undefined],
    ])
  })

  it("rejects malformed requests and responses", async () => {
    await expect(setOpenRouterApiKey("short")).rejects.toMatchObject({
      details: { code: "invalid_request_contract" },
    })
    expect(invokeMock).not.toHaveBeenCalled()
    invokeMock.mockResolvedValue({ configured: true, apiKey: "leaked" })
    await expect(getOpenRouterCredentialStatus()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })
})
