import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

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
import {
  pausePersistedSessionRequestSchema,
  persistenceStatusSchema,
  resumePersistedSessionRequestSchema,
  sessionTranscriptionFinalSchema,
  sessionTranscriptionGapSchema,
  sessionTranscriptionPartialSchema,
  startPersistedSessionRequestSchema,
  stopPersistedSessionRequestSchema,
  type PausePersistedSessionRequest,
  type PersistenceStatus,
  type ResumePersistedSessionRequest,
  type SessionTranscriptionFinal,
  type SessionTranscriptionGap,
  type SessionTranscriptionPartial,
  type StartPersistedSessionRequest,
  type StopPersistedSessionRequest,
} from "@/contracts/session-lifecycle"

export const SESSION_TRANSCRIPTION_PARTIAL_EVENT =
  "session-transcription-partial"
export const SESSION_TRANSCRIPTION_FINAL_EVENT = "session-transcription-final"
export const SESSION_TRANSCRIPTION_GAP_EVENT = "session-transcription-gap"
export const PERSISTENCE_STATUS_EVENT = "persistence-status"

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

export async function startPersistedSession(
  value: StartPersistedSessionRequest,
): Promise<Session> {
  return await call(
    "start_session",
    request(startPersistedSessionRequestSchema, value),
    sessionSchema,
  )
}

export async function pausePersistedSession(
  value: PausePersistedSessionRequest,
): Promise<Session> {
  return await call(
    "pause_session",
    request(pausePersistedSessionRequestSchema, value),
    sessionSchema,
  )
}

export async function resumePersistedSession(
  value: ResumePersistedSessionRequest,
): Promise<Session> {
  return await call(
    "resume_session",
    request(resumePersistedSessionRequestSchema, value),
    sessionSchema,
  )
}

export async function stopPersistedSession(
  value: StopPersistedSessionRequest,
): Promise<Session> {
  return await call(
    "stop_session",
    request(stopPersistedSessionRequestSchema, value),
    sessionSchema,
  )
}

export function listenToSessionTranscriptionPartials(
  onEvent: (event: SessionTranscriptionPartial) => void,
): Promise<UnlistenFn> {
  return listenParsed(
    SESSION_TRANSCRIPTION_PARTIAL_EVENT,
    sessionTranscriptionPartialSchema,
    onEvent,
  )
}

export function listenToSessionTranscriptionFinals(
  onEvent: (event: SessionTranscriptionFinal) => void,
): Promise<UnlistenFn> {
  return listenParsed(
    SESSION_TRANSCRIPTION_FINAL_EVENT,
    sessionTranscriptionFinalSchema,
    onEvent,
  )
}

export function listenToSessionTranscriptionGaps(
  onEvent: (event: SessionTranscriptionGap) => void,
): Promise<UnlistenFn> {
  return listenParsed(
    SESSION_TRANSCRIPTION_GAP_EVENT,
    sessionTranscriptionGapSchema,
    onEvent,
  )
}

export function listenToPersistenceStatus(
  onEvent: (event: PersistenceStatus) => void,
): Promise<UnlistenFn> {
  return listenParsed(
    PERSISTENCE_STATUS_EVENT,
    persistenceStatusSchema,
    onEvent,
  )
}

function listenParsed<Output>(
  event: string,
  schema: {
    safeParse(
      value: unknown,
    ): { success: true; data: Output } | { success: false }
  },
  onEvent: (event: Output) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(event, ({ payload }) => {
    const parsed = schema.safeParse(payload)
    if (parsed.success) onEvent(parsed.data)
  })
}
