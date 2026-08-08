import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import appSettingsFixture from "../../../fixtures/contracts/app-settings-v1.json"
import commandErrorFixture from "../../../fixtures/contracts/command-error-v1.json"
import versionedUpdateFixture from "../../../fixtures/contracts/versioned-app-settings-update-v1.json"
import workspaceStatusFixture from "../../../fixtures/contracts/workspace-status-v1.json"

import { versionedAppSettingsUpdateSchema } from "@/contracts/settings"
import {
  chooseWorkspace,
  getSettings,
  updateSettings,
} from "@/lib/tauri/settings"

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))

const invokeMock = vi.mocked(invoke)
const validUpdate = versionedAppSettingsUpdateSchema.parse(
  versionedUpdateFixture,
)

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

  it("validates and invokes the exact optimistic update shape", async () => {
    invokeMock.mockResolvedValue({ ...appSettingsFixture, revision: 1 })

    await expect(updateSettings(validUpdate)).resolves.toMatchObject({
      revision: 1,
    })
    expect(invokeMock).toHaveBeenCalledWith("update_settings", validUpdate)
  })

  it("rejects an invalid update before invoking Rust", async () => {
    await expect(
      updateSettings({ expectedRevision: 0, value: {} }),
    ).rejects.toMatchObject({ details: { code: "invalid_request_contract" } })
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it("normalizes a settings update command rejection", async () => {
    invokeMock.mockRejectedValue(commandErrorFixture)

    await expect(updateSettings(validUpdate)).rejects.toMatchObject({
      details: { code: commandErrorFixture.error.code },
    })
  })

  it("invokes workspace selection without passing a frontend path", async () => {
    invokeMock.mockResolvedValue(workspaceStatusFixture)

    await expect(chooseWorkspace()).resolves.toEqual(workspaceStatusFixture)
    expect(invokeMock).toHaveBeenCalledWith("choose_workspace")
  })

  it("rejects malformed workspace status without exposing it", async () => {
    invokeMock.mockResolvedValue({ path: "secret-canary" })

    await expect(chooseWorkspace()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })
})
