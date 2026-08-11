import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  sessionCreateRequestSchema,
  sessionIdentitySchema,
  sessionPageRequestSchema,
  sessionPageSchema,
  sessionSchema,
  sessionUpdateRequestSchema,
  type Session,
  type SessionCreateRequest,
  type SessionIdentity,
  type SessionPage,
  type SessionPageRequest,
  type SessionUpdateRequest,
} from "@/contracts/projects"

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

function request<T>(
  schema: { safeParse: (value: unknown) => { success: boolean; data?: T } },
  value: unknown,
): T {
  const parsed = schema.safeParse(value)
  if (!parsed.success) throw createRequestContractApplicationError()
  return parsed.data as T
}

export async function listSessions(
  value: SessionPageRequest,
): Promise<SessionPage> {
  return await call(
    "list_sessions",
    request(sessionPageRequestSchema, value),
    sessionPageSchema,
  )
}

export async function getSession(value: SessionIdentity): Promise<Session> {
  return await call(
    "get_session",
    request(sessionIdentitySchema, value),
    sessionSchema,
  )
}

export async function createSession(
  value: SessionCreateRequest,
): Promise<Session> {
  const validated = request(sessionCreateRequestSchema, value)
  return await call(
    "create_session",
    { projectId: validated.projectId, ...validated.value },
    sessionSchema,
  )
}

export async function updateSession(
  value: SessionUpdateRequest,
): Promise<Session> {
  return await call(
    "update_session",
    request(sessionUpdateRequestSchema, value),
    sessionSchema,
  )
}
