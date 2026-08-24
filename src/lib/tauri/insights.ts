import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  generateRecentInsightsRequestSchema,
  recentInsightsResponseSchema,
  sessionInsightsPublicationSchema,
  type GenerateRecentInsightsRequest,
  type RecentInsightsResponse,
  type SessionInsightsPublication,
} from "@/contracts/insights"

export const SESSION_INSIGHTS_EVENT = "session-insights"

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

/**
 * Opens the detached insights window.
 *
 * Rust owns the window's label, URL, title, and size: no argument is sent, so
 * the frontend cannot address or create any other webview.
 */
export async function openInsightsWindow(): Promise<void> {
  try {
    await invoke<void>("open_insights_window")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
}

export async function closeInsightsWindow(): Promise<void> {
  try {
    await invoke<void>("close_insights_window")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
}

/**
 * Subscribes to generated insight batches.
 *
 * This is how the detached insights window learns anything at all: it holds no
 * command permission, so it cannot ask for a generation and cannot fetch a past
 * one. A batch that does not match the contract is dropped rather than
 * rendered.
 */
export function listenToSessionInsights(
  onEvent: (publication: SessionInsightsPublication) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(SESSION_INSIGHTS_EVENT, (event) => {
    const publication = sessionInsightsPublicationSchema.safeParse(
      event.payload,
    )
    if (publication.success) onEvent(publication.data)
  })
}
