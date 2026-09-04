import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import { presetListSchema, type Preset } from "@/contracts/presets"

export async function listPresets(): Promise<Preset[]> {
  let response: unknown

  try {
    response = await invoke<unknown>("list_presets")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }

  const parsed = presetListSchema.safeParse(response)
  if (!parsed.success) {
    throw createContractApplicationError()
  }

  return parsed.data
}
