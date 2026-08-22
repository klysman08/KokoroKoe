import { z } from "zod"

import { requestIdSchema } from "@/contracts/models"
import { projectIdSchema, sessionIdSchema } from "@/contracts/projects"

const costSchema = z
  .string()
  .regex(/^(?:0|[1-9]\d*)\.\d{12}$/)
  .max(48)
const timestampSchema = z.iso.datetime({ offset: true })
const modelIdSchema = z
  .string()
  .min(1)
  .max(256)
  .refine((value) => value.trim().length > 0)
  .refine((value) =>
    [...value].every((character) => !/\p{Cc}/u.test(character)),
  )

/** The portable `summary.md` body. Rendered inertly; never parsed. */
const summaryMarkdownSchema = z
  .string()
  .min(1)
  .max(1_048_576)
  .refine((value) => value.trim().length > 0)
  .refine((value) => !value.includes("\r"))
  .refine((value) =>
    [...value].every(
      (character) => !/\p{Cc}/u.test(character) || character === "\n",
    ),
  )

export const generateSessionSummaryRequestSchema = z.strictObject({
  requestId: requestIdSchema,
  projectId: projectIdSchema,
  sessionId: sessionIdSchema,
})

export const getSessionSummaryRequestSchema = z.strictObject({
  projectId: projectIdSchema,
  sessionId: sessionIdSchema,
})

export const summaryUsageSchema = z.strictObject({
  inputTokens: z.number().int().nonnegative().max(4_294_967_295),
  outputTokens: z.number().int().nonnegative().max(4_294_967_295),
  actualCostUsd: costSchema,
})

export const sessionSummaryDocumentSchema = z
  .strictObject({
    schemaVersion: z.literal(1),
    projectId: projectIdSchema,
    sessionId: sessionIdSchema,
    generatedAt: timestampSchema,
    modelId: modelIdSchema,
    markdown: summaryMarkdownSchema,
    segmentsConsidered: z.number().int().nonnegative().max(4_294_967_295),
    segmentsIncluded: z.number().int().nonnegative().max(4_294_967_295),
  })
  .refine((value) => value.segmentsIncluded <= value.segmentsConsidered)

export const sessionSummaryStatusSchema = z
  .strictObject({
    schemaVersion: z.literal(1),
    projectId: projectIdSchema,
    sessionId: sessionIdSchema,
    document: sessionSummaryDocumentSchema.optional(),
  })
  .refine(
    (value) =>
      value.document === undefined ||
      (value.document.projectId === value.projectId &&
        value.document.sessionId === value.sessionId),
  )

export const generateSessionSummaryResponseSchema = z
  .strictObject({
    schemaVersion: z.literal(1),
    requestId: requestIdSchema,
    document: sessionSummaryDocumentSchema,
    primaryAttempts: z.number().int().min(1).max(3),
    repaired: z.boolean(),
    primaryUsage: summaryUsageSchema,
    repairUsage: summaryUsageSchema.optional(),
    sessionActualCostUsd: costSchema,
    availableBudgetUsd: costSchema,
  })
  .refine((value) => value.repaired === (value.repairUsage !== undefined))

export const sessionSummaryFixtureSchema = z.strictObject({
  generateRequest: generateSessionSummaryRequestSchema,
  generateResponse: generateSessionSummaryResponseSchema,
  status: sessionSummaryStatusSchema,
  absentStatus: sessionSummaryStatusSchema,
})

export type GenerateSessionSummaryRequest = z.infer<
  typeof generateSessionSummaryRequestSchema
>
export type GetSessionSummaryRequest = z.infer<
  typeof getSessionSummaryRequestSchema
>
export type SessionSummaryDocument = z.infer<
  typeof sessionSummaryDocumentSchema
>
export type SessionSummaryStatus = z.infer<typeof sessionSummaryStatusSchema>
export type GenerateSessionSummaryResponse = z.infer<
  typeof generateSessionSummaryResponseSchema
>
