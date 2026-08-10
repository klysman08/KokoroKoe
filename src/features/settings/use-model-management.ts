import { useEffect } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { ApplicationError } from "@/contracts/app-error"
import type {
  ModelDownloadJob,
  ModelInstallation,
  RequestId,
} from "@/contracts/models"
import type { AppSettings } from "@/contracts/settings"
import { settingsQueryKey } from "@/features/settings/use-settings-query"
import {
  cancelModelDownload,
  deleteTranscriptionModel,
  downloadTranscriptionModel,
  listenToModelDownloadProgress,
  listTranscriptionModels,
  resumeModelDownload,
  setDefaultTranscriptionModel,
} from "@/lib/tauri/models"

export const modelsQueryKey = ["transcription-models"] as const

export function useModelsQuery() {
  return useQuery<ModelInstallation[], ApplicationError>({
    queryKey: modelsQueryKey,
    queryFn: listTranscriptionModels,
    retry: false,
    gcTime: 0,
  })
}

export function useModelProgressEvents() {
  const queryClient = useQueryClient()
  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void listenToModelDownloadProgress((event) => {
      queryClient.setQueryData<ModelInstallation[]>(modelsQueryKey, (models) =>
        models?.map((model) =>
          model.descriptor.id === event.payload.job.modelId
            ? {
                ...model,
                downloadJob: event.payload.job,
                status:
                  event.payload.job.status === "failed"
                    ? "failed"
                    : event.payload.job.status === "completed"
                      ? "installed"
                      : "downloading",
                lastError: event.payload.job.error,
              }
            : model,
        ),
      )
      if (
        ["completed", "cancelled", "failed"].includes(event.payload.job.status)
      ) {
        void queryClient.invalidateQueries({ queryKey: modelsQueryKey })
      }
    })
      .then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
      .catch(() => undefined)
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [queryClient])
}

function useModelMutation<TVariables, TData>(
  mutationFn: (variables: TVariables) => Promise<TData>,
) {
  const queryClient = useQueryClient()
  return useMutation<TData, ApplicationError, TVariables>({
    mutationFn,
    retry: false,
    gcTime: 0,
    onSettled: async () =>
      queryClient.invalidateQueries({ queryKey: modelsQueryKey }),
  })
}

export const useDownloadModelMutation = () =>
  useModelMutation<string, ModelDownloadJob>(downloadTranscriptionModel)
export const useResumeModelMutation = () =>
  useModelMutation<string, ModelDownloadJob>(resumeModelDownload)
export const useCancelModelMutation = () =>
  useModelMutation<RequestId, ModelDownloadJob>(cancelModelDownload)
export const useDeleteModelMutation = () =>
  useModelMutation<string, ModelInstallation>(deleteTranscriptionModel)

export function useSetDefaultModelMutation() {
  const queryClient = useQueryClient()
  return useMutation<
    AppSettings,
    ApplicationError,
    { modelId: string; expectedSettingsRevision: number }
  >({
    mutationFn: ({ modelId, expectedSettingsRevision }) =>
      setDefaultTranscriptionModel(modelId, expectedSettingsRevision),
    retry: false,
    gcTime: 0,
    onSuccess: (settings) =>
      queryClient.setQueryData(settingsQueryKey, settings),
    onSettled: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: modelsQueryKey }),
        queryClient.invalidateQueries({ queryKey: settingsQueryKey }),
      ])
    },
  })
}
