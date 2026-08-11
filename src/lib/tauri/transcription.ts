import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import { requestIdSchema, type RequestId } from "@/contracts/models"
import {
  liveTranscriptionInputSchema,
  liveTranscriptionStatusSchema,
  transcriptionFinalEnvelopeSchema,
  transcriptionGapEnvelopeSchema,
  transcriptionPartialEnvelopeSchema,
  type LiveTranscriptionInput,
  type LiveTranscriptionStatus,
  type TranscriptionFinalEnvelope,
  type TranscriptionGapEnvelope,
  type TranscriptionPartialEnvelope,
} from "@/contracts/transcription"

export const TRANSCRIPTION_PARTIAL_EVENT = "transcription-partial"
export const TRANSCRIPTION_FINAL_EVENT = "transcription-final"
export const TRANSCRIPTION_GAP_EVENT = "transcription-gap"

export async function startLiveTranscription(
  input: LiveTranscriptionInput,
  requestId: RequestId,
): Promise<LiveTranscriptionStatus> {
  const parsedInput = liveTranscriptionInputSchema.safeParse(input)
  const parsedRequestId = requestIdSchema.safeParse(requestId)
  if (!parsedInput.success || !parsedRequestId.success) {
    throw createRequestContractApplicationError()
  }
  const status = await invokeAndParse(
    "start_live_transcription",
    liveTranscriptionStatusSchema,
    { input: parsedInput.data, requestId: parsedRequestId.data },
  )
  if (status.requestId !== parsedRequestId.data) {
    throw createContractApplicationError()
  }
  return status
}

export function getLiveTranscriptionStatus(): Promise<LiveTranscriptionStatus> {
  return invokeAndParse(
    "get_live_transcription_status",
    liveTranscriptionStatusSchema,
  )
}

export async function stopLiveTranscription(
  requestId: RequestId,
): Promise<LiveTranscriptionStatus> {
  const parsed = requestIdSchema.safeParse(requestId)
  if (!parsed.success) throw createRequestContractApplicationError()
  const status = await invokeAndParse(
    "stop_live_transcription",
    liveTranscriptionStatusSchema,
    { requestId: parsed.data },
  )
  if (status.requestId !== parsed.data) throw createContractApplicationError()
  return status
}

export function listenToTranscriptionPartials(
  onEvent: (event: TranscriptionPartialEnvelope) => void,
): Promise<UnlistenFn> {
  return listenParsed(
    TRANSCRIPTION_PARTIAL_EVENT,
    transcriptionPartialEnvelopeSchema,
    onEvent,
  )
}

export function listenToTranscriptionFinals(
  onEvent: (event: TranscriptionFinalEnvelope) => void,
): Promise<UnlistenFn> {
  return listenParsed(
    TRANSCRIPTION_FINAL_EVENT,
    transcriptionFinalEnvelopeSchema,
    onEvent,
  )
}

export function listenToTranscriptionGaps(
  onEvent: (event: TranscriptionGapEnvelope) => void,
): Promise<UnlistenFn> {
  return listenParsed(
    TRANSCRIPTION_GAP_EVENT,
    transcriptionGapEnvelopeSchema,
    onEvent,
  )
}

function listenParsed<Output>(
  event: string,
  schema: {
    safeParse(
      value: unknown,
    ): { success: true; data: Output } | { success: false }
  },
  onEvent: (event: Output) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(event, ({ payload }) => {
    const parsed = schema.safeParse(payload)
    if (parsed.success) onEvent(parsed.data)
  })
}

async function invokeAndParse<Output>(
  command: string,
  schema: {
    safeParse(
      value: unknown,
    ): { success: true; data: Output } | { success: false }
  },
  args?: Record<string, unknown>,
): Promise<Output> {
  let response: unknown
  try {
    response = await invoke<unknown>(command, args)
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const parsed = schema.safeParse(response)
  if (!parsed.success) throw createContractApplicationError()
  return parsed.data
}
