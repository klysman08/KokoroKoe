import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  CalendarClock,
  FileText,
  Pause,
  Pencil,
  Play,
  Plus,
  Square,
} from "lucide-react"
import { useEffect, useMemo, useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Bubble, BubbleContent } from "@/components/ui/bubble"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Message, MessageContent, MessageHeader } from "@/components/ui/message"
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
} from "@/components/ui/message-scroller"
import { type Project, type Session } from "@/contracts/projects"
import { type AppSettings } from "@/contracts/settings"
import { type ApplicationError } from "@/contracts/app-error"
import { type AudioDeviceList } from "@/contracts/audio"
import { requestIdSchema } from "@/contracts/models"
import { type PersistenceStatus } from "@/contracts/session-lifecycle"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { SessionForm } from "@/features/sessions/SessionForm"
import { SessionInsightsPanel } from "@/features/sessions/SessionInsightsPanel"
import {
  useCreateSessionMutation,
  useSessionLifecycleMutation,
  useSessionsQuery,
  useUpdateSessionMutation,
} from "@/features/sessions/use-sessions"
import { listAudioDevices } from "@/lib/tauri/audio"
import { SavedTranscript } from "@/features/transcript/SavedTranscript"
import {
  reduceLiveRecords,
  type TranscriptRecord,
} from "@/features/transcript/live-transcript-records"
import {
  listenToPersistenceStatus,
  listenToSessionTranscriptionFinals,
  listenToSessionTranscriptionGaps,
  listenToSessionTranscriptionPartials,
} from "@/lib/tauri/sessions"
import { useActiveSessionStore } from "@/stores/active-session-store"

export function ProjectSessions({
  project,
  onClose,
  settings,
}: {
  project: Project
  onClose: () => void
  settings: AppSettings | undefined
}) {
  const sessionsQuery = useSessionsQuery(project.id, true)
  const devicesQuery = useQuery<AudioDeviceList, ApplicationError>({
    queryKey: ["audio-devices", "session-form"],
    queryFn: listAudioDevices,
    gcTime: 0,
  })
  const createMutation = useCreateSessionMutation(project.id)
  const updateMutation = useUpdateSessionMutation(project.id)
  const lifecycleMutation = useSessionLifecycleMutation(project.id)
  const queryClient = useQueryClient()
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<Session>()
  const [viewingTranscript, setViewingTranscript] = useState<Session>()
  const [consent, setConsent] = useState<Record<string, boolean>>({})
  const [persistence, setPersistence] = useState<
    Record<string, PersistenceStatus>
  >({})
  const [liveRecords, setLiveRecords] = useState<
    Record<string, TranscriptRecord[]>
  >({})
  const syncActiveSession = useActiveSessionStore((state) => state.syncSession)
  const sessions = useMemo(
    () => sessionsQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [sessionsQuery.data?.pages],
  )
  const busy =
    createMutation.isPending ||
    updateMutation.isPending ||
    lifecycleMutation.isPending
  const closeForm = () => {
    setCreating(false)
    setEditing(undefined)
    createMutation.reset()
    updateMutation.reset()
  }

  useEffect(() => {
    const active = sessions.find(
      (session) =>
        session.state === "transcribing" || session.state === "paused",
    )
    if (active) syncActiveSession(active)
  }, [sessions, syncActiveSession])

  useEffect(() => {
    let disposed = false
    const unlisteners: Array<() => void> = []
    for (const subscription of [
      listenToPersistenceStatus((status) => {
        if (status.projectId !== project.id) return
        setPersistence((current) => ({
          ...current,
          [status.sessionId]: status,
        }))
      }),
      listenToSessionTranscriptionFinals((envelope) => {
        if (envelope.projectId !== project.id) return
        setLiveRecords((current) => ({
          ...current,
          [envelope.sessionId]: reduceLiveRecords(
            current[envelope.sessionId] ?? [],
            envelope.event,
          ),
        }))
        void queryClient.invalidateQueries({
          queryKey: ["saved-transcript", project.id, envelope.sessionId],
        })
      }),
      listenToSessionTranscriptionPartials((envelope) => {
        if (envelope.projectId !== project.id) return
        setLiveRecords((current) => ({
          ...current,
          [envelope.sessionId]: reduceLiveRecords(
            current[envelope.sessionId] ?? [],
            envelope.event,
          ),
        }))
      }),
      listenToSessionTranscriptionGaps((envelope) => {
        if (envelope.projectId !== project.id) return
        setLiveRecords((current) => ({
          ...current,
          [envelope.sessionId]: reduceLiveRecords(
            current[envelope.sessionId] ?? [],
            envelope.event,
          ),
        }))
      }),
    ]) {
      void subscription
        .then((unlisten) => {
          if (disposed) unlisten()
          else unlisteners.push(unlisten)
        })
        .catch(() => undefined)
    }
    return () => {
      disposed = true
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, [project.id, queryClient])

  return (
    <>
      <Card>
        <CardHeader className="flex-row items-start justify-between gap-4">
          <div>
            <CardTitle>{project.name} sessions</CardTitle>
            <CardDescription>
              Session metadata is saved to project-scoped portable Markdown.
            </CardDescription>
          </div>
          <div className="flex gap-2">
            <Button
              disabled={!devicesQuery.data}
              onClick={() => {
                setEditing(undefined)
                setCreating(true)
              }}
              size="sm"
            >
              <Plus />
              New session
            </Button>
            <Button onClick={onClose} size="sm" variant="outline">
              Close
            </Button>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {(creating || editing) && devicesQuery.data && (
            <SessionForm
              busy={busy}
              devices={devicesQuery.data}
              key={editing?.id ?? "create"}
              onCancel={closeForm}
              onCreate={(value) =>
                createMutation.mutate(
                  { projectId: project.id, value },
                  { onSuccess: closeForm },
                )
              }
              onUpdate={(value) =>
                editing &&
                updateMutation.mutate(
                  {
                    projectId: project.id,
                    sessionId: editing.id,
                    expectedRevision: editing.revision,
                    value,
                  },
                  { onSuccess: closeForm },
                )
              }
              project={project}
              settings={settings}
              session={editing}
            />
          )}
          {devicesQuery.error && (
            <SanitizedErrorPanel error={devicesQuery.error} />
          )}
          {(sessionsQuery.error ||
            createMutation.error ||
            updateMutation.error ||
            lifecycleMutation.error) && (
            <SanitizedErrorPanel
              error={
                (sessionsQuery.error ??
                  createMutation.error ??
                  updateMutation.error ??
                  lifecycleMutation.error)!
              }
            />
          )}
          {sessionsQuery.isPending ? (
            <p className="text-muted-foreground text-sm">Loading sessionsâ€¦</p>
          ) : sessions.length === 0 ? (
            <div className="bg-muted/30 grid min-h-28 place-items-center rounded-xl border border-dashed p-6 text-center">
              <div>
                <CalendarClock className="text-muted-foreground mx-auto size-5" />
                <p className="mt-2 text-sm font-medium">
                  No sessions in this project
                </p>
              </div>
            </div>
          ) : (
            <div className="grid gap-3 md:grid-cols-2">
              {sessions.map((session) => (
                <div className="rounded-xl border p-4" key={session.id}>
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <p className="font-medium">{session.title}</p>
                      <p className="text-muted-foreground mt-1 line-clamp-2 text-xs">
                        {session.objective || "No objective"}
                      </p>
                    </div>
                    <Button
                      aria-label={`Edit ${session.title}`}
                      disabled={session.state !== "idle"}
                      onClick={() => {
                        setCreating(false)
                        setEditing(session)
                      }}
                      size="icon"
                      variant="ghost"
                    >
                      <Pencil />
                    </Button>
                  </div>
                  <div className="mt-3 flex flex-wrap items-center gap-2">
                    <Badge variant="secondary">{session.state}</Badge>
                    <span className="text-muted-foreground text-xs">
                      Revision {session.revision}
                    </span>
                    <Button
                      onClick={() => setViewingTranscript(session)}
                      size="sm"
                      variant="outline"
                    >
                      <FileText aria-hidden="true" /> View transcript
                    </Button>
                  </div>
                  <div className="mt-3 space-y-2 border-t pt-3">
                    {(session.state === "idle" ||
                      session.state === "paused") && (
                      <label className="flex items-start gap-2 text-xs">
                        <input
                          checked={consent[session.id] ?? false}
                          disabled={busy}
                          onChange={(event) =>
                            setConsent((current) => ({
                              ...current,
                              [session.id]: event.target.checked,
                            }))
                          }
                          type="checkbox"
                        />
                        <span>
                          I consent to local microphone and system-audio
                          capture.
                        </span>
                      </label>
                    )}
                    <div className="flex flex-wrap gap-2">
                      {session.state === "idle" && (
                        <Button
                          disabled={busy || !consent[session.id]}
                          onClick={() =>
                            lifecycleMutation.mutate({
                              kind: "start",
                              request: {
                                projectId: project.id,
                                sessionId: session.id,
                                expectedRevision: session.revision,
                                requestId: requestIdSchema.parse(
                                  globalThis.crypto.randomUUID(),
                                ),
                                acknowledgedCaptureConsent: true,
                              },
                            })
                          }
                          size="sm"
                        >
                          <Play aria-hidden="true" /> Start
                        </Button>
                      )}
                      {session.state === "transcribing" && (
                        <Button
                          disabled={busy}
                          onClick={() =>
                            lifecycleMutation.mutate({
                              kind: "pause",
                              request: lifecycleBoundaryRequest(
                                project,
                                session,
                              ),
                            })
                          }
                          size="sm"
                          variant="outline"
                        >
                          <Pause aria-hidden="true" /> Pause
                        </Button>
                      )}
                      {session.state === "paused" && (
                        <Button
                          disabled={busy || !consent[session.id]}
                          onClick={() =>
                            lifecycleMutation.mutate({
                              kind: "resume",
                              request: {
                                ...lifecycleBoundaryRequest(project, session),
                                requestId: requestIdSchema.parse(
                                  globalThis.crypto.randomUUID(),
                                ),
                                acknowledgedCaptureConsent: true,
                              },
                            })
                          }
                          size="sm"
                        >
                          <Play aria-hidden="true" /> Resume
                        </Button>
                      )}
                      {(session.state === "transcribing" ||
                        session.state === "paused") && (
                        <Button
                          disabled={busy}
                          onClick={() =>
                            lifecycleMutation.mutate({
                              kind: "stop",
                              request: lifecycleBoundaryRequest(
                                project,
                                session,
                              ),
                            })
                          }
                          size="sm"
                          variant="outline"
                        >
                          <Square aria-hidden="true" /> Stop
                        </Button>
                      )}
                    </div>
                    {session.channelHealth.microphone.detailCode ===
                      "recovery_required" && (
                      <p className="text-xs text-amber-700" role="status">
                        Recovery required: the previous run ended unexpectedly.
                        Review the saved transcript before resuming or stopping.
                      </p>
                    )}
                    <PersistenceLine status={persistence[session.id]} />
                    {(session.state === "transcribing" ||
                      session.state === "paused") && (
                      <>
                        <SessionLiveTranscript
                          records={liveRecords[session.id] ?? []}
                          session={session}
                        />
                        <SessionInsightsPanel
                          project={project}
                          session={session}
                        />
                      </>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
          {sessionsQuery.hasNextPage && (
            <div className="flex justify-center">
              <Button
                disabled={sessionsQuery.isFetchingNextPage}
                onClick={() => void sessionsQuery.fetchNextPage()}
                variant="outline"
              >
                {sessionsQuery.isFetchingNextPage ? "Loadingâ€¦" : "Load more"}
              </Button>
            </div>
          )}
        </CardContent>
      </Card>
      {viewingTranscript && (
        <SavedTranscript
          key={viewingTranscript.id}
          onClose={() => setViewingTranscript(undefined)}
          project={project}
          session={viewingTranscript}
        />
      )}
    </>
  )
}

function SessionLiveTranscript({
  records,
  session,
}: {
  records: TranscriptRecord[]
  session: Session
}) {
  return (
    <section aria-label={`Live transcript for ${session.title}`}>
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-xs font-medium">Live transcript</p>
        <Badge variant="outline">
          {session.state === "paused" ? "Paused" : "Following latest"}
        </Badge>
      </div>
      <div className="bg-muted/20 h-56 overflow-hidden rounded-lg border">
        {records.length === 0 ? (
          <div className="grid size-full place-items-center p-4 text-center">
            <p className="text-muted-foreground text-xs">
              {session.state === "paused"
                ? "No live text was received before this pause."
                : "Listening for speech…"}
            </p>
          </div>
        ) : (
          <MessageScrollerProvider autoScroll>
            <MessageScroller>
              <MessageScrollerViewport>
                <MessageScrollerContent className="gap-3 p-3">
                  {records.map((record) => (
                    <MessageScrollerItem
                      key={
                        record.kind === "segment"
                          ? record.segment.id
                          : record.id
                      }
                      messageId={
                        record.kind === "segment"
                          ? record.segment.id
                          : record.id
                      }
                    >
                      <TranscriptMessage record={record} />
                    </MessageScrollerItem>
                  ))}
                </MessageScrollerContent>
              </MessageScrollerViewport>
              <MessageScrollerButton />
            </MessageScroller>
          </MessageScrollerProvider>
        )}
      </div>
    </section>
  )
}

function TranscriptMessage({ record }: { record: TranscriptRecord }) {
  const source =
    record.kind === "segment" ? record.segment.source : record.source
  const startMs =
    record.kind === "segment" ? record.segment.startMs : record.startMs
  const status = record.kind === "segment" ? record.segment.status : "gap"
  return (
    <Message align={source === "microphone" ? "start" : "end"}>
      <MessageContent>
        <MessageHeader>
          {source === "microphone" ? "Microphone" : "System output"} ·{" "}
          {formatTimeline(startMs)} · {status}
        </MessageHeader>
        <Bubble
          align={source === "microphone" ? "start" : "end"}
          variant={record.kind === "gap" ? "destructive" : "muted"}
        >
          <BubbleContent>
            {record.kind === "segment"
              ? record.segment.text || "Listening…"
              : `Audio gap: ${record.code}`}
          </BubbleContent>
        </Bubble>
      </MessageContent>
    </Message>
  )
}

function formatTimeline(milliseconds: number) {
  const seconds = Math.floor(milliseconds / 1_000)
  const minutes = Math.floor(seconds / 60)
  return `${String(minutes).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`
}

function lifecycleBoundaryRequest(project: Project, session: Session) {
  return {
    projectId: project.id,
    sessionId: session.id,
    expectedRevision: session.revision,
  }
}

function PersistenceLine({
  status,
}: {
  status: PersistenceStatus | undefined
}) {
  if (!status) return null
  return (
    <p className="text-muted-foreground text-xs" role="status">
      Persistence: {status.state}; journal {status.journalSequence}, snapshot{" "}
      {status.snapshotSequence}
    </p>
  )
}
