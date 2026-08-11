import { z } from "zod"

import { appErrorSchema } from "@/contracts/app-error"
import { audioSourceSchema, deviceSelectionSchema } from "@/contracts/audio"
import { requestIdSchema } from "@/contracts/models"

const safeCounterSchema = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER)
const timestampSchema = z.iso.datetime({ offset: true })
const segmentIdSchema = z.uuid().brand<"SegmentId">()
const utf8Length = (value: string) => new TextEncoder().encode(value).length

export const liveTranscriptionInputSchema = z.strictObject({
  acknowledgedCaptureConsent: z.literal(true),
  microphoneSelection: deviceSelectionSchema,
  systemOutputSelection: deviceSelectionSchema,
})

export const liveTranscriptionStatusSchema = z
  .strictObject({
    state: z.enum([
      "idle",
      "starting",
      "running",
      "stopping",
      "stopped",
      "failed",
    ]),
    requestId: requestIdSchema.optional(),
    startedAt: timestampSchema.optional(),
    stoppedAt: timestampSchema.optional(),
    error: appErrorSchema.optional(),
  })
  .superRefine((value, context) => {
    const idle = value.state === "idle"
    const active = ["starting", "running", "stopping"].includes(value.state)
    const terminal = value.state === "stopped" || value.state === "failed"
    if (
      (idle &&
        (value.requestId !== undefined ||
          value.startedAt !== undefined ||
          value.stoppedAt !== undefined ||
          value.error !== undefined)) ||
      (active &&
        (value.requestId === undefined ||
          value.startedAt === undefined ||
          value.stoppedAt !== undefined ||
          value.error !== undefined)) ||
      (terminal &&
        (value.requestId === undefined ||
          value.startedAt === undefined ||
          value.stoppedAt === undefined)) ||
      (value.state === "stopped" && value.error !== undefined) ||
      (value.state === "failed" && value.error === undefined)
    ) {
      context.addIssue({ code: "custom", message: "Invalid live status." })
    }
  })

export const liveTranscriptSegmentSchema = z
  .strictObject({
    id: segmentIdSchema,
    source: audioSourceSchema,
    startMs: safeCounterSchema,
    endMs: safeCounterSchema,
    text: z
      .string()
      .max(1_048_576)
      .refine((value) => utf8Length(value) <= 1_048_576)
      .refine((value) => !value.includes("\0")),
    status: z.enum(["partial", "final"]),
    language: z
      .string()
      .min(1)
      .max(64)
      .refine((value) => utf8Length(value) <= 64)
      .refine(
        (value) =>
          !Array.from(value).some((character) => {
            const code = character.charCodeAt(0)
            return code <= 31 || code === 127
          }),
      ),
  })
  .superRefine((value, context) => {
    if (value.endMs <= value.startMs) {
      context.addIssue({ code: "custom", message: "Invalid segment range." })
    }
  })

const eventEnvelopeFields = {
  schemaVersion: z.literal(1),
  eventId: z.uuid(),
  emittedAt: timestampSchema,
  sessionSequence: safeCounterSchema.min(1),
  requestId: requestIdSchema,
}

export const transcriptionPartialEnvelopeSchema = z
  .strictObject({
    ...eventEnvelopeFields,
    payload: z.strictObject({ segment: liveTranscriptSegmentSchema }),
  })
  .superRefine((value, context) => {
    if (value.payload.segment.status !== "partial") {
      context.addIssue({ code: "custom", message: "Partial status required." })
    }
  })

export const transcriptionFinalEnvelopeSchema = z
  .strictObject({
    ...eventEnvelopeFields,
    payload: z.strictObject({
      segment: liveTranscriptSegmentSchema,
      replacesPartialId: segmentIdSchema.optional(),
    }),
  })
  .superRefine((value, context) => {
    if (
      value.payload.segment.status !== "final" ||
      (value.payload.replacesPartialId !== undefined &&
        value.payload.replacesPartialId !== value.payload.segment.id)
    ) {
      context.addIssue({
        code: "custom",
        message: "Invalid final replacement.",
      })
    }
  })

export const transcriptionGapEnvelopeSchema = z.strictObject({
  ...eventEnvelopeFields,
  payload: z
    .strictObject({
      source: audioSourceSchema,
      startMs: safeCounterSchema,
      endMs: safeCounterSchema,
      code: z
        .string()
        .min(1)
        .max(128)
        .regex(/^[a-z0-9_]+$/),
    })
    .superRefine((value, context) => {
      if (value.endMs <= value.startMs) {
        context.addIssue({ code: "custom", message: "Invalid gap range." })
      }
    }),
})

export const liveTranscriptionFixtureSchema = z.strictObject({
  input: liveTranscriptionInputSchema,
  idleStatus: liveTranscriptionStatusSchema,
  runningStatus: liveTranscriptionStatusSchema,
  stoppedStatus: liveTranscriptionStatusSchema,
  partialEvent: transcriptionPartialEnvelopeSchema,
  finalEvent: transcriptionFinalEnvelopeSchema,
  gapEvent: transcriptionGapEnvelopeSchema,
})

export type LiveTranscriptionInput = z.infer<
  typeof liveTranscriptionInputSchema
>
export type LiveTranscriptionStatus = z.infer<
  typeof liveTranscriptionStatusSchema
>
export type LiveTranscriptSegment = z.infer<typeof liveTranscriptSegmentSchema>
export type TranscriptionPartialEnvelope = z.infer<
  typeof transcriptionPartialEnvelopeSchema
>
export type TranscriptionFinalEnvelope = z.infer<
  typeof transcriptionFinalEnvelopeSchema
>
export type TranscriptionGapEnvelope = z.infer<
  typeof transcriptionGapEnvelopeSchema
>
