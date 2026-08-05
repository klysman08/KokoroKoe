import appSettingsFixture from "../../fixtures/contracts/app-settings-v1.json"

import { appSettingsSchema } from "@/contracts/settings"

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
})
