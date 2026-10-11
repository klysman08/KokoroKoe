import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  dashboardMetricsQuerySchema,
  dashboardMetricsSchema,
  type DashboardMetrics,
  type DashboardMetricsQuery,
} from "@/contracts/dashboard"

export async function getDashboardMetrics(
  query: DashboardMetricsQuery,
): Promise<DashboardMetrics> {
  const request = dashboardMetricsQuerySchema.safeParse(query)
  if (!request.success) throw createRequestContractApplicationError()

  let response: unknown
  try {
    response = await invoke("get_dashboard_metrics", { query: request.data })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const parsed = dashboardMetricsSchema.safeParse(response)
  if (!parsed.success) throw createContractApplicationError()
  return parsed.data
}
