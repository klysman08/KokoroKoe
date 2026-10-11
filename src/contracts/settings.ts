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
const languageTagSchema = z
  .string()
  .max(64)
  .regex(/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8}){0,7}$/)
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
    // The spoken language Home offers next, remembered from the last meeting.
    defaultSessionLanguage: languageTagSchema.optional(),
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
    defaultSessionLanguage: languageTagSchema.optional(),
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

/**
 * What a workspace rescan found.
 *
 * `issues` is a count rather than a list on purpose: a discovery issue names a
 * folder that could not be parsed, and those names are the user's own meeting
 * content. The number says "something here needs looking at" without carrying
 * meeting material into a settings panel.
 */
export const workspaceRescanReportSchema = z.strictObject({
  schemaVersion: z.literal(1),
  projects: z.number().int().nonnegative().max(4_294_967_295),
  sessions: z.number().int().nonnegative().max(4_294_967_295),
  transcripts: z.number().int().nonnegative().max(4_294_967_295),
  segments: z.number().int().nonnegative().max(4_294_967_295),
  issues: z.number().int().nonnegative().max(4_294_967_295),
})

export type WorkspaceRescanReport = z.infer<typeof workspaceRescanReportSchema>
