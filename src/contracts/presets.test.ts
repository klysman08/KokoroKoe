import fixture from "../../fixtures/contracts/preset-catalog-v1.json"

import {
  createPresetInputSchema,
  presetExportSchema,
  presetListSchema,
  presetSchema,
  presetSnapshotOf,
  updatePresetInputSchema,
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

  /// A built-in belongs to the binary and a custom preset to a project, so the
  /// two facts have to agree in every direction.
  it("ties the built-in flag to the owning project", () => {
    const projectId = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"

    expect(
      presetSchema.safeParse({ ...fixture, builtIn: false, projectId }).success,
    ).toBe(true)
    expect(
      presetSchema.safeParse({ ...fixture, builtIn: true, projectId }).success,
    ).toBe(false)
    expect(presetSchema.safeParse({ ...fixture, builtIn: false }).success).toBe(
      false,
    )
  })

  it("accepts a written preset and refuses an empty edit", () => {
    const input = {
      name: "Weekly sync",
      description: "",
      assistantRole: "Keep decisions visible.",
      analysisObjectives: ["Track decisions"],
      insightTypes: ["decision"],
      responseTone: "Concise.",
      finalSummarySections: ["Decisions"],
      highlightInstructions: [],
      prohibitedBehaviors: [],
    }

    expect(createPresetInputSchema.parse(input)).toEqual(input)
    expect(
      createPresetInputSchema.safeParse({ ...input, name: "" }).success,
    ).toBe(false)
    expect(
      createPresetInputSchema.safeParse({ ...input, insightTypes: [] }).success,
    ).toBe(false)
    expect(
      createPresetInputSchema.safeParse({ ...input, unexpected: true }).success,
    ).toBe(false)

    expect(updatePresetInputSchema.safeParse({}).success).toBe(false)
    expect(
      updatePresetInputSchema.safeParse({ responseTone: "Warmer." }).success,
    ).toBe(true)
  })

  it("accepts only a bounded export receipt", () => {
    const receipt = {
      path: "C:\\Users\\Example\\weekly-sync.md",
      files: 1,
      bytes: 512,
    }

    expect(presetExportSchema.parse(receipt)).toEqual(receipt)
    expect(presetExportSchema.safeParse({ ...receipt, path: "" }).success).toBe(
      false,
    )
    expect(presetExportSchema.safeParse({ ...receipt, files: 0 }).success).toBe(
      false,
    )
  })
})
