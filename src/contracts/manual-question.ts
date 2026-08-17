import { z } from "zod"

import { requestIdSchema } from "@/contracts/models"
import { projectIdSchema, sessionIdSchema } from "@/contracts/projects"

const segmentIdSchema = z.uuid().brand<"SegmentId">()
const byteLength = (value: string) => new TextEncoder().encode(value).length
const hasDuplicates = (values: readonly string[]) =>
  new Set(values).size !== values.length
const questionSchema = z
  .string()
  .min(1)
  .refine((value) => value.trim() === value)
  .refine((value) => byteLength(value) <= 4_096)
  .refine((value) =>
    [...value].every(
      (character) =>
        character !== "\r" &&
        (!/\p{Cc}/u.test(character) ||
          character === "\n" ||
          character === "\t"),
    ),
  )
const generatedTextSchema = (maximum: number) =>
  z
    .string()
    .min(1)
    .refine((value) => value.trim().length > 0)
    .refine((value) => value.length <= maximum)
    .refine((value) =>
      [...value].every((character) => !/\p{Cc}/u.test(character)),
    )
const costSchema = z
  .string()
  .regex(/^(?:0|[1-9]\d*)\.\d{12}$/)
  .max(48)

export const askManualQuestionRequestSchema = z.strictObject({
  requestId: requestIdSchema,
  projectId: projectIdSchema,
  sessionId: sessionIdSchema,
  selectedSegmentId: segmentIdSchema,
  question: questionSchema,
})

export const manualQuestionUsageSchema = z.strictObject({
  inputTokens: z.number().int().nonnegative().max(4_294_967_295),
  outputTokens: z.number().int().nonnegative().max(4_294_967_295),
  actualCostUsd: costSchema,
})

export const manualQuestionResponseSchema = z
  .strictObject({
    schemaVersion: z.literal(1),
    requestId: requestIdSchema,
    projectId: projectIdSchema,
    sessionId: sessionIdSchema,
    selectedSegmentId: segmentIdSchema,
    answer: generatedTextSchema(8_000),
    relatedSegmentIds: z
      .array(segmentIdSchema)
      .max(16)
      .refine((values) => !hasDuplicates(values)),
    limitations: z
      .array(generatedTextSchema(1_000))
      .max(16)
      .refine((values) => !hasDuplicates(values)),
    primaryAttempts: z.number().int().min(1).max(3),
    repaired: z.boolean(),
    primaryUsage: manualQuestionUsageSchema,
    repairUsage: manualQuestionUsageSchema.optional(),
    sessionActualCostUsd: costSchema,
    availableBudgetUsd: costSchema,
  })
  .refine((value) => value.repaired === (value.repairUsage !== undefined))

export const manualQuestionFixtureSchema = z.strictObject({
  request: askManualQuestionRequestSchema,
  response: manualQuestionResponseSchema,
})

export type AskManualQuestionRequest = z.infer<
  typeof askManualQuestionRequestSchema
>
export type ManualQuestionResponse = z.infer<
  typeof manualQuestionResponseSchema
>
