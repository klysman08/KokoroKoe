import { useQuery } from "@tanstack/react-query"
import { ArrowRight, Radio } from "lucide-react"

import { Button } from "@/components/ui/button"
import { type ApplicationError } from "@/contracts/app-error"
import { type CredentialStatus } from "@/contracts/credentials"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { QuickStartCard } from "@/features/meeting/QuickStartCard"
import { RecentMeetings } from "@/features/meeting/RecentMeetings"
import {
  SetupChecklist,
  type SetupItem,
} from "@/features/meeting/SetupChecklist"
import { useProjectsQuery } from "@/features/projects/use-projects"
import { useModelsQuery } from "@/features/settings/use-model-management"
import { useSettingsQuery } from "@/features/settings/use-settings-query"
import { getOpenRouterCredentialStatus } from "@/lib/tauri/credentials"
import { useActiveSessionStore } from "@/stores/active-session-store"
import { useMeetingStore } from "@/stores/meeting-store"
import { useNavigationStore } from "@/stores/navigation-store"

/**
 * Home, for someone who opened the app to record a meeting.
 *
 * A running meeting takes precedence over everything else; otherwise the page
 * is the Start button, whatever setup is still missing, and the meetings that
 * have already happened. Projects, presets, and models live on their own pages.
 */
export function HomePage() {
  const settingsQuery = useSettingsQuery()
  const projectsQuery = useProjectsQuery()
  const modelsQuery = useModelsQuery()
  const credentialQuery = useQuery<CredentialStatus, ApplicationError>({
    // The same key Settings uses, so saving a key there updates this page.
    queryKey: ["openrouter-credential-status"],
    queryFn: getOpenRouterCredentialStatus,
    retry: false,
  })
  const active = useActiveSessionStore((state) => state.activeSession)
  const openMeeting = useMeetingStore((state) => state.open)
  const navigate = useNavigationStore((state) => state.navigate)
  const projects = projectsQuery.data?.pages.flatMap((page) => page.items) ?? []
  const settings = settingsQuery.data

  const defaultModel = modelsQuery.data?.find(
    (model) => model.descriptor.id === settings?.defaultTranscriptionModelId,
  )
  const transcriptionReady = defaultModel?.status === "installed"
  const setup: SetupItem[] = [
    {
      label: "Transcription model",
      done: transcriptionReady,
      hint: "download a Whisper model in Settings. Meetings cannot start without one.",
    },
    {
      label: "OpenRouter key",
      done: credentialQuery.data?.configured === true,
      hint: "optional. Add one to get insights during meetings and a summary after.",
    },
    {
      label: "Insights and summary models",
      done:
        settings?.defaultLlmModels.insights !== undefined &&
        settings.defaultLlmModels.summaries !== undefined,
      hint: "choose which OpenRouter models write insights and summaries.",
    },
  ]
  const loadingReadiness = modelsQuery.isPending || settingsQuery.isPending

  return (
    <div className="space-y-8">
      {active && (
        <section className="bg-primary text-primary-foreground flex flex-col gap-3 rounded-2xl p-5 md:flex-row md:items-center md:justify-between">
          <div className="flex items-center gap-3">
            <Radio aria-hidden="true" className="size-5 shrink-0" />
            <div>
              <p className="text-sm font-semibold">
                {active.state === "paused"
                  ? "A meeting is paused"
                  : "A meeting is live"}
              </p>
              <p className="text-sm opacity-90">{active.title}</p>
            </div>
          </div>
          <Button
            onClick={() => {
              openMeeting({ projectId: active.projectId, sessionId: active.id })
              navigate("meeting")
            }}
            variant="secondary"
          >
            Return to meeting <ArrowRight data-icon="inline-end" />
          </Button>
        </section>
      )}

      {!active && (
        <QuickStartCard
          blockedReason={
            loadingReadiness || transcriptionReady
              ? undefined
              : "Download a transcription model in Settings before your first meeting."
          }
          projects={projects}
          settings={settings}
        />
      )}

      {!loadingReadiness && <SetupChecklist items={setup} />}

      {settingsQuery.error && (
        <SanitizedErrorPanel error={settingsQuery.error} />
      )}
      {projectsQuery.error && (
        <SanitizedErrorPanel error={projectsQuery.error} />
      )}

      <RecentMeetings projects={projects} />
    </div>
  )
}
