import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import commandErrorFixture from "../../../fixtures/contracts/command-error-v1.json"
import presetFixture from "../../../fixtures/contracts/preset-catalog-v1.json"

import { listPresets } from "@/lib/tauri/presets"

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))

const invokeMock = vi.mocked(invoke)

describe("listPresets", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("invokes the exact command without arguments", async () => {
    invokeMock.mockResolvedValue([presetFixture])

    await expect(listPresets()).resolves.toEqual([presetFixture])
    expect(invokeMock).toHaveBeenCalledWith("list_presets")
  })

  it("rejects malformed backend data without exposing it", async () => {
    invokeMock.mockResolvedValue([{ name: "secret-canary" }])

    await expect(listPresets()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })

  it("rejects an empty library rather than showing no choice", async () => {
    invokeMock.mockResolvedValue([])

    await expect(listPresets()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })

  it("normalizes a command rejection", async () => {
    invokeMock.mockRejectedValue(commandErrorFixture)

    await expect(listPresets()).rejects.toMatchObject({
      details: { code: commandErrorFixture.error.code },
    })
  })
})
