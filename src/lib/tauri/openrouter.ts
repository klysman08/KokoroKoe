import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import { requestIdSchema } from "@/contracts/models"
import {
  credentialValidationSchema,
  openRouterModelListSchema,
  type CredentialValidation,
  type OpenRouterModel,
} from "@/contracts/openrouter"

async function call<T>(
  command: string,
  args: Record<string, unknown>,
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

function requestId() {
  return requestIdSchema.parse(globalThis.crypto.randomUUID())
}

export function validateOpenRouterApiKey(): Promise<CredentialValidation> {
  return call(
    "validate_openrouter_api_key",
    { requestId: requestId() },
    credentialValidationSchema,
  )
}

export function listOpenRouterModels(
  forceRefresh: boolean,
): Promise<OpenRouterModel[]> {
  return call(
    "list_openrouter_models",
    { forceRefresh, requestId: requestId() },
    openRouterModelListSchema,
  )
}
