import fixture from "../../fixtures/contracts/preset-catalog-v1.json"

import {
  presetListSchema,
  presetSchema,
  presetSnapshotOf,
  type Preset,
} from "@/contracts/presets"
import { presetSnapshotSchema } from "@/contracts/projects"

describe("preset catalog contracts", () => {
  it("matches the shared strict Rust fixture", () => {
    expect(presetSchema.parse(fixture)).toEqual(fixture)
    expect(presetListSchema.parse([fixture])).toEqual([fixture])
  })

  it("rejects unknown, nullable, and inconsistent values", () => {
    expect(
      presetSchema.safeParse({ ...fixture, unexpected: true }).success,
    ).toBe(false)
    expect(
      presetSchema.safeParse({ ...fixture, description: null }).success,
    ).toBe(false)
    expect(
      presetSchema.safeParse({ ...fixture, schemaVersion: 2 }).success,
    ).toBe(false)
    expect(
      presetSchema.safeParse({
        ...fixture,
        insightTypes: ["decision", "decision"],
      }).success,
    ).toBe(false)
    expect(
      presetSchema.safeParse({ ...fixture, insightTypes: ["telepathy"] })
        .success,
    ).toBe(false)
  })

  it("freezes only the fields a session records", () => {
    const preset = presetSchema.parse(fixture) as Preset
    const snapshot = presetSnapshotOf(preset)

    expect(presetSnapshotSchema.parse(snapshot)).toEqual(snapshot)
    expect(Object.keys(snapshot).sort()).toEqual(
      [
        "analysisObjectives",
        "assistantRole",
        "finalSummarySections",
        "highlightInstructions",
        "id",
        "insightTypes",
        "name",
        "prohibitedBehaviors",
        "responseTone",
        "version",
      ].sort(),
    )
    expect(snapshot).not.toHaveProperty("description")
    expect(snapshot).not.toHaveProperty("builtIn")
    expect(snapshot).not.toHaveProperty("revision")
  })
})
