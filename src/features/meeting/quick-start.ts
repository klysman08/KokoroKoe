import { type AudioDevice, type AudioDeviceList } from "@/contracts/audio"
import { presetSnapshotOf, type Preset } from "@/contracts/presets"
import {
  createProjectInputSchema,
  createSessionInputSchema,
  type CreateProjectInput,
  type CreateSessionInput,
  type Project,
} from "@/contracts/projects"
import { type AppSettings } from "@/contracts/settings"

/** The project a meeting lands in when the user has not made one. */
export const GENERAL_PROJECT_NAME = "General"

/**
 * Languages offered on Home. Whisper is told the language rather than guessing
 * it, so this list is the ones a casual user is most likely to speak; the full
 * session form still accepts any supported tag.
 */
export const MEETING_LANGUAGES = [
  { tag: "pt-BR", label: "Português" },
  { tag: "en-US", label: "English" },
  { tag: "es-ES", label: "Español" },
  { tag: "fr-FR", label: "Français" },
  { tag: "de-DE", label: "Deutsch" },
  { tag: "it-IT", label: "Italiano" },
  { tag: "ja-JP", label: "日本語" },
] as const

export type MeetingLanguage = (typeof MEETING_LANGUAGES)[number]["tag"]

/** The offered language closest to the system's, or English. */
export function defaultMeetingLanguage(
  systemLanguage: string = globalThis.navigator?.language ?? "en-US",
): MeetingLanguage {
  const primary = systemLanguage.split("-")[0]?.toLowerCase()
  return (
    MEETING_LANGUAGES.find((language) =>
      language.tag.toLowerCase().startsWith(`${primary}-`),
    )?.tag ?? "en-US"
  )
}

export function defaultMeetingTitle(now: Date = new Date()) {
  return `Meeting · ${new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(now)}`
}

export function generalProjectInput(settings: AppSettings): CreateProjectInput {
  return createProjectInputSchema.parse({
    name: GENERAL_PROJECT_NAME,
    description: "Meetings started from Home.",
    globalContext: "",
    participants: [],
    tags: [],
    defaultPresetId: settings.defaultPresetId,
    defaultTranscriptionModelId: settings.defaultTranscriptionModelId,
    preferredLlmModels: {},
  })
}

/**
 * The device a meeting should capture from: Windows' default for that role,
 * recorded as a default selection so the session follows the default if the
 * user switches headsets before pressing Start.
 */
function defaultDevice(
  devices: AudioDevice[],
  role: "communications" | "console",
) {
  const active = devices.filter((device) => device.state === "active")
  const chosen =
    active.find((device) =>
      role === "communications"
        ? device.isDefaultCommunications
        : device.isDefaultConsole,
    ) ??
    active.find((device) => device.isDefaultConsole) ??
    active[0]
  if (!chosen) return undefined
  return {
    endpointId: chosen.endpointId,
    friendlyName: chosen.friendlyName,
    selection: { kind: "default" as const, role },
    nativeSampleRate: chosen.sampleRate,
    nativeChannels: chosen.channels,
  }
}

export type QuickSessionResult =
  | { ok: true; input: CreateSessionInput }
  | { ok: false; reason: "no_microphone" | "no_output" | "invalid" }

/**
 * A complete session from the user's saved defaults and three choices.
 *
 * The transcription model is the one Settings currently names rather than the
 * project's, because Settings is where the user downloads and picks a model;
 * a project made weeks ago would otherwise start meetings on a model they have
 * since removed.
 */
export function quickSessionInput({
  title,
  language,
  preset,
  project,
  settings,
  devices,
}: {
  title: string
  language: string
  preset: Preset
  project: Project
  settings: AppSettings
  devices: AudioDeviceList
}): QuickSessionResult {
  const microphone = defaultDevice(devices.inputs, "communications")
  if (!microphone) return { ok: false, reason: "no_microphone" }
  const systemOutput = defaultDevice(devices.outputs, "console")
  if (!systemOutput) return { ok: false, reason: "no_output" }
  const parsed = createSessionInputSchema.safeParse({
    title: title.trim() || defaultMeetingTitle(),
    objective: "",
    sessionContext: "",
    preset: presetSnapshotOf(preset),
    language,
    microphone,
    systemOutput,
    transcriptionModelId: settings.defaultTranscriptionModelId,
    llmModels: {
      insights:
        project.preferredLlmModels.insights ??
        settings.defaultLlmModels.insights,
      summaries:
        project.preferredLlmModels.summaries ??
        settings.defaultLlmModels.summaries,
      manualQuestions:
        project.preferredLlmModels.manualQuestions ??
        settings.defaultLlmModels.manualQuestions,
    },
    spendingLimitUsd: settings.defaultSessionBudgetUsd,
    maxTokensPerRequest: settings.maxTokensPerRequest,
    retainAudio: settings.retainAudioByDefault,
  })
  return parsed.success
    ? { ok: true, input: parsed.data }
    : { ok: false, reason: "invalid" }
}
