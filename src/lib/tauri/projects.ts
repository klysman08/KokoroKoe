import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  createProjectInputSchema,
  projectIdSchema,
  projectPageRequestSchema,
  projectPageSchema,
  projectSchema,
  projectUpdateRequestSchema,
  type CreateProjectInput,
  type Project,
  type ProjectId,
  type ProjectPage,
  type ProjectPageRequest,
  type ProjectUpdateRequest,
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

export async function listProjects(
  value: ProjectPageRequest,
): Promise<ProjectPage> {
  return await call(
    "list_projects",
    request(projectPageRequestSchema, value),
    projectPageSchema,
  )
}

export async function getProject(projectId: ProjectId): Promise<Project> {
  return await call(
    "get_project",
    { projectId: request(projectIdSchema, projectId) },
    projectSchema,
  )
}

export async function createProject(
  value: CreateProjectInput,
): Promise<Project> {
  return await call(
    "create_project",
    request(createProjectInputSchema, value),
    projectSchema,
  )
}

export async function updateProject(
  value: ProjectUpdateRequest,
): Promise<Project> {
  return await call(
    "update_project",
    request(projectUpdateRequestSchema, value),
    projectSchema,
  )
}
