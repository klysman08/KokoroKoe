import { z } from "zod"

export const presetIdSchema = z.uuid().brand<"PresetId">()

export const appSettingsSchema = z
  .object({
    revision: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
    workspacePath: z.string().min(1).max(32_767),
    defaultPresetId: presetIdSchema,
    defaultTranscriptionModelId: z.string().min(1).max(128),
    llmEnabled: z.boolean(),
    retainAudioByDefault: z.boolean(),
    requireZeroDataRetention: z.boolean(),
    denyProviderDataCollection: z.boolean(),
    maxTokensPerRequest: z.number().int().min(1).max(1_000_000),
    defaultSessionBudgetUsd: z.string().regex(/^\d+(?:\.\d{2})$/),
  })
  .strict()

export type AppSettings = z.infer<typeof appSettingsSchema>
