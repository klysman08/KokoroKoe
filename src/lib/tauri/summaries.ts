import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  generateSessionSummaryRequestSchema,
  generateSessionSummaryResponseSchema,
  getSessionSummaryRequestSchema,
  sessionSummaryStatusSchema,
  type GenerateSessionSummaryRequest,
  type GenerateSessionSummaryResponse,
  type GetSessionSummaryRequest,
  type SessionSummaryStatus,
} from "@/contracts/summaries"

export async function getSessionSummary(
  value: GetSessionSummaryRequest,
): Promise<SessionSummaryStatus> {
  const request = getSessionSummaryRequestSchema.safeParse(value)
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>("get_session_summary", {
      request: request.data,
    })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const status = sessionSummaryStatusSchema.safeParse(raw)
  if (!status.success) throw createContractApplicationError()
  if (
    status.data.projectId !== request.data.projectId ||
    status.data.sessionId !== request.data.sessionId
  )
    throw createContractApplicationError()
  return status.data
}

export async function generateSessionSummary(
  value: GenerateSessionSummaryRequest,
): Promise<GenerateSessionSummaryResponse> {
  const request = generateSessionSummaryRequestSchema.safeParse(value)
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>("generate_session_summary", {
      request: request.data,
    })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const response = generateSessionSummaryResponseSchema.safeParse(raw)
  if (!response.success) throw createContractApplicationError()
  if (
    response.data.requestId !== request.data.requestId ||
    response.data.document.projectId !== request.data.projectId ||
    response.data.document.sessionId !== request.data.sessionId
  )
    throw createContractApplicationError()
  return response.data
}
