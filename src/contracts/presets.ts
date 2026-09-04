import { z } from "zod"

import { presetSnapshotSchema } from "@/contracts/projects"

const timestamp = z.iso.datetime({ offset: true })

// A built-in preset as the Rust-owned library publishes it: the snapshot a
// Session freezes, plus the metadata that only matters while choosing one.
export const presetSchema = z
  .strictObject({
    ...presetSnapshotSchema.shape,
    schemaVersion: z.literal(1),
    description: z.string().min(1).max(1024),
    builtIn: z.boolean(),
    createdAt: timestamp,
    updatedAt: timestamp,
    revision: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
  })
  .refine(
    (value) => new Set(value.insightTypes).size === value.insightTypes.length,
  )

export const presetListSchema = z.array(presetSchema).min(1).max(64)

export type Preset = z.infer<typeof presetSchema>

// The frozen copy a Session keeps, taken from a chosen preset.
//
// Choosing a preset is a decision about a Session, not a link to the library:
// the library can move on afterwards, and the Session must keep reading the
// way it did when it started.
export function presetSnapshotOf(preset: Preset) {
  return presetSnapshotSchema.parse({
    id: preset.id,
    version: preset.version,
    name: preset.name,
    assistantRole: preset.assistantRole,
    analysisObjectives: preset.analysisObjectives,
    insightTypes: preset.insightTypes,
    responseTone: preset.responseTone,
    finalSummarySections: preset.finalSummarySections,
    highlightInstructions: preset.highlightInstructions,
    prohibitedBehaviors: preset.prohibitedBehaviors,
  })
}
