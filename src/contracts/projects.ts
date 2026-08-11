import { z } from "zod"

import { deviceSelectionSchema } from "@/contracts/audio"
import { presetIdSchema } from "@/contracts/settings"

const safeCounterSchema = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER)
const timestampSchema = z.iso.datetime({ offset: true })
const fixedDecimalSchema = z
  .string()
  .regex(/^\d+\.\d{2}$/)
  .max(18)
const singleLine = (maximum: number) =>
  z
    .string()
    .min(1)
    .max(maximum)
    .refine((value) =>
      [...value].every((character) => {
        const code = character.codePointAt(0) ?? 0
        return code > 31 && code !== 127
      }),
    )
const multiline = (maximum: number) =>
  z
    .string()
    .max(maximum)
    .refine((value) =>
      [...value].every((character) => {
        const code = character.codePointAt(0) ?? 0
        return (
          code >= 32 ||
          character === "\n" ||
          character === "\r" ||
          character === "\t"
        )
      }),
    )
const uniqueList = (maximumItems: number, maximumText: number) =>
  z
    .array(multiline(maximumText).min(1))
    .max(maximumItems)
    .refine((values) => new Set(values).size === values.length)

export const projectIdSchema = z.uuid().brand<"ProjectId">()
export const sessionIdSchema = z.uuid().brand<"SessionId">()

export const llmRoleModelsSchema = z.strictObject({
  insights: singleLine(256).optional(),
  summaries: singleLine(256).optional(),
  manualQuestions: singleLine(256).optional(),
})

export const insightTypeSchema = z.enum([
  "suggested_response",
  "follow_up_question",
  "clarification",
  "fact_or_number",
  "risk",
  "objection",
  "decision",
  "action_item",
  "contradiction",
  "unaddressed_topic",
])

export const presetSnapshotSchema = z
  .strictObject({
    id: presetIdSchema,
    version: z.number().int().positive().max(0xffff_ffff),
    name: singleLine(128),
    assistantRole: multiline(4096).min(1),
    analysisObjectives: uniqueList(32, 1024).min(1),
    insightTypes: z.array(insightTypeSchema).min(1).max(16),
    responseTone: multiline(1024).min(1),
    finalSummarySections: uniqueList(32, 128).min(1),
    highlightInstructions: uniqueList(32, 1024),
    prohibitedBehaviors: uniqueList(32, 1024),
  })
  .refine(
    (value) => new Set(value.insightTypes).size === value.insightTypes.length,
  )

export const audioDeviceSnapshotSchema = z
  .strictObject({
    endpointId: singleLine(1024),
    friendlyName: singleLine(512),
    selection: deviceSelectionSchema,
    nativeSampleRate: z.number().int().positive().max(0xffff_ffff).optional(),
    nativeChannels: z.number().int().positive().max(0xffff).optional(),
  })
  .superRefine((value, context) => {
    if (
      value.selection.kind === "fixed" &&
      value.selection.endpointId !== value.endpointId
    ) {
      context.addIssue({
        code: "custom",
        message: "A fixed device snapshot must preserve its selected endpoint.",
      })
    }
  })

export const channelHealthSchema = z.strictObject({
  status: z.enum([
    "starting",
    "active",
    "silent",
    "reconnecting",
    "unavailable",
    "stopped",
  ]),
  endpointId: singleLine(1024).optional(),
  detailCode: singleLine(128).optional(),
  updatedAt: timestampSchema,
})

export const usageAggregateSchema = z.strictObject({
  inputTokens: safeCounterSchema,
  outputTokens: safeCounterSchema,
  estimatedCostUsd: fixedDecimalSchema,
  actualCostUsd: fixedDecimalSchema,
})

const llmRoleFields = llmRoleModelsSchema
const projectFolder = z
  .string()
  .min(11)
  .max(80)
  .regex(/^[a-z0-9]+(?:-[a-z0-9]+)*--[0-9a-f]{8}$/)
const sessionFolder = z
  .string()
  .min(22)
  .max(80)
  .regex(/^\d{4}-\d{2}-\d{2}-[a-z0-9]+(?:-[a-z0-9]+)*--[0-9a-f]{8}$/)

export const projectSchema = z
  .strictObject({
    schemaVersion: z.literal(1),
    id: projectIdSchema,
    name: singleLine(128),
    folderName: projectFolder,
    description: multiline(4096),
    globalContext: multiline(32768),
    participants: uniqueList(64, 128),
    tags: uniqueList(64, 64),
    defaultPresetId: presetIdSchema,
    defaultTranscriptionModelId: singleLine(128),
    preferredLlmModels: llmRoleFields,
    createdAt: timestampSchema,
    updatedAt: timestampSchema,
    revision: safeCounterSchema,
  })
  .superRefine((value, context) => {
    if (
      !value.folderName.endsWith(
        `--${value.id.replaceAll("-", "").slice(0, 8)}`,
      )
    ) {
      context.addIssue({
        code: "custom",
        message: "Project folder identity differs.",
      })
    }
    if (Date.parse(value.updatedAt) < Date.parse(value.createdAt)) {
      context.addIssue({
        code: "custom",
        message: "Project timestamps are reversed.",
      })
    }
  })

const createProjectFields = {
  name: singleLine(128),
  description: multiline(4096),
  globalContext: multiline(32768),
  participants: uniqueList(64, 128),
  tags: uniqueList(64, 64),
  defaultPresetId: presetIdSchema,
  defaultTranscriptionModelId: singleLine(128),
  preferredLlmModels: llmRoleModelsSchema,
} as const

export const createProjectInputSchema = z.strictObject(createProjectFields)
export const updateProjectInputSchema = z
  .strictObject(createProjectFields)
  .partial()
  .refine((value) => Object.values(value).some((field) => field !== undefined))

const projectCursorSchema = z
  .string()
  .min(1)
  .max(128)
  .regex(/^p1:\d+:\d+$/)

export const projectPageRequestSchema = z.strictObject({
  cursor: projectCursorSchema.optional(),
  limit: z.number().int().min(1).max(100),
})

export const projectPageSchema = z.strictObject({
  items: z.array(projectSchema).max(100),
  nextCursor: projectCursorSchema.optional(),
})

export const projectUpdateRequestSchema = z.strictObject({
  projectId: projectIdSchema,
  expectedRevision: safeCounterSchema,
  value: updateProjectInputSchema,
})

export const projectManagementFixtureSchema = z.strictObject({
  pageRequest: projectPageRequestSchema,
  page: projectPageSchema,
  createInput: createProjectInputSchema,
  updateRequest: projectUpdateRequestSchema,
})

export const sessionStateSchema = z.enum([
  "idle",
  "preparing",
  "capturing",
  "transcribing",
  "paused",
  "stopping",
  "processing_summary",
  "completed",
  "failed",
])

export const sessionSchema = z
  .strictObject({
    schemaVersion: z.literal(1),
    id: sessionIdSchema,
    projectId: projectIdSchema,
    folderName: sessionFolder,
    title: singleLine(256),
    objective: multiline(4096),
    sessionContext: multiline(32768),
    preset: presetSnapshotSchema,
    language: z
      .string()
      .min(2)
      .max(64)
      .regex(/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8}){0,7}$/),
    microphone: audioDeviceSnapshotSchema,
    systemOutput: audioDeviceSnapshotSchema,
    transcriptionEngine: z.literal("whisper"),
    transcriptionModelId: singleLine(128),
    llmModels: llmRoleFields,
    retainAudio: z.boolean(),
    state: sessionStateSchema,
    channelHealth: z.strictObject({
      microphone: channelHealthSchema,
      systemOutput: channelHealthSchema,
    }),
    summaryStatus: z.enum([
      "not_requested",
      "pending",
      "completed",
      "deferred",
      "failed",
    ]),
    usage: usageAggregateSchema,
    createdAt: timestampSchema,
    startedAt: timestampSchema.optional(),
    endedAt: timestampSchema.optional(),
    updatedAt: timestampSchema,
    revision: safeCounterSchema,
  })
  .superRefine((value, context) => {
    const suffix = value.id.replaceAll("-", "").slice(0, 8)
    if (
      !value.folderName.startsWith(`${value.createdAt.slice(0, 10)}-`) ||
      !value.folderName.endsWith(`--${suffix}`)
    ) {
      context.addIssue({
        code: "custom",
        message: "Session folder identity differs.",
      })
    }
    const created = Date.parse(value.createdAt)
    const started =
      value.startedAt === undefined ? created : Date.parse(value.startedAt)
    const ended =
      value.endedAt === undefined ? undefined : Date.parse(value.endedAt)
    if (
      Date.parse(value.updatedAt) < created ||
      started < created ||
      (ended !== undefined && ended < started)
    ) {
      context.addIssue({
        code: "custom",
        message: "Session timestamps are reversed.",
      })
    }
    if (value.state === "completed" && value.endedAt === undefined) {
      context.addIssue({
        code: "custom",
        message: "Completed lifecycle is inconsistent.",
      })
    }
  })

const sessionEditableFields = {
  title: singleLine(256),
  objective: multiline(4096),
  sessionContext: multiline(32768),
  preset: presetSnapshotSchema,
  language: z
    .string()
    .min(2)
    .max(64)
    .regex(/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8}){0,7}$/),
  microphone: audioDeviceSnapshotSchema,
  systemOutput: audioDeviceSnapshotSchema,
  transcriptionModelId: singleLine(128),
  llmModels: llmRoleModelsSchema,
  retainAudio: z.boolean(),
} as const

export const createSessionInputSchema = z.strictObject(sessionEditableFields)
export const updateSessionInputSchema = z
  .strictObject(sessionEditableFields)
  .partial()
  .refine((value) => Object.values(value).some((field) => field !== undefined))

const sessionCursorSchema = z
  .string()
  .min(1)
  .max(160)
  .regex(/^s1:\d+:[0-9a-f-]{36}:\d+$/)

export const sessionPageRequestSchema = z.strictObject({
  projectId: projectIdSchema,
  cursor: sessionCursorSchema.optional(),
  limit: z.number().int().min(1).max(100),
})

export const sessionPageSchema = z.strictObject({
  items: z.array(sessionSchema).max(100),
  nextCursor: sessionCursorSchema.optional(),
})

export const sessionIdentitySchema = z.strictObject({
  projectId: projectIdSchema,
  sessionId: sessionIdSchema,
})

export const sessionCreateRequestSchema = z.strictObject({
  projectId: projectIdSchema,
  value: createSessionInputSchema,
})

export const sessionUpdateRequestSchema = z.strictObject({
  projectId: projectIdSchema,
  sessionId: sessionIdSchema,
  expectedRevision: safeCounterSchema,
  value: updateSessionInputSchema,
})

export const sessionManagementFixtureSchema = z.strictObject({
  pageRequest: sessionPageRequestSchema,
  page: sessionPageSchema,
  createRequest: sessionCreateRequestSchema,
  updateRequest: sessionUpdateRequestSchema,
})

const portablePath = z
  .string()
  .min(1)
  .max(1024)
  .regex(/^projects\/[a-z0-9./-]+$/)
  .refine((value) =>
    value
      .split("/")
      .every((part) => /^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$/.test(part)),
  )

export const portableFolderContractSchema = z.strictObject({
  projectDirectory: portablePath,
  projectDocument: portablePath,
  presetsDirectory: portablePath,
  sessionsDirectory: portablePath,
  sessionDirectory: portablePath,
  sessionDocument: portablePath,
  transcriptDocument: portablePath,
  summaryDocument: portablePath,
  insightsDocument: portablePath,
  actionsDocument: portablePath,
  questionsDocument: portablePath,
  recoveryJournal: portablePath,
  audioDirectory: portablePath,
})

export const projectSessionFixtureSchema = z
  .strictObject({
    project: projectSchema,
    session: sessionSchema,
    layout: portableFolderContractSchema,
  })
  .superRefine((value, context) => {
    if (value.session.projectId !== value.project.id) {
      context.addIssue({
        code: "custom",
        message: "Session project identity differs.",
      })
    }
    const projectRoot = `projects/${value.project.folderName}`
    const sessionRoot = `${projectRoot}/sessions/${value.session.folderName}`
    const expected = {
      projectDirectory: projectRoot,
      projectDocument: `${projectRoot}/project.md`,
      presetsDirectory: `${projectRoot}/presets`,
      sessionsDirectory: `${projectRoot}/sessions`,
      sessionDirectory: sessionRoot,
      sessionDocument: `${sessionRoot}/session.md`,
      transcriptDocument: `${sessionRoot}/transcript.md`,
      summaryDocument: `${sessionRoot}/summary.md`,
      insightsDocument: `${sessionRoot}/insights.md`,
      actionsDocument: `${sessionRoot}/actions.md`,
      questionsDocument: `${sessionRoot}/questions.md`,
      recoveryJournal: `${sessionRoot}/recovery.journal`,
      audioDirectory: `${sessionRoot}/audio`,
    }
    for (const key of Object.keys(expected) as (keyof typeof expected)[]) {
      if (value.layout[key] !== expected[key]) {
        context.addIssue({
          code: "custom",
          path: ["layout", key],
          message: "Layout differs.",
        })
      }
    }
  })

export type ProjectId = z.infer<typeof projectIdSchema>
export type SessionId = z.infer<typeof sessionIdSchema>
export type Project = z.infer<typeof projectSchema>
export type CreateProjectInput = z.infer<typeof createProjectInputSchema>
export type UpdateProjectInput = z.infer<typeof updateProjectInputSchema>
export type ProjectPageRequest = z.infer<typeof projectPageRequestSchema>
export type ProjectPage = z.infer<typeof projectPageSchema>
export type ProjectUpdateRequest = z.infer<typeof projectUpdateRequestSchema>
export type Session = z.infer<typeof sessionSchema>
export type CreateSessionInput = z.infer<typeof createSessionInputSchema>
export type UpdateSessionInput = z.infer<typeof updateSessionInputSchema>
export type SessionPageRequest = z.infer<typeof sessionPageRequestSchema>
export type SessionPage = z.infer<typeof sessionPageSchema>
export type SessionIdentity = z.infer<typeof sessionIdentitySchema>
export type SessionCreateRequest = z.infer<typeof sessionCreateRequestSchema>
export type SessionUpdateRequest = z.infer<typeof sessionUpdateRequestSchema>
export type PortableFolderContract = z.infer<
  typeof portableFolderContractSchema
>
