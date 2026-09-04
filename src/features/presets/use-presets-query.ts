import { useQuery } from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import { type Preset } from "@/contracts/presets"
import { listPresets } from "@/lib/tauri/presets"

export const presetsQueryKey = ["presets"] as const

// The library is a compile-time catalog in Rust, so it cannot change while the
// application runs: fetch it once and keep it.
export function usePresetsQuery() {
  return useQuery<Preset[], ApplicationError>({
    queryKey: presetsQueryKey,
    queryFn: listPresets,
    staleTime: Infinity,
    retry: (failureCount, error) => error.details.retryable && failureCount < 1,
    retryDelay: 0,
    throwOnError: false,
  })
}
