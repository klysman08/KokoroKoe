import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  modelDownloadJobSchema,
  modelDownloadProgressEnvelopeSchema,
  modelInstallationListSchema,
  modelInstallationSchema,
  requestIdSchema,
  type ModelDownloadJob,
  type ModelDownloadProgressEnvelope,
  type ModelInstallation,
} from "@/contracts/models"
import { appSettingsSchema, type AppSettings } from "@/contracts/settings"

const modelIdSchema = modelInstallationSchema.shape.descriptor.shape.id

async function call<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  schema: { safeParse: (value: unknown) => { success: boolean; data?: T } },
): Promise<T> {
  let response: unknown
  try {
    response = await invoke<unknown>(command, args)
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const parsed = schema.safeParse(response)
  if (!parsed.success) throw createContractApplicationError()
  return parsed.data as T
}

function validModelId(modelId: string) {
  const parsed = modelIdSchema.safeParse(modelId)
  if (!parsed.success) throw createRequestContractApplicationError()
  return parsed.data
}

export function listTranscriptionModels() {
  return call<ModelInstallation[]>(
    "list_transcription_models",
    undefined,
    modelInstallationListSchema,
  )
}

export function downloadTranscriptionModel(modelId: string) {
  return call<ModelDownloadJob>(
    "download_transcription_model",
    {
      modelId: validModelId(modelId),
      requestId: requestIdSchema.parse(crypto.randomUUID()),
    },
    modelDownloadJobSchema,
  )
}

export function resumeModelDownload(modelId: string) {
  return call<ModelDownloadJob>(
    "resume_model_download",
    {
      modelId: validModelId(modelId),
      requestId: requestIdSchema.parse(crypto.randomUUID()),
    },
    modelDownloadJobSchema,
  )
}

export function cancelModelDownload(requestId: string) {
  const parsed = requestIdSchema.safeParse(requestId)
  if (!parsed.success) throw createRequestContractApplicationError()
  return call<ModelDownloadJob>(
    "cancel_model_download",
    { requestId: parsed.data },
    modelDownloadJobSchema,
  )
}

export function deleteTranscriptionModel(modelId: string) {
  return call<ModelInstallation>(
    "delete_transcription_model",
    { modelId: validModelId(modelId) },
    modelInstallationSchema,
  )
}

export function setDefaultTranscriptionModel(
  modelId: string,
  expectedSettingsRevision: number,
) {
  if (
    !Number.isSafeInteger(expectedSettingsRevision) ||
    expectedSettingsRevision < 0
  ) {
    throw createRequestContractApplicationError()
  }
  return call<AppSettings>(
    "set_default_transcription_model",
    { modelId: validModelId(modelId), expectedSettingsRevision },
    appSettingsSchema,
  )
}

export async function listenToModelDownloadProgress(
  onProgress: (event: ModelDownloadProgressEnvelope) => void,
): Promise<UnlistenFn> {
  return listen<unknown>("model-download-progress", ({ payload }) => {
    const parsed = modelDownloadProgressEnvelopeSchema.safeParse(payload)
    if (parsed.success) onProgress(parsed.data)
  })
}
