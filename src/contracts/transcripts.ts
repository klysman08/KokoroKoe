import { z } from "zod"

import { audioSourceSchema } from "@/contracts/audio"
import { projectIdSchema, sessionIdSchema } from "@/contracts/projects"

const safeMilliseconds = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER)
const segmentIdSchema = z.uuid().brand<"SegmentId">()
const languageSchema = z
  .string()
  .min(2)
  .max(64)
  .regex(/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8}){0,7}$/)
const isControl = (character: string) => /\p{Cc}/u.test(character)
const controlFree = (value: string) =>
  [...value].every((character) => !isControl(character))
const transcriptTextSchema = z
  .string()
  .min(1)
  .refine((value) => new TextEncoder().encode(value).length <= 32 * 1024)
  .refine((value) =>
    [...value].every((character) => {
      return (
        character !== "\r" &&
        (!isControl(character) || character === "\n" || character === "\t")
      )
    }),
  )

export const transcriptSegmentSchema = z
  .strictObject({
    id: segmentIdSchema,
    projectId: projectIdSchema,
    sessionId: sessionIdSchema,
    source: audioSourceSchema,
    startMs: safeMilliseconds,
    endMs: safeMilliseconds,
    text: transcriptTextSchema,
    status: z.literal("final"),
    language: languageSchema,
  })
  .refine((value) => value.endMs > value.startMs)

const readCursorSchema = z
  .string()
  .min(1)
  .max(256)
  .regex(/^r1:[0-9a-f-]{36}:[0-9a-f-]{36}:[0-9a-f]{64}:\d+$/)

export const transcriptPageRequestSchema = z.strictObject({
  projectId: projectIdSchema,
  sessionId: sessionIdSchema,
  cursor: readCursorSchema.optional(),
  limit: z.number().int().min(1).max(100),
})

export const transcriptPageSchema = z.strictObject({
  items: z.array(transcriptSegmentSchema).max(100),
  nextCursor: readCursorSchema.optional(),
})

const searchCursorSchema = z
  .string()
  .min(1)
  .max(256)
  .regex(/^t1:\d+:[0-9a-f-]{36}:(?:\*|[0-9a-f-]{36}):[0-9a-f]{16}:\d+$/)

const transcriptQuerySchema = z
  .string()
  .min(1)
  .refine((value) => new TextEncoder().encode(value).length <= 256)
  .refine(controlFree)
  .refine((value) => {
    const terms = value.trim().split(/\s+/)
    return terms.length >= 1 && terms.length <= 16 && terms[0] !== ""
  })

export const transcriptSearchRequestSchema = z.strictObject({
  projectId: projectIdSchema,
  sessionId: sessionIdSchema.optional(),
  query: transcriptQuerySchema,
  cursor: searchCursorSchema.optional(),
  limit: z.number().int().min(1).max(100),
})

export const transcriptSearchHitSchema = z
  .strictObject({
    id: segmentIdSchema,
    projectId: projectIdSchema,
    sessionId: sessionIdSchema,
    source: audioSourceSchema,
    startMs: safeMilliseconds,
    endMs: safeMilliseconds,
    language: languageSchema,
    snippet: z.string().min(1).max(240).refine(controlFree),
  })
  .refine((value) => value.endMs > value.startMs)

export const transcriptSearchPageSchema = z.strictObject({
  items: z.array(transcriptSearchHitSchema).max(100),
  nextCursor: searchCursorSchema.optional(),
})

export const transcriptReadingFixtureSchema = z.strictObject({
  pageRequest: transcriptPageRequestSchema,
  page: transcriptPageSchema,
  searchRequest: transcriptSearchRequestSchema,
  searchPage: transcriptSearchPageSchema,
})

export type TranscriptSegment = z.infer<typeof transcriptSegmentSchema>
export type TranscriptPageRequest = z.infer<typeof transcriptPageRequestSchema>
export type TranscriptPage = z.infer<typeof transcriptPageSchema>
export type TranscriptSearchRequest = z.infer<
  typeof transcriptSearchRequestSchema
>
export type TranscriptSearchHit = z.infer<typeof transcriptSearchHitSchema>
export type TranscriptSearchPage = z.infer<typeof transcriptSearchPageSchema>
