import { useQuery } from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import { type AppSettings } from "@/contracts/settings"
import { getSettings } from "@/lib/tauri/settings"

export const settingsQueryKey = ["app-settings"] as const

export function useSettingsQuery() {
  return useQuery<AppSettings, ApplicationError>({
    queryKey: settingsQueryKey,
    queryFn: getSettings,
    gcTime: 0,
    retry: (failureCount, error) => error.details.retryable && failureCount < 1,
    retryDelay: 0,
    throwOnError: false,
  })
}
