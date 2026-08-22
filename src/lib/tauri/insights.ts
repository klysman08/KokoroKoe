import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  generateRecentInsightsRequestSchema,
  recentInsightsResponseSchema,
  type GenerateRecentInsightsRequest,
  type RecentInsightsResponse,
} from "@/contracts/insights"

export async function generateRecentInsights(
  value: GenerateRecentInsightsRequest,
): Promise<RecentInsightsResponse> {
  const request = generateRecentInsightsRequestSchema.safeParse(value)
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>("generate_recent_insights", {
      request: request.data,
    })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const response = recentInsightsResponseSchema.safeParse(raw)
  if (!response.success) throw createContractApplicationError()
  if (
    response.data.requestId !== request.data.requestId ||
    response.data.projectId !== request.data.projectId ||
    response.data.sessionId !== request.data.sessionId
  )
    throw createContractApplicationError()
  return response.data
}
