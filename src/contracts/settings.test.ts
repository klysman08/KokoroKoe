import appSettingsFixture from "../../fixtures/contracts/app-settings-v1.json"
import versionedUpdateFixture from "../../fixtures/contracts/versioned-app-settings-update-v1.json"
import workspaceStatusFixture from "../../fixtures/contracts/workspace-status-v1.json"

import {
  appSettingsSchema,
  versionedAppSettingsUpdateSchema,
  workspaceStatusSchema,
} from "@/contracts/settings"

describe("AppSettings contract", () => {
  it("parses the shared Rust and TypeScript golden fixture", () => {
    expect(appSettingsSchema.parse(appSettingsFixture)).toEqual(
      appSettingsFixture,
    )
  })

  it("rejects unknown fields at the transport boundary", () => {
    expect(() =>
      appSettingsSchema.parse({ ...appSettingsFixture, unexpected: true }),
    ).toThrow()
  })

  it.each([
    ["revision", Number.MAX_SAFE_INTEGER + 1],
    ["workspacePath", ""],
    ["defaultPresetId", "not-a-uuid"],
    ["defaultTranscriptionModelId", ""],
    ["defaultTranscriptionModelId", "😀".repeat(65)],
    ["maxTokensPerRequest", 0],
    ["defaultSessionBudgetUsd", "1.0"],
  ])("rejects an invalid %s", (field, invalidValue) => {
    expect(() =>
      appSettingsSchema.parse({
        ...appSettingsFixture,
        [field]: invalidValue,
      }),
    ).toThrow()
  })

  it("parses the shared versioned settings mutation fixture", () => {
    expect(
      versionedAppSettingsUpdateSchema.parse(versionedUpdateFixture),
    ).toEqual(versionedUpdateFixture)
  })

  it.each([
    [{ expectedRevision: 0, value: {} }],
    [{ expectedRevision: 0, value: { llmEnabled: null } }],
    [{ expectedRevision: 0, value: { workspacePath: "C:\\unsafe" } }],
    [
      {
        expectedRevision: Number.MAX_SAFE_INTEGER + 1,
        value: { llmEnabled: false },
      },
    ],
    [{ expectedRevision: 0, value: { maxTokensPerRequest: 0 } }],
    [{ expectedRevision: 0, value: { defaultSessionBudgetUsd: "1.0" } }],
    [
      {
        expectedRevision: 0,
        value: { defaultSessionBudgetUsd: `${"1".repeat(16)}.00` },
      },
    ],
  ])("rejects an invalid versioned settings mutation", (value) => {
    expect(() => versionedAppSettingsUpdateSchema.parse(value)).toThrow()
  })

  it("parses the shared workspace status fixture", () => {
    expect(workspaceStatusSchema.parse(workspaceStatusFixture)).toEqual(
      workspaceStatusFixture,
    )
  })

  it.each([
    [{ ...workspaceStatusFixture, unexpected: true }],
    [{ ...workspaceStatusFixture, path: "" }],
    [{ ...workspaceStatusFixture, freeBytes: Number.MAX_SAFE_INTEGER + 1 }],
    [{ ...workspaceStatusFixture, warning: null }],
  ])("rejects an invalid workspace status", (value) => {
    expect(() => workspaceStatusSchema.parse(value)).toThrow()
  })
})
