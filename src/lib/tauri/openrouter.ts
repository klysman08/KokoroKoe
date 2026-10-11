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

/**
 * Loads the model catalog a generation is about to be priced against.
 *
 * Rust prices every request from a catalog it caches for fifteen minutes and,
 * by design, never fetches on its own from inside a completion. So a meeting
 * that outlived the cache failed every insight with "catalog required", and a
 * summary at its end failed the same way, until someone opened Settings. This
 * asks for the catalog first — a request for the public model list, carrying no
 * meeting content, that Rust answers from its cache while it is still fresh.
 * A failure here is not reported: the generation that follows reports the real
 * problem, such as a missing key, in its own words.
 */
export async function ensureModelCatalog(): Promise<void> {
  try {
    await listOpenRouterModels(false)
  } catch {
    // The generation below reports what is actually wrong.
  }
}
