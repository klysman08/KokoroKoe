import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { vi } from "vitest"

import fixture from "../../../fixtures/contracts/session-management-v1.json"
import sessionFixture from "../../../fixtures/contracts/project-session-v1.json"
import lifecycleFixture from "../../../fixtures/contracts/session-lifecycle-v1.json"
import {
  sessionManagementFixtureSchema,
  sessionSchema,
} from "@/contracts/projects"
import { sessionLifecycleFixtureSchema } from "@/contracts/session-lifecycle"
import {
  createSession,
  getSession,
  listSessions,
  listenToPersistenceStatus,
  pausePersistedSession,
  resumePersistedSession,
  startPersistedSession,
  stopPersistedSession,
  updateSession,
} from "@/lib/tauri/sessions"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))
const invokeMock = vi.mocked(invoke)
const parsed = sessionManagementFixtureSchema.parse(fixture)
const session = sessionSchema.parse(sessionFixture.session)
const lifecycle = sessionLifecycleFixtureSchema.parse(lifecycleFixture)

beforeEach(() => invokeMock.mockReset())

it("uses the four exact session commands and bounded camel-case arguments", async () => {
  invokeMock
    .mockResolvedValueOnce(parsed.page)
    .mockResolvedValueOnce(session)
    .mockResolvedValueOnce(session)
    .mockResolvedValueOnce(session)

  await listSessions(parsed.pageRequest)
  await getSession({
    projectId: parsed.pageRequest.projectId,
    sessionId: session.id,
  })
  await createSession(parsed.createRequest)
  await updateSession(parsed.updateRequest)

  expect(invokeMock.mock.calls).toEqual([
    ["list_sessions", parsed.pageRequest],
    [
      "get_session",
      { projectId: parsed.pageRequest.projectId, sessionId: session.id },
    ],
    [
      "create_session",
      {
        projectId: parsed.createRequest.projectId,
        ...parsed.createRequest.value,
      },
    ],
    ["update_session", parsed.updateRequest],
  ])
})

it("rejects malformed session requests and responses", async () => {
  await expect(
    listSessions({ projectId: parsed.pageRequest.projectId, limit: 0 }),
  ).rejects.toMatchObject({ details: { code: "invalid_request_contract" } })
  expect(invokeMock).not.toHaveBeenCalled()
  invokeMock.mockResolvedValue({ items: [{ unsafe: true }] })
  await expect(listSessions(parsed.pageRequest)).rejects.toMatchObject({
    details: { code: "invalid_backend_contract" },
  })
})

it("uses the four exact persisted lifecycle commands", async () => {
  invokeMock.mockResolvedValue(session)
  await startPersistedSession(lifecycle.startRequest)
  await pausePersistedSession(lifecycle.pauseRequest)
  await resumePersistedSession(lifecycle.resumeRequest)
  await stopPersistedSession(lifecycle.stopRequest)
  expect(invokeMock.mock.calls).toEqual([
    ["start_session", lifecycleFixture.startRequest],
    ["pause_session", lifecycleFixture.pauseRequest],
    ["resume_session", lifecycleFixture.resumeRequest],
    ["stop_session", lifecycleFixture.stopRequest],
  ])
})

it("drops malformed persistence events at the Tauri boundary", async () => {
  let handler: ((event: { payload: unknown }) => void) | undefined
  vi.mocked(listen).mockImplementation(async (_event, callback) => {
    handler = callback as (event: { payload: unknown }) => void
    return () => undefined
  })
  const received = vi.fn()
  await listenToPersistenceStatus(received)
  handler?.({ payload: { ...lifecycleFixture.persistenceStatus, extra: true } })
  handler?.({ payload: lifecycleFixture.persistenceStatus })
  expect(received).toHaveBeenCalledOnce()
  expect(received).toHaveBeenCalledWith(lifecycleFixture.persistenceStatus)
})
