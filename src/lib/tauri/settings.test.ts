import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import appSettingsFixture from "../../../fixtures/contracts/app-settings-v1.json"

import { getSettings } from "@/lib/tauri/settings"

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))

const invokeMock = vi.mocked(invoke)

describe("getSettings", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("invokes the exact command and validates its response", async () => {
    invokeMock.mockResolvedValue(appSettingsFixture)

    await expect(getSettings()).resolves.toEqual(appSettingsFixture)
    expect(invokeMock).toHaveBeenCalledWith("get_settings")
  })

  it("rejects malformed backend data without exposing it", async () => {
    invokeMock.mockResolvedValue({ secret: "secret-canary" })

    await expect(getSettings()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })
})
