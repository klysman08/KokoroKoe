import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  annotateTranscriptSegmentRequestSchema,
  transcriptPageRequestSchema,
  transcriptPageSchema,
  transcriptSegmentSchema,
  transcriptSearchPageSchema,
  transcriptSearchRequestSchema,
  type AnnotateTranscriptSegmentRequest,
  type TranscriptPage,
  type TranscriptPageRequest,
  type TranscriptSegment,
  type TranscriptSearchPage,
  type TranscriptSearchRequest,
} from "@/contracts/transcripts"

async function call<T>(
  command: string,
  args: Record<string, unknown>,
  schema: { safeParse: (value: unknown) => { success: boolean; data?: T } },
): Promise<T> {
  let response: unknown
  try {
    response = await invoke<unknown>(command, args)
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const parsed = schema.safeParse(response)
  if (!parsed.success) throw createContractApplicationError()
  return parsed.data as T
}

function request<T>(
  schema: { safeParse: (value: unknown) => { success: boolean; data?: T } },
  value: unknown,
): T {
  const parsed = schema.safeParse(value)
  if (!parsed.success) throw createRequestContractApplicationError()
  return parsed.data as T
}

export async function getTranscriptPage(
  value: TranscriptPageRequest,
): Promise<TranscriptPage> {
  const validated = request(transcriptPageRequestSchema, value)
  const page = await call(
    "get_transcript_page",
    validated,
    transcriptPageSchema,
  )
  if (
    page.items.some(
      (item) =>
        item.projectId !== validated.projectId ||
        item.sessionId !== validated.sessionId,
    )
  )
    throw createContractApplicationError()
  return page
}

export async function searchTranscript(
  value: TranscriptSearchRequest,
): Promise<TranscriptSearchPage> {
  const validated = request(transcriptSearchRequestSchema, value)
  const page = await call(
    "search_transcript",
    validated,
    transcriptSearchPageSchema,
  )
  if (
    page.items.some(
      (item) =>
        item.projectId !== validated.projectId ||
        (validated.sessionId !== undefined &&
          item.sessionId !== validated.sessionId),
    )
  )
    throw createContractApplicationError()
  return page
}

/**
 * Records a correction or an importance mark against one finalized segment.
 *
 * Rust appends it to the append-only journal and republishes the transcript
 * from that log, so the transcription is never overwritten and the updated
 * segment comes back carrying both readings.
 */
export async function annotateTranscriptSegment(
  value: AnnotateTranscriptSegmentRequest,
): Promise<TranscriptSegment> {
  const request = annotateTranscriptSegmentRequestSchema.safeParse(value)
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>("annotate_transcript_segment", {
      request: request.data,
    })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const segment = transcriptSegmentSchema.safeParse(raw)
  if (!segment.success || segment.data.id !== request.data.segmentId)
    throw createContractApplicationError()
  return segment.data
}
