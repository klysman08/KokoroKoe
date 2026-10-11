import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import commandErrorFixture from "../../../fixtures/contracts/command-error-v1.json"
import presetFixture from "../../../fixtures/contracts/preset-catalog-v1.json"

import {
  createPreset,
  deleteCustomPreset,
  duplicatePreset,
  exportPreset,
  listPresets,
  updatePreset,
} from "@/lib/tauri/presets"

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))

const invokeMock = vi.mocked(invoke)
const projectId = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
const customFixture = {
  ...presetFixture,
  id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
  builtIn: false,
  projectId,
}
const createInput = {
  name: "Weekly sync",
  description: "Mine.",
  assistantRole: "Keep decisions visible.",
  analysisObjectives: ["Track decisions"],
  insightTypes: ["decision" as const],
  responseTone: "Concise.",
  finalSummarySections: ["Decisions"],
  highlightInstructions: [],
  prohibitedBehaviors: [],
}

describe("preset library commands", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("lists the built-in catalog without a project scope", async () => {
    invokeMock.mockResolvedValue([presetFixture])

    await expect(listPresets()).resolves.toEqual([presetFixture])
    expect(invokeMock).toHaveBeenCalledWith("list_presets", undefined)
  })

  it("lists a project's library with its scope", async () => {
    invokeMock.mockResolvedValue([presetFixture, customFixture])

    await expect(listPresets(projectId)).resolves.toEqual([
      presetFixture,
      customFixture,
    ])
    expect(invokeMock).toHaveBeenCalledWith("list_presets", { projectId })
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

  it("refuses a preset that claims to be built in and owned by a project", async () => {
    invokeMock.mockResolvedValue([{ ...customFixture, builtIn: true }])

    await expect(listPresets(projectId)).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })

  it("normalizes a command rejection", async () => {
    invokeMock.mockRejectedValue(commandErrorFixture)

    await expect(listPresets()).rejects.toMatchObject({
      details: { code: commandErrorFixture.error.code },
    })
  })

  it("creates a preset inside the project that owns it", async () => {
    invokeMock.mockResolvedValue(customFixture)

    await expect(createPreset(projectId, createInput)).resolves.toEqual(
      customFixture,
    )
    expect(invokeMock).toHaveBeenCalledWith("create_preset", {
      projectId,
      value: createInput,
    })
  })

  it("sends an edit with the revision it was shown", async () => {
    invokeMock.mockResolvedValue({ ...customFixture, revision: 2 })

    await expect(
      updatePreset(projectId, customFixture.id, 1, { responseTone: "Warmer." }),
    ).resolves.toMatchObject({ revision: 2 })
    expect(invokeMock).toHaveBeenCalledWith("update_preset", {
      projectId,
      presetId: customFixture.id,
      expectedRevision: 1,
      value: { responseTone: "Warmer." },
    })
  })

  it("refuses an empty edit before invoking Rust", async () => {
    await expect(
      updatePreset(projectId, customFixture.id, 1, {}),
    ).rejects.toMatchObject({ details: { code: "invalid_request_contract" } })
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it("refuses an identifier that is not a preset before invoking Rust", async () => {
    await expect(
      duplicatePreset(projectId, "not-a-uuid", "Copy"),
    ).rejects.toMatchObject({ details: { code: "invalid_request_contract" } })
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it("duplicates a preset under a new name", async () => {
    invokeMock.mockResolvedValue(customFixture)

    await expect(
      duplicatePreset(projectId, presetFixture.id, "Business meeting (copy)"),
    ).resolves.toEqual(customFixture)
    expect(invokeMock).toHaveBeenCalledWith("duplicate_preset", {
      projectId,
      presetId: presetFixture.id,
      name: "Business meeting (copy)",
    })
  })

  it("deletes a preset with the revision it was shown", async () => {
    invokeMock.mockResolvedValue(null)

    await expect(
      deleteCustomPreset(projectId, customFixture.id, 3),
    ).resolves.toBeUndefined()
    expect(invokeMock).toHaveBeenCalledWith("delete_custom_preset", {
      projectId,
      presetId: customFixture.id,
      expectedRevision: 3,
    })
  })

  it("exports without sending a path", async () => {
    invokeMock.mockResolvedValue({
      path: "C:\\Users\\Example\\Documents\\weekly-sync.md",
      files: 1,
      bytes: 512,
    })

    await expect(
      exportPreset(projectId, customFixture.id),
    ).resolves.toMatchObject({ files: 1 })
    expect(invokeMock).toHaveBeenCalledWith("export_preset", {
      projectId,
      presetId: customFixture.id,
    })
  })
})
