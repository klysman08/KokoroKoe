import { invoke } from "@tauri-apps/api/core"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  createPresetInputSchema,
  presetExportSchema,
  presetIdSchema,
  presetListSchema,
  presetSchema,
  updatePresetInputSchema,
  type CreatePresetInput,
  type Preset,
  type PresetExport,
  type UpdatePresetInput,
} from "@/contracts/presets"
import { projectIdSchema } from "@/contracts/projects"

async function call<T>(
  command: string,
  args: Record<string, unknown> | undefined,
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

function validScope(projectId: string, presetId?: string) {
  const project = projectIdSchema.safeParse(projectId)
  const preset =
    presetId === undefined ? undefined : presetIdSchema.safeParse(presetId)
  if (!project.success || preset?.success === false) {
    throw createRequestContractApplicationError()
  }
  return { projectId: project.data, presetId: preset?.data }
}

function validRevision(expectedRevision: number) {
  if (!Number.isSafeInteger(expectedRevision) || expectedRevision < 1) {
    throw createRequestContractApplicationError()
  }
  return expectedRevision
}

// Without a project this is the built-in catalog alone, which is all the
// project-creation form can offer: no project exists yet to own a custom preset.
export async function listPresets(projectId?: string) {
  const args =
    projectId === undefined
      ? undefined
      : { projectId: validScope(projectId).projectId }
  return call<Preset[]>("list_presets", args, presetListSchema)
}

export async function createPreset(
  projectId: string,
  value: CreatePresetInput,
) {
  const parsed = createPresetInputSchema.safeParse(value)
  if (!parsed.success) throw createRequestContractApplicationError()
  return call<Preset>(
    "create_preset",
    { projectId: validScope(projectId).projectId, value: parsed.data },
    presetSchema,
  )
}

export async function updatePreset(
  projectId: string,
  presetId: string,
  expectedRevision: number,
  value: UpdatePresetInput,
) {
  const parsed = updatePresetInputSchema.safeParse(value)
  if (!parsed.success) throw createRequestContractApplicationError()
  const scope = validScope(projectId, presetId)
  return call<Preset>(
    "update_preset",
    {
      projectId: scope.projectId,
      presetId: scope.presetId,
      expectedRevision: validRevision(expectedRevision),
      value: parsed.data,
    },
    presetSchema,
  )
}

export async function duplicatePreset(
  projectId: string,
  presetId: string,
  name: string,
) {
  const parsed = createPresetInputSchema.shape.name.safeParse(name)
  if (!parsed.success) throw createRequestContractApplicationError()
  const scope = validScope(projectId, presetId)
  return call<Preset>(
    "duplicate_preset",
    { projectId: scope.projectId, presetId: scope.presetId, name: parsed.data },
    presetSchema,
  )
}

export async function deleteCustomPreset(
  projectId: string,
  presetId: string,
  expectedRevision: number,
) {
  const scope = validScope(projectId, presetId)
  const revision = validRevision(expectedRevision)
  try {
    await invoke<unknown>("delete_custom_preset", {
      projectId: scope.projectId,
      presetId: scope.presetId,
      expectedRevision: revision,
    })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
}

// The destination is chosen in a Rust-owned save dialog, so no path is sent.
export async function exportPreset(projectId: string, presetId: string) {
  const scope = validScope(projectId, presetId)
  return call<PresetExport>(
    "export_preset",
    { projectId: scope.projectId, presetId: scope.presetId },
    presetExportSchema,
  )
}
