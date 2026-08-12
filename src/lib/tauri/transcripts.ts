import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  transcriptPageRequestSchema,
  transcriptPageSchema,
  transcriptSearchPageSchema,
  transcriptSearchRequestSchema,
  type TranscriptPage,
  type TranscriptPageRequest,
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
