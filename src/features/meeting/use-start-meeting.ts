import { useMutation, useQueryClient } from "@tanstack/react-query"

import {
  createLocalApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import { requestIdSchema } from "@/contracts/models"
import { type Project, type Session } from "@/contracts/projects"
import { type Preset } from "@/contracts/presets"
import { type AppSettings } from "@/contracts/settings"
import { projectsQueryKey } from "@/features/projects/use-projects"
import { sessionsQueryKey } from "@/features/sessions/use-sessions"
import { settingsQueryKey } from "@/features/settings/use-settings-query"
import { listAudioDevices } from "@/lib/tauri/audio"
import { listPresets } from "@/lib/tauri/presets"
import { createProject } from "@/lib/tauri/projects"
import { createSession, startPersistedSession } from "@/lib/tauri/sessions"
import { updateSettings } from "@/lib/tauri/settings"
import { useActiveSessionStore } from "@/stores/active-session-store"
import { useMeetingStore } from "@/stores/meeting-store"
import { useNavigationStore } from "@/stores/navigation-store"

import { generalProjectInput, quickSessionInput } from "./quick-start"

export type StartMeetingChoices = {
  /** An existing project, or none to use (and if needed create) General. */
  project: Project | undefined
  title: string
  language: string
  presetId: string | undefined
}

/**
 * Starts a meeting in one step: project, session, and capture.
 *
 * Every step is an existing command, so nothing here widens what the main
 * window may do. Pressing Start is the capture consent the session command
 * requires — the button sits under the consent notice and says what it does.
 * A session that was created but could not start stays in its project, idle,
 * and the Meeting view offers to start it again.
 */
export function useStartMeeting(settings: AppSettings | undefined) {
  const queryClient = useQueryClient()
  return useMutation<
    { project: Project; session: Session; preset: Preset; language: string },
    ApplicationError,
    StartMeetingChoices
  >({
    mutationFn: async ({ project: chosen, title, language, presetId }) => {
      if (!settings) {
        throw createLocalApplicationError(
          "meeting_settings_unavailable",
          "Settings are still loading. Try again in a moment.",
        )
      }
      const project =
        chosen ?? (await createProject(generalProjectInput(settings)))
      const [devices, presets] = await Promise.all([
        listAudioDevices(),
        listPresets(project.id),
      ])
      const preset =
        presets.find((candidate) => candidate.id === presetId) ??
        presets.find((candidate) => candidate.id === project.defaultPresetId) ??
        presets[0]
      if (!preset) {
        throw createLocalApplicationError(
          "meeting_preset_unavailable",
          "No meeting type is available. Check the presets in this project.",
        )
      }
      const built = quickSessionInput({
        title,
        language,
        preset,
        project,
        settings,
        devices,
      })
      if (!built.ok) {
        throw createLocalApplicationError(
          `meeting_${built.reason}`,
          built.reason === "no_microphone"
            ? "No microphone is available. Connect one and try again."
            : built.reason === "no_output"
              ? "No speaker or headset output is available. Connect one and try again."
              : "Your saved defaults are incomplete. Check Settings and try again.",
        )
      }
      const created = await createSession({
        projectId: project.id,
        value: built.input,
      })
      useMeetingStore
        .getState()
        .open({ projectId: project.id, sessionId: created.id })
      const session = await startPersistedSession({
        projectId: project.id,
        sessionId: created.id,
        expectedRevision: created.revision,
        requestId: requestIdSchema.parse(globalThis.crypto.randomUUID()),
        acknowledgedCaptureConsent: true,
      })
      return { project, session, preset, language }
    },
    retry: false,
    gcTime: 0,
    onSuccess: ({ session, preset, language }) => {
      useActiveSessionStore.getState().syncSession(session)
      useNavigationStore.getState().navigate("meeting")
      void rememberChoices(settings, preset, language).then((saved) => {
        if (saved) queryClient.setQueryData(settingsQueryKey, saved)
      })
    },
    onSettled: async (result) => {
      await queryClient.invalidateQueries({ queryKey: projectsQueryKey })
      if (result) {
        await queryClient.invalidateQueries({
          queryKey: sessionsQueryKey(result.project.id),
        })
      }
      await queryClient.invalidateQueries({ queryKey: ["recent-meetings"] })
    },
  })
}

/**
 * Makes the language and meeting type just used the ones Home offers next.
 *
 * Only a built-in meeting type is remembered: it is saved as the default
 * preset, which new projects also start from, and a preset that lives in one
 * project means nothing to another. Remembering is a convenience, so a failure
 * — such as Settings changing in another view at the same moment — is ignored.
 */
async function rememberChoices(
  settings: AppSettings | undefined,
  preset: Preset,
  language: string,
): Promise<AppSettings | undefined> {
  if (!settings) return undefined
  const value = {
    ...(preset.builtIn && preset.id !== settings.defaultPresetId
      ? { defaultPresetId: preset.id }
      : {}),
    ...(language !== settings.defaultSessionLanguage
      ? { defaultSessionLanguage: language }
      : {}),
  }
  if (Object.keys(value).length === 0) return undefined
  try {
    return await updateSettings({
      expectedRevision: settings.revision,
      value,
    })
  } catch {
    return undefined
  }
}
