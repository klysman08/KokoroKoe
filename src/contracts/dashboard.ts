import { z } from "zod"

import { projectIdSchema } from "@/contracts/projects"

const RANKING_LIMIT = 5
const RECENT_ERROR_LIMIT = 10

const countSchema = z.number().int().nonnegative().max(4_294_967_295)
const safeIntegerSchema = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER)
const timestampSchema = z.iso.datetime({ offset: true })
const modelIdSchema = z.string().min(1).max(256)
/** The ledger's exact amount: whole dollars and twelve decimals. */
const usdSchema = z.string().regex(/^(?:0|[1-9]\d{0,6})\.\d{12}$/)

/**
 * The period and project a dashboard reads. `from` is inclusive and `to`
 * exclusive; leaving both out means all time.
 */
export const dashboardMetricsQuerySchema = z.strictObject({
  from: timestampSchema.optional(),
  to: timestampSchema.optional(),
  projectId: projectIdSchema.optional(),
})

export const dashboardMetricsSchema = z.strictObject({
  schemaVersion: z.literal(1),
  sessions: countSchema,
  transcribedMs: safeIntegerSchema,
  projects: z
    .array(
      z.strictObject({
        projectId: projectIdSchema,
        sessions: countSchema.min(1),
      }),
    )
    .max(RANKING_LIMIT),
  transcriptionModels: z
    .array(
      z.strictObject({ modelId: modelIdSchema, sessions: countSchema.min(1) }),
    )
    .max(RANKING_LIMIT),
  llmModels: z
    .array(
      z.strictObject({
        modelId: modelIdSchema,
        requests: countSchema.min(1),
        inputTokens: safeIntegerSchema,
        outputTokens: safeIntegerSchema,
        actualCostUsd: usdSchema,
      }),
    )
    .max(RANKING_LIMIT),
  llmRequests: countSchema,
  inputTokens: safeIntegerSchema,
  outputTokens: safeIntegerSchema,
  actualCostUsd: usdSchema,
  insightsGenerated: safeIntegerSchema,
  /** Spending before this moment was never recorded. */
  usageRecordedSince: timestampSchema,
  disk: z.strictObject({
    workspaceBytes: safeIntegerSchema,
    trashBytes: safeIntegerSchema,
    modelBytes: safeIntegerSchema,
    complete: z.boolean(),
  }),
  /**
   * The sanitized envelope of each recent failure. The technical detail is
   * deliberately absent: it is the one field allowed to name what went wrong.
   */
  recentErrors: z
    .array(
      z.strictObject({
        code: z.string().min(1).max(128),
        userMessage: z.string().min(1).max(512),
        severity: z.enum(["warning", "error", "critical"]),
        retryable: z.boolean(),
        correlationId: z.uuid(),
        occurredAt: timestampSchema,
      }),
    )
    .max(RECENT_ERROR_LIMIT),
})

export type DashboardMetricsQuery = z.infer<typeof dashboardMetricsQuerySchema>
export type DashboardMetrics = z.infer<typeof dashboardMetricsSchema>
