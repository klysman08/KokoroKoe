import { useMutation, useQueryClient } from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import {
  appSettingsSchema,
  type AppSettings,
  type VersionedAppSettingsUpdate,
  type WorkspaceStatus,
} from "@/contracts/settings"
import { settingsQueryKey } from "@/features/settings/use-settings-query"
import {
  chooseWorkspace,
  openWorkspaceFolder,
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
