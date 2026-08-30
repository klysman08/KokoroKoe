import { useMutation, useQueryClient } from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import {
  appSettingsSchema,
  type AppSettings,
  type VersionedAppSettingsUpdate,
  type WorkspaceRescanReport,
  type WorkspaceStatus,
} from "@/contracts/settings"
import { settingsQueryKey } from "@/features/settings/use-settings-query"
import {
  chooseWorkspace,
  openWorkspaceFolder,
  rescanWorkspace,
  updateSettings,
} from "@/lib/tauri/settings"

export function useUpdateSettingsMutation() {
  const queryClient = useQueryClient()

  return useMutation<
    AppSettings,
    ApplicationError,
    VersionedAppSettingsUpdate,
    { previous: AppSettings | undefined }
  >({
    mutationFn: updateSettings,
    retry: false,
    gcTime: 0,
    onMutate: async (request) => {
      await queryClient.cancelQueries({ queryKey: settingsQueryKey })
      const previous = queryClient.getQueryData<AppSettings>(settingsQueryKey)
      if (previous?.revision === request.expectedRevision) {
        const optimistic = appSettingsSchema.safeParse({
          ...previous,
          ...request.value,
        })
        if (optimistic.success) {
          queryClient.setQueryData<AppSettings>(
            settingsQueryKey,
            optimistic.data,
          )
        }
      }
      return { previous }
    },
    onError: (_error, _request, context) => {
      if (context?.previous) {
        queryClient.setQueryData(settingsQueryKey, context.previous)
      }
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(settingsQueryKey, settings)
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: settingsQueryKey })
    },
  })
}

export function useChooseWorkspaceMutation() {
  const queryClient = useQueryClient()

  return useMutation<WorkspaceStatus, ApplicationError>({
    mutationFn: chooseWorkspace,
    retry: false,
    gcTime: 0,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: settingsQueryKey })
    },
  })
}

export function useOpenWorkspaceFolderMutation() {
  return useMutation<void, ApplicationError>({
    mutationFn: openWorkspaceFolder,
    retry: false,
    gcTime: 0,
  })
}

/**
 * Rebuilds the derived indexes from the workspace on disk.
 *
 * Every list in the application reads one of those indexes, so they are all
 * invalidated afterwards: a rescan exists precisely because what is on disk
 * and what the application believes have drifted apart.
 */
export function useRescanWorkspaceMutation() {
  const queryClient = useQueryClient()
  return useMutation<WorkspaceRescanReport, ApplicationError>({
    mutationFn: rescanWorkspace,
    retry: false,
    gcTime: 0,
    onSettled: async () => {
      await queryClient.invalidateQueries()
    },
  })
}
