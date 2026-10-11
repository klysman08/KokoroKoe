import { ChevronDown, Mic } from "lucide-react"
import { useState } from "react"

import { Button } from "@/components/ui/button"
import { type Project } from "@/contracts/projects"
import { type AppSettings } from "@/contracts/settings"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { usePresetsQuery } from "@/features/presets/use-presets"
import { cn } from "@/lib/utils"

import {
  GENERAL_PROJECT_NAME,
  MEETING_LANGUAGES,
  defaultMeetingLanguage,
  defaultMeetingTitle,
} from "./quick-start"
import { useStartMeeting } from "./use-start-meeting"

const fieldClass =
  "border-input bg-background focus-visible:ring-ring w-full rounded-md border px-3 py-2 text-sm outline-none focus-visible:ring-2 disabled:opacity-60"

/**
 * One button to a running meeting.
 *
 * Everything a meeting needs already has a saved default, so the only thing the
 * user must do is press Start. Title, language, and meeting type are offered
 * because they change the result; the project is tucked away because most
 * people never need more than one.
 */
export function QuickStartCard({
  settings,
  projects,
  blockedReason,
}: {
  settings: AppSettings | undefined
  projects: Project[]
  /** Why a meeting cannot start yet, such as no transcription model. */
  blockedReason: string | undefined
}) {
  const general = projects.find(
    (project) => project.name === GENERAL_PROJECT_NAME,
  )
  const [title, setTitle] = useState("")
  // Undefined until the user picks: until then, what they chose last time.
  const [language, setLanguage] = useState<string>()
  // Empty means General: the existing one once projects load, or a new one.
  const [projectId, setProjectId] = useState<string>("")
  const [presetId, setPresetId] = useState<string>()
  const [moreOptions, setMoreOptions] = useState(false)
  const project =
    projects.find((candidate) => candidate.id === projectId) ??
    (projectId === "" ? general : undefined)
  const presetsQuery = usePresetsQuery(project?.id)
  const start = useStartMeeting(settings)
  const offered = (id: string | undefined) =>
    presetsQuery.data?.some((preset) => preset.id === id) ? id : undefined
  // The meeting type last started from Home is remembered as the default
  // preset, so it wins over the project's own default.
  const chosenPresetId =
    presetId ??
    offered(settings?.defaultPresetId) ??
    offered(project?.defaultPresetId) ??
    presetsQuery.data?.[0]?.id ??
    ""
  const chosenLanguage =
    language ?? settings?.defaultSessionLanguage ?? defaultMeetingLanguage()
  const languages = MEETING_LANGUAGES.some(
    (option) => option.tag === chosenLanguage,
  )
    ? MEETING_LANGUAGES
    : [...MEETING_LANGUAGES, { tag: chosenLanguage, label: chosenLanguage }]

  return (
    <section
      aria-labelledby="quick-start-title"
      className="bg-card rounded-2xl border p-6 shadow-sm md:p-8"
    >
      <h1
        className="text-3xl font-semibold tracking-tight"
        id="quick-start-title"
      >
        Start a meeting
      </h1>
      <p className="text-muted-foreground mt-2 max-w-2xl text-sm leading-6">
        KokoroKoe listens to your microphone and your computer's audio,
        transcribes on this device, and suggests insights while you talk.
      </p>

      <form
        className="mt-6 grid gap-4"
        onSubmit={(event) => {
          event.preventDefault()
          start.mutate({
            project,
            title,
            language: chosenLanguage,
            presetId: chosenPresetId,
          })
        }}
      >
        <div className="grid gap-4 md:grid-cols-[2fr_1fr_1fr]">
          <label className="grid gap-1.5 text-sm font-medium">
            Title
            <input
              className={fieldClass}
              disabled={start.isPending}
              maxLength={256}
              onChange={(event) => setTitle(event.target.value)}
              placeholder={defaultMeetingTitle()}
              value={title}
            />
          </label>
          <label className="grid gap-1.5 text-sm font-medium">
            Spoken language
            <select
              className={fieldClass}
              disabled={start.isPending}
              onChange={(event) => setLanguage(event.target.value)}
              value={chosenLanguage}
            >
              {languages.map((option) => (
                <option key={option.tag} value={option.tag}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label className="grid gap-1.5 text-sm font-medium">
            Meeting type
            <select
              className={fieldClass}
              disabled={start.isPending || !presetsQuery.data}
              onChange={(event) => setPresetId(event.target.value)}
              value={chosenPresetId}
            >
              {(presetsQuery.data ?? []).map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.name}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div>
          <button
            aria-expanded={moreOptions}
            className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1 text-xs font-medium"
            onClick={() => setMoreOptions((open) => !open)}
            type="button"
          >
            <ChevronDown
              aria-hidden="true"
              className={cn(
                "size-3.5 transition-transform",
                moreOptions && "rotate-180",
              )}
            />
            More options
          </button>
          {moreOptions && (
            <label className="mt-3 grid max-w-sm gap-1.5 text-sm font-medium">
              Project
              <select
                className={fieldClass}
                disabled={start.isPending}
                onChange={(event) => {
                  setProjectId(event.target.value)
                  setPresetId(undefined)
                }}
                value={project?.id ?? ""}
              >
                {!general && (
                  <option value="">{GENERAL_PROJECT_NAME} (new)</option>
                )}
                {projects.map((candidate) => (
                  <option key={candidate.id} value={candidate.id}>
                    {candidate.name}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>

        <div className="flex flex-col gap-3 border-t pt-4 md:flex-row md:items-center md:justify-between">
          <p className="text-muted-foreground max-w-2xl text-xs leading-5">
            Pressing Start begins capturing and transcribing on this device. You
            are responsible for obtaining any consent required to record or
            transcribe the people in this meeting.
          </p>
          <Button
            className="shrink-0"
            disabled={
              start.isPending ||
              settings === undefined ||
              blockedReason !== undefined
            }
            size="lg"
            type="submit"
          >
            <Mic data-icon="inline-start" />
            {start.isPending ? "Starting…" : "Start meeting"}
          </Button>
        </div>
        {blockedReason && (
          <p
            className="text-sm text-amber-700 dark:text-amber-400"
            role="status"
          >
            {blockedReason}
          </p>
        )}
      </form>
      {start.error && (
        <div className="mt-4">
          <SanitizedErrorPanel error={start.error} />
        </div>
      )}
    </section>
  )
}
