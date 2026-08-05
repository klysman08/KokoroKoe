import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import { appSettingsSchema, type AppSettings } from "@/contracts/settings"

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
