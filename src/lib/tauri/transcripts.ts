import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  annotateTranscriptSegmentRequestSchema,
  markTranscriptSegmentRequestSchema,
  transcriptBookmarkListRequestSchema,
  transcriptBookmarkListSchema,
  transcriptPageRequestSchema,
  transcriptPageSchema,
  transcriptSegmentSchema,
  transcriptSearchPageSchema,
  transcriptSearchRequestSchema,
  transcriptSegmentHistoryRequestSchema,
  transcriptSegmentHistorySchema,
  type AnnotateTranscriptSegmentRequest,
  type MarkTranscriptSegmentRequest,
  type TranscriptBookmarkList,
  type TranscriptBookmarkListRequest,
  type TranscriptSegmentHistory,
  type TranscriptSegmentHistoryRequest,
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

/**
 * Reads how one segment came to read as it does.
 *
 * Rust reads the append-only journal, so this returns wordings the transcript
 * document no longer carries. It writes nothing: asking how a segment got here
 * cannot change where it is.
 */
export async function getTranscriptSegmentHistory(
  value: TranscriptSegmentHistoryRequest,
): Promise<TranscriptSegmentHistory> {
  const validated = request(transcriptSegmentHistoryRequestSchema, value)
  const history = await call(
    "get_transcript_segment_history",
    { request: validated },
    transcriptSegmentHistorySchema,
  )
  if (
    history.projectId !== validated.projectId ||
    history.sessionId !== validated.sessionId ||
    history.segmentId !== validated.segmentId
  )
    throw createContractApplicationError()
  return history
}

/**
 * Flags one finalized segment from a window that may do nothing else.
 *
 * This is the transcript window's only command. It is kept separate from
 * `annotateTranscriptSegment` rather than sharing it, because the narrowness of
 * the request is the reason the window is allowed to make it at all.
 */
export async function markTranscriptSegment(
  value: MarkTranscriptSegmentRequest,
): Promise<TranscriptSegment> {
  const validated = request(markTranscriptSegmentRequestSchema, value)
  const segment = await call(
    "mark_transcript_segment",
    { request: validated },
    transcriptSegmentSchema,
  )
  if (segment.id !== validated.segmentId) throw createContractApplicationError()
  return segment
}

/**
 * Every place in one Session the reader asked to return to.
 *
 * Rust reads the journal, so a bookmark dropped during a run is listed before
 * the run has published it. It writes nothing.
 */
export async function listTranscriptBookmarks(
  value: TranscriptBookmarkListRequest,
): Promise<TranscriptBookmarkList> {
  const validated = request(transcriptBookmarkListRequestSchema, value)
  const list = await call(
    "list_transcript_bookmarks",
    { request: validated },
    transcriptBookmarkListSchema,
  )
  if (
    list.items.some(
      (item) =>
        item.projectId !== validated.projectId ||
        item.sessionId !== validated.sessionId,
    )
  )
    throw createContractApplicationError()
  return list
}
