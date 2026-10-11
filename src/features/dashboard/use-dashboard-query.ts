import { useQuery } from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import {
  type DashboardMetrics,
  type DashboardMetricsQuery,
} from "@/contracts/dashboard"
import { type ProjectId } from "@/contracts/projects"
import { getDashboardMetrics } from "@/lib/tauri/dashboard"

export type DashboardPeriod = "7d" | "30d" | "all"

const DAY_MS = 24 * 60 * 60 * 1000
const PERIOD_DAYS: Record<Exclude<DashboardPeriod, "all">, number> = {
  "7d": 7,
  "30d": 30,
}

/**
 * The query a period stands for, resolved at fetch time so "the last seven
 * days" means the seven days before this read rather than before the page
 * first rendered.
 */
export function dashboardQueryFor(
  period: DashboardPeriod,
  projectId: ProjectId | undefined,
  now: number = Date.now(),
): DashboardMetricsQuery {
  const query: DashboardMetricsQuery = {}
  if (period !== "all") {
    query.from = new Date(now - PERIOD_DAYS[period] * DAY_MS).toISOString()
  }
  if (projectId) query.projectId = projectId
  return query
}

export function useDashboardQuery(
  period: DashboardPeriod,
  projectId: ProjectId | undefined,
) {
  return useQuery<DashboardMetrics, ApplicationError>({
    queryKey: ["dashboard-metrics", period, projectId ?? "all-projects"],
    queryFn: () => getDashboardMetrics(dashboardQueryFor(period, projectId)),
    retry: (failureCount, error) => error.details.retryable && failureCount < 1,
    retryDelay: 0,
    throwOnError: false,
  })
}
