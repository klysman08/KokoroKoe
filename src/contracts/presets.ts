import { z } from "zod"

import { presetIdSchema } from "@/contracts/settings"
import { presetSnapshotSchema, projectIdSchema } from "@/contracts/projects"

const timestamp = z.iso.datetime({ offset: true })
const description = z.string().max(1024)

// A preset as the Rust-owned library publishes it: the snapshot a Session
// freezes, plus the metadata that only matters while choosing or managing one.
// A custom preset names the project that owns it; a built-in names none.
export const presetSchema = z
  .strictObject({
    ...presetSnapshotSchema.shape,
    schemaVersion: z.literal(1),
    description,
    builtIn: z.boolean(),
    projectId: projectIdSchema.optional(),
    createdAt: timestamp,
    updatedAt: timestamp,
    revision: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
  })
  .refine(
    (value) => new Set(value.insightTypes).size === value.insightTypes.length,
  )
  .refine((value) => value.builtIn === (value.projectId === undefined))

export const presetListSchema = z.array(presetSchema).min(1).max(1024)

const editableFields = {
  name: presetSnapshotSchema.shape.name,
  description,
  assistantRole: presetSnapshotSchema.shape.assistantRole,
  analysisObjectives: presetSnapshotSchema.shape.analysisObjectives,
  insightTypes: presetSnapshotSchema.shape.insightTypes,
  responseTone: presetSnapshotSchema.shape.responseTone,
  finalSummarySections: presetSnapshotSchema.shape.finalSummarySections,
  highlightInstructions: presetSnapshotSchema.shape.highlightInstructions,
  prohibitedBehaviors: presetSnapshotSchema.shape.prohibitedBehaviors,
} as const

export const createPresetInputSchema = z.strictObject(editableFields)
export const updatePresetInputSchema = z
  .strictObject(editableFields)
  .partial()
  .refine((value) => Object.values(value).some((field) => field !== undefined))

export const presetExportSchema = z.strictObject({
  path: z.string().min(1).max(4096),
  files: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
  bytes: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
})

export { presetIdSchema }

export type Preset = z.infer<typeof presetSchema>
export type CreatePresetInput = z.infer<typeof createPresetInputSchema>
export type UpdatePresetInput = z.infer<typeof updatePresetInputSchema>
export type PresetExport = z.infer<typeof presetExportSchema>

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
