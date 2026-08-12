import { z } from "zod"

import { requestIdSchema } from "@/contracts/models"
import { projectIdSchema, sessionIdSchema } from "@/contracts/projects"
import {
  transcriptionFinalEnvelopeSchema,
  transcriptionGapEnvelopeSchema,
  transcriptionPartialEnvelopeSchema,
} from "@/contracts/transcription"

const safeRevision = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER)
const identity = {
  projectId: projectIdSchema,
  sessionId: sessionIdSchema,
  expectedRevision: safeRevision,
} as const

export const startPersistedSessionRequestSchema = z.strictObject({
  ...identity,
  requestId: requestIdSchema,
  acknowledgedCaptureConsent: z.literal(true),
})

export const pausePersistedSessionRequestSchema = z.strictObject(identity)

export const resumePersistedSessionRequestSchema =
  startPersistedSessionRequestSchema

export const stopPersistedSessionRequestSchema = z.strictObject(identity)

const scoped = <T extends z.ZodType>(event: T) =>
  z.strictObject({
    schemaVersion: z.literal(1),
    projectId: projectIdSchema,
    sessionId: sessionIdSchema,
    event,
  })

export const sessionTranscriptionPartialSchema = scoped(
  transcriptionPartialEnvelopeSchema,
)
export const sessionTranscriptionFinalSchema = scoped(
  transcriptionFinalEnvelopeSchema,
)
export const sessionTranscriptionGapSchema = scoped(
  transcriptionGapEnvelopeSchema,
)

export const persistenceStatusSchema = z
  .strictObject({
    schemaVersion: z.literal(1),
    projectId: projectIdSchema,
    sessionId: sessionIdSchema,
    journalSequence: safeRevision,
    snapshotSequence: safeRevision,
    state: z.enum(["clean", "deferred", "conflict", "failed"]),
    code: z
      .string()
      .min(1)
      .max(128)
      .regex(/^[a-z0-9_]+$/)
      .optional(),
  })
  .superRefine((value, context) => {
    if (
      value.snapshotSequence > value.journalSequence ||
      ((value.state === "clean" || value.state === "deferred") &&
        value.code !== undefined) ||
      ((value.state === "conflict" || value.state === "failed") &&
        value.code === undefined)
    ) {
      context.addIssue({
        code: "custom",
        message: "Invalid persistence state.",
      })
    }
  })

export const sessionLifecycleFixtureSchema = z.strictObject({
  startRequest: startPersistedSessionRequestSchema,
  pauseRequest: pausePersistedSessionRequestSchema,
  resumeRequest: resumePersistedSessionRequestSchema,
  stopRequest: stopPersistedSessionRequestSchema,
  partialEvent: sessionTranscriptionPartialSchema,
  finalEvent: sessionTranscriptionFinalSchema,
  gapEvent: sessionTranscriptionGapSchema,
  persistenceStatus: persistenceStatusSchema,
})

export type StartPersistedSessionRequest = z.infer<
  typeof startPersistedSessionRequestSchema
>
export type PausePersistedSessionRequest = z.infer<
  typeof pausePersistedSessionRequestSchema
>
export type ResumePersistedSessionRequest = z.infer<
  typeof resumePersistedSessionRequestSchema
>
export type StopPersistedSessionRequest = z.infer<
  typeof stopPersistedSessionRequestSchema
>
export type SessionTranscriptionPartial = z.infer<
  typeof sessionTranscriptionPartialSchema
>
export type SessionTranscriptionFinal = z.infer<
  typeof sessionTranscriptionFinalSchema
>
export type SessionTranscriptionGap = z.infer<
  typeof sessionTranscriptionGapSchema
>
export type PersistenceStatus = z.infer<typeof persistenceStatusSchema>
