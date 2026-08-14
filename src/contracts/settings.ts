import { z } from "zod"

export const presetIdSchema = z.uuid().brand<"PresetId">()

const revisionSchema = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER)
const workspacePathSchema = z.string().min(1).max(32_767)
const modelIdSchema = z.string().min(1).max(128)
const openRouterModelIdSchema = z.string().min(1).max(256)
const defaultLlmModelsSchema = z
  .object({
    insights: openRouterModelIdSchema.optional(),
    summaries: openRouterModelIdSchema.optional(),
    manualQuestions: openRouterModelIdSchema.optional(),
  })
  .strict()
const maxTokensSchema = z.number().int().min(1).max(1_000_000)
const budgetSchema = z
  .string()
  .max(18)
  .regex(/^\d+(?:\.\d{2})$/)

export const appSettingsSchema = z
  .object({
    revision: revisionSchema,
    workspacePath: workspacePathSchema,
    defaultPresetId: presetIdSchema,
    defaultTranscriptionModelId: modelIdSchema,
    defaultLlmModels: defaultLlmModelsSchema,
    llmEnabled: z.boolean(),
    retainAudioByDefault: z.boolean(),
    requireZeroDataRetention: z.boolean(),
    denyProviderDataCollection: z.boolean(),
    maxTokensPerRequest: maxTokensSchema,
    defaultSessionBudgetUsd: budgetSchema,
  })
  .strict()

export type AppSettings = z.infer<typeof appSettingsSchema>

export const appSettingsUpdateSchema = z
  .object({
    defaultPresetId: presetIdSchema.optional(),
    defaultTranscriptionModelId: modelIdSchema.optional(),
    defaultLlmModels: defaultLlmModelsSchema.optional(),
    llmEnabled: z.boolean().optional(),
    retainAudioByDefault: z.boolean().optional(),
    requireZeroDataRetention: z.boolean().optional(),
    denyProviderDataCollection: z.boolean().optional(),
    maxTokensPerRequest: maxTokensSchema.optional(),
    defaultSessionBudgetUsd: budgetSchema.optional(),
  })
  .strict()
  .refine((value) => Object.keys(value).length > 0, {
    message: "At least one setting must change.",
  })

export const versionedAppSettingsUpdateSchema = z
  .object({
    expectedRevision: revisionSchema,
    value: appSettingsUpdateSchema,
  })
  .strict()

export const workspaceStatusSchema = z
  .object({
    path: workspacePathSchema,
    writable: z.boolean(),
    freeBytes: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
    warning: z.string().min(1).max(512).optional(),
  })
  .strict()

export type AppSettingsUpdate = z.infer<typeof appSettingsUpdateSchema>
export type VersionedAppSettingsUpdate = z.infer<
  typeof versionedAppSettingsUpdateSchema
>
export type WorkspaceStatus = z.infer<typeof workspaceStatusSchema>
