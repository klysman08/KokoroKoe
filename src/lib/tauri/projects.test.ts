import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import fixture from "../../../fixtures/contracts/project-management-v1.json"

import { projectManagementFixtureSchema } from "@/contracts/projects"
import {
  createProject,
  getProject,
  listProjects,
  updateProject,
} from "@/lib/tauri/projects"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const parsedFixture = projectManagementFixtureSchema.parse(fixture)

beforeEach(() => invokeMock.mockReset())

it("uses the four exact project commands and camel-case arguments", async () => {
  invokeMock
    .mockResolvedValueOnce(parsedFixture.page)
    .mockResolvedValueOnce(parsedFixture.page.items[0])
    .mockResolvedValueOnce(parsedFixture.page.items[0])
    .mockResolvedValueOnce(parsedFixture.page.items[0])

  await listProjects(parsedFixture.pageRequest)
  await getProject(parsedFixture.page.items[0]!.id)
  await createProject(parsedFixture.createInput)
  await updateProject(parsedFixture.updateRequest)

  expect(invokeMock.mock.calls).toEqual([
    ["list_projects", parsedFixture.pageRequest],
    ["get_project", { projectId: parsedFixture.page.items[0]!.id }],
    ["create_project", parsedFixture.createInput],
    ["update_project", parsedFixture.updateRequest],
  ])
})

it("rejects malformed requests and responses at the adapter boundary", async () => {
  await expect(listProjects({ limit: 0 })).rejects.toMatchObject({
    details: { code: "invalid_request_contract" },
  })
  expect(invokeMock).not.toHaveBeenCalled()

  invokeMock.mockResolvedValue({ items: [{ unsafe: true }] })
  await expect(listProjects({ limit: 24 })).rejects.toMatchObject({
    details: { code: "invalid_backend_contract" },
  })
})
