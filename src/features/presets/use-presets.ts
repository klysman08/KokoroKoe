import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import {
  type CreatePresetInput,
  type Preset,
  type PresetExport,
  type UpdatePresetInput,
} from "@/contracts/presets"
import {
  createPreset,
  deleteCustomPreset,
  duplicatePreset,
  exportPreset,
  listPresets,
  updatePreset,
} from "@/lib/tauri/presets"

// The library is keyed by the project whose presets it includes. Without a
// project it is the built-in catalog, which is a compile-time constant in Rust
// and cannot change while the application runs.
export const presetsQueryKey = (projectId?: string) =>
  projectId === undefined
    ? (["presets"] as const)
    : (["presets", projectId] as const)

export function usePresetsQuery(projectId?: string) {
  return useQuery<Preset[], ApplicationError>({
    queryKey: presetsQueryKey(projectId),
    queryFn: () => listPresets(projectId),
    staleTime: projectId === undefined ? Infinity : 0,
    retry: (failureCount, error) => error.details.retryable && failureCount < 1,
    retryDelay: 0,
    throwOnError: false,
  })
}

export function useCreatePresetMutation(projectId: string) {
  const queryClient = useQueryClient()
  return useMutation<Preset, ApplicationError, CreatePresetInput>({
    mutationFn: (value) => createPreset(projectId, value),
    onSuccess: () => invalidate(queryClient, projectId),
  })
}

export function useUpdatePresetMutation(projectId: string) {
  const queryClient = useQueryClient()
  return useMutation<
    Preset,
    ApplicationError,
    { presetId: string; expectedRevision: number; value: UpdatePresetInput }
  >({
    mutationFn: ({ presetId, expectedRevision, value }) =>
      updatePreset(projectId, presetId, expectedRevision, value),
    onSuccess: () => invalidate(queryClient, projectId),
  })
}

export function useDuplicatePresetMutation(projectId: string) {
  const queryClient = useQueryClient()
  return useMutation<
    Preset,
    ApplicationError,
    { presetId: string; name: string }
  >({
    mutationFn: ({ presetId, name }) =>
      duplicatePreset(projectId, presetId, name),
    onSuccess: () => invalidate(queryClient, projectId),
  })
}

export function useDeletePresetMutation(projectId: string) {
  const queryClient = useQueryClient()
  return useMutation<
    void,
    ApplicationError,
    { presetId: string; expectedRevision: number }
  >({
    mutationFn: ({ presetId, expectedRevision }) =>
      deleteCustomPreset(projectId, presetId, expectedRevision),
    onSuccess: () => invalidate(queryClient, projectId),
  })
}

// Export writes a file and changes nothing in the library, so it invalidates
// nothing.
export function useExportPresetMutation(projectId: string) {
  return useMutation<PresetExport, ApplicationError, { presetId: string }>({
    mutationFn: ({ presetId }) => exportPreset(projectId, presetId),
  })
}

function invalidate(
  queryClient: ReturnType<typeof useQueryClient>,
  projectId: string,
) {
  return queryClient.invalidateQueries({
    queryKey: presetsQueryKey(projectId),
  })
}
