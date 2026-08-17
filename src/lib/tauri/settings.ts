import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  appSettingsSchema,
  versionedAppSettingsUpdateSchema,
  workspaceStatusSchema,
  type AppSettings,
  type VersionedAppSettingsUpdate,
  type WorkspaceStatus,
} from "@/contracts/settings"

export async function getSettings(): Promise<AppSettings> {
  let response: unknown

  try {
    response = await invoke<unknown>("get_settings")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }

  const parsed = appSettingsSchema.safeParse(response)
  if (!parsed.success) {
    throw createContractApplicationError()
  }

  return parsed.data
}

export async function updateSettings(
  request: VersionedAppSettingsUpdate,
): Promise<AppSettings> {
  const validatedRequest = versionedAppSettingsUpdateSchema.safeParse(request)
  if (!validatedRequest.success) {
    throw createRequestContractApplicationError()
  }

  let response: unknown
  try {
    response = await invoke<unknown>("update_settings", validatedRequest.data)
  } catch (error: unknown) {
    throw toApplicationError(error)
  }

  const parsed = appSettingsSchema.safeParse(response)
  if (!parsed.success) {
    throw createContractApplicationError()
  }
  return parsed.data
}

export async function chooseWorkspace(): Promise<WorkspaceStatus> {
  let response: unknown
  try {
    response = await invoke<unknown>("choose_workspace")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }

  const parsed = workspaceStatusSchema.safeParse(response)
  if (!parsed.success) {
    throw createContractApplicationError()
  }
  return parsed.data
}

export async function openWorkspaceFolder(): Promise<void> {
  try {
    await invoke("open_workspace_folder")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
}
