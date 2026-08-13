import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  credentialStatusSchema,
  openRouterApiKeySchema,
  type CredentialStatus,
} from "@/contracts/credentials"

async function invokeStatus(
  command: string,
  args?: Record<string, unknown>,
): Promise<CredentialStatus> {
  let response: unknown
  try {
    response = await invoke<unknown>(command, args)
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const parsed = credentialStatusSchema.safeParse(response)
  if (!parsed.success) throw createContractApplicationError()
  return parsed.data
}

export function getOpenRouterCredentialStatus(): Promise<CredentialStatus> {
  return invokeStatus("get_openrouter_credential_status")
}

export function deleteOpenRouterApiKey(): Promise<CredentialStatus> {
  return invokeStatus("delete_openrouter_api_key")
}

export function setOpenRouterApiKey(apiKey: string): Promise<CredentialStatus> {
  const parsed = openRouterApiKeySchema.safeParse(apiKey)
  if (!parsed.success)
    return Promise.reject(createRequestContractApplicationError())
  return invokeStatus("set_openrouter_api_key", { apiKey: parsed.data })
}
