import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import fixture from "../../../fixtures/contracts/session-management-v1.json"
import sessionFixture from "../../../fixtures/contracts/project-session-v1.json"
import {
  sessionManagementFixtureSchema,
  sessionSchema,
} from "@/contracts/projects"
import {
  createSession,
  getSession,
  listSessions,
  updateSession,
} from "@/lib/tauri/sessions"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
const invokeMock = vi.mocked(invoke)
const parsed = sessionManagementFixtureSchema.parse(fixture)
const session = sessionSchema.parse(sessionFixture.session)

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
