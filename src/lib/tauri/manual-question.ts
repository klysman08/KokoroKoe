import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  askManualQuestionRequestSchema,
  manualQuestionResponseSchema,
  type AskManualQuestionRequest,
  type ManualQuestionResponse,
} from "@/contracts/manual-question"

export async function askManualQuestion(
  value: AskManualQuestionRequest,
): Promise<ManualQuestionResponse> {
  const request = askManualQuestionRequestSchema.safeParse(value)
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>("ask_manual_question", {
      request: request.data,
    })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const response = manualQuestionResponseSchema.safeParse(raw)
  if (!response.success) throw createContractApplicationError()
  if (
    response.data.requestId !== request.data.requestId ||
    response.data.projectId !== request.data.projectId ||
    response.data.sessionId !== request.data.sessionId ||
    response.data.selectedSegmentId !== request.data.selectedSegmentId
  )
    throw createContractApplicationError()
  return response.data
}
