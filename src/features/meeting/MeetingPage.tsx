import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Mic, PanelRightOpen, Pause, Play, Plus, Square } from "lucide-react"
import { useEffect, useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import { requestIdSchema } from "@/contracts/models"
import {
  type Project,
  type ProjectId,
  type Session,
  type SessionId,
} from "@/contracts/projects"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { SessionLiveTranscript } from "@/features/sessions/SessionLiveTranscript"
import { SessionSummaryPanel } from "@/features/sessions/SessionSummaryPanel"
import { useSessionLiveRecords } from "@/features/sessions/use-session-live-records"
import { useSessionLifecycleMutation } from "@/features/sessions/use-sessions"
import { SavedTranscript } from "@/features/transcript/SavedTranscript"
import { getProject } from "@/lib/tauri/projects"
import { getSession } from "@/lib/tauri/sessions"
import { openDetachedWindow } from "@/lib/tauri/windows"
import { useActiveSessionStore } from "@/stores/active-session-store"
import { useMeetingStore } from "@/stores/meeting-store"
import { useNavigationStore } from "@/stores/navigation-store"

import { MeetingInsights } from "./MeetingInsights"
import { MEETING_STATE_LABELS } from "./meeting-labels"

const LIVE_STATES: ReadonlySet<Session["state"]> = new Set([
  "transcribing",
  "paused",
])
const ENDED_STATES: ReadonlySet<Session["state"]> = new Set([
  "completed",
  "failed",
])

/**
 * The meeting: its transcript on the left, insights or its summary on the
 * right, and the controls that matter in the header. It shows the meeting just
 * started, the one still running, or a past one opened from Home.
 */
export function MeetingPage() {
  const focus = useMeetingStore((state) => state.focus)
  const active = useActiveSessionStore((state) => state.activeSession)
  const target =
    focus ??
    (active ? { projectId: active.projectId, sessionId: active.id } : undefined)

  if (!target) return <NoMeeting />
  return (
    <MeetingRoom
      key={target.sessionId}
      projectId={target.projectId}
      sessionId={target.sessionId}
    />
  )
}

function NoMeeting() {
  const navigate = useNavigationStore((state) => state.navigate)
  return (
    <Card>
      <CardContent className="grid min-h-72 place-items-center p-8 text-center">
        <div className="max-w-sm space-y-3">
          <Mic className="text-muted-foreground mx-auto size-6" />
          <h1 className="text-xl font-semibold">No meeting is running</h1>
          <p className="text-muted-foreground text-sm">
            Start one from Home, or open a past meeting there to read its
            transcript and summary.
          </p>
          <Button onClick={() => navigate("home")}>
            <Plus data-icon="inline-start" /> Start a meeting
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}

const meetingSessionQueryKey = (projectId: ProjectId, sessionId: SessionId) =>
  ["meeting-session", projectId, sessionId] as const

function MeetingRoom({
  projectId,
  sessionId,
}: {
  projectId: ProjectId
  sessionId: SessionId
}) {
  const queryClient = useQueryClient()
  const projectQuery = useQuery<Project, ApplicationError>({
    queryKey: ["meeting-project", projectId],
    queryFn: () => getProject(projectId),
    gcTime: 0,
  })
  const sessionKey = meetingSessionQueryKey(projectId, sessionId)
  const sessionQuery = useQuery<Session, ApplicationError>({
    queryKey: sessionKey,
    queryFn: () => getSession({ projectId, sessionId }),
    gcTime: 0,
  })
  const live = useSessionLiveRecords(projectId)

  const error = projectQuery.error ?? sessionQuery.error
  if (error) return <SanitizedErrorPanel error={error} />
  if (!projectQuery.data || !sessionQuery.data) {
    return <p className="text-muted-foreground text-sm">Opening the meeting…</p>
  }
  return (
    <MeetingLayout
      finals={live.finals[sessionId] ?? 0}
      onSession={(session) => queryClient.setQueryData(sessionKey, session)}
      project={projectQuery.data}
      records={live.records[sessionId] ?? []}
      session={sessionQuery.data}
    />
  )
}

function MeetingLayout({
  project,
  session,
  records,
  finals,
  onSession,
}: {
  project: Project
  session: Session
  records: Parameters<typeof SessionLiveTranscript>[0]["records"]
  finals: number
  onSession: (session: Session) => void
}) {
  const navigate = useNavigationStore((state) => state.navigate)
  const clearFocus = useMeetingStore((state) => state.clear)
  const syncActive = useActiveSessionStore((state) => state.syncSession)
  const lifecycle = useSessionLifecycleMutation(project.id)
  const [windowError, setWindowError] = useState<ApplicationError>()
  const isLive = LIVE_STATES.has(session.state)
  const isEnded = ENDED_STATES.has(session.state)

  // A meeting reopened after a restart is still the active one; the sidebar
  // controls should know about it without visiting the project first.
  useEffect(() => syncActive(session), [session, syncActive])

  const boundary = {
    projectId: project.id,
    sessionId: session.id,
    expectedRevision: session.revision,
  }
  const freshRequest = () => ({
    requestId: requestIdSchema.parse(globalThis.crypto.randomUUID()),
    acknowledgedCaptureConsent: true as const,
  })
  const act = (action: Parameters<typeof lifecycle.mutate>[0]) =>
    lifecycle.mutate(action, { onSuccess: onSession })

  return (
    <div className="space-y-6">
      <header className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-2">
            <Badge
              variant={
                session.state === "transcribing" ? "default" : "secondary"
              }
            >
              {MEETING_STATE_LABELS[session.state]}
            </Badge>
            {session.state === "transcribing" && session.startedAt && (
              <Elapsed since={session.startedAt} />
            )}
          </div>
          <h1 className="truncate text-2xl font-semibold tracking-tight">
            {session.title}
          </h1>
          <p className="text-muted-foreground text-sm">
            {project.name} · {session.preset.name}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {session.state === "idle" && (
            <Button
              disabled={lifecycle.isPending}
              onClick={() =>
                act({
                  kind: "start",
                  request: { ...boundary, ...freshRequest() },
                })
              }
            >
              <Mic data-icon="inline-start" /> Start
            </Button>
          )}
          {session.state === "transcribing" && (
            <Button
              disabled={lifecycle.isPending}
              onClick={() => act({ kind: "pause", request: boundary })}
              variant="outline"
            >
              <Pause data-icon="inline-start" /> Pause
            </Button>
          )}
          {session.state === "paused" && (
            <Button
              disabled={lifecycle.isPending}
              onClick={() =>
                act({
                  kind: "resume",
                  request: { ...boundary, ...freshRequest() },
                })
              }
            >
              <Play data-icon="inline-start" /> Resume
            </Button>
          )}
          {isLive && (
            <Button
              disabled={lifecycle.isPending}
              onClick={() => act({ kind: "stop", request: boundary })}
              variant="destructive"
            >
              <Square data-icon="inline-start" /> End meeting
            </Button>
          )}
          {isLive && (
            <Button
              aria-label="Pop out transcript"
              onClick={() =>
                void openDetachedWindow("transcript").catch((caught: unknown) =>
                  setWindowError(toApplicationError(caught)),
                )
              }
              size="icon"
              variant="ghost"
            >
              <PanelRightOpen />
            </Button>
          )}
          {isEnded && (
            <Button
              onClick={() => {
                clearFocus()
                navigate("home")
              }}
              variant="outline"
            >
              <Plus data-icon="inline-start" /> New meeting
            </Button>
          )}
        </div>
      </header>

      {(session.state === "idle" || session.state === "paused") && (
        <p className="text-muted-foreground text-xs">
          Starting or resuming captures your microphone and computer audio and
          transcribes them on this device. You are responsible for obtaining any
          consent required to record or transcribe the people in this meeting.
        </p>
      )}
      {session.channelHealth.microphone.detailCode === "recovery_required" && (
        <p className="text-sm text-amber-700 dark:text-amber-400" role="status">
          This meeting ended unexpectedly last time. What was transcribed before
          then is saved; resume to continue or end it to keep it as it is.
        </p>
      )}
      {lifecycle.error && <SanitizedErrorPanel error={lifecycle.error} />}
      {windowError && <SanitizedErrorPanel error={windowError} />}

      {isEnded ? (
        <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(320px,420px)]">
          <SavedTranscript
            onClose={() => {
              clearFocus()
              navigate("home")
            }}
            project={project}
            session={session}
          />
          <section aria-label="Meeting summary" className="space-y-3">
            <h2 className="text-sm font-semibold">Summary</h2>
            <SessionSummaryPanel project={project} session={session} />
          </section>
        </div>
      ) : (
        <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(320px,420px)]">
          {session.state === "idle" ? (
            <p className="text-muted-foreground rounded-xl border border-dashed p-6 text-sm">
              This meeting has not started. Press Start when you are ready.
            </p>
          ) : (
            <SessionLiveTranscript
              project={project}
              records={records}
              session={session}
              viewportClassName="h-[60vh]"
            />
          )}
          <MeetingInsights
            finals={finals}
            project={project}
            session={session}
          />
        </div>
      )}
    </div>
  )
}

function Elapsed({ since }: { since: string }) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1_000)
    return () => window.clearInterval(timer)
  }, [])
  const seconds = Math.max(0, Math.floor((now - Date.parse(since)) / 1_000))
  const hours = Math.floor(seconds / 3_600)
  const minutes = Math.floor((seconds % 3_600) / 60)
  const pad = (value: number) => String(value).padStart(2, "0")
  return (
    <span className="text-muted-foreground text-xs tabular-nums">
      {hours > 0 ? `${hours}:` : ""}
      {pad(minutes)}:{pad(seconds % 60)}
    </span>
  )
}
