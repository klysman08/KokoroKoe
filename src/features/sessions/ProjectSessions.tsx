import { useQuery, useQueryClient } from "@tanstack/react-query"
import {
  CalendarClock,
  FileText,
  Pause,
  Pencil,
  Play,
  Plus,
  Square,
  Star,
  Bookmark,
  Copy,
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
import {
  type ApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import { type AudioDeviceList } from "@/contracts/audio"
import { requestIdSchema } from "@/contracts/models"
import { type PersistenceStatus } from "@/contracts/session-lifecycle"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { DeleteConfirm } from "@/features/projects/DeleteConfirm"
import { usePresetsQuery } from "@/features/presets/use-presets"
import { SessionForm } from "@/features/sessions/SessionForm"
import { SessionInsightsPanel } from "@/features/sessions/SessionInsightsPanel"
import { SessionSummaryPanel } from "@/features/sessions/SessionSummaryPanel"
import {
  useCreateSessionMutation,
  useDeleteSessionMutation,
  useSessionLifecycleMutation,
  useSessionsQuery,
  useUpdateSessionMutation,
} from "@/features/sessions/use-sessions"
import { listAudioDevices } from "@/lib/tauri/audio"
import { SavedTranscript } from "@/features/transcript/SavedTranscript"
import {
  annotateTranscriptSegmentRequestSchema,
  type SegmentAnnotation,
} from "@/contracts/transcripts"
import { annotateTranscriptSegment } from "@/lib/tauri/transcripts"
import { segmentAsText } from "@/features/transcript/segment-actions"
import { cn } from "@/lib/utils"
import {
  markableSegmentId,
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
  const presetsQuery = usePresetsQuery(project.id)
  const createMutation = useCreateSessionMutation(project.id)
  const updateMutation = useUpdateSessionMutation(project.id)
  const lifecycleMutation = useSessionLifecycleMutation(project.id)
  const deleteSession = useDeleteSessionMutation(project.id)
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
              disabled={!devicesQuery.data || !presetsQuery.data}
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
          {(creating || editing) && devicesQuery.data && presetsQuery.data && (
            <SessionForm
              busy={busy}
              devices={devicesQuery.data}
              presets={presetsQuery.data}
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
          {presetsQuery.error && (
            <SanitizedErrorPanel error={presetsQuery.error} />
          )}
          {(sessionsQuery.error ||
            createMutation.error ||
            updateMutation.error ||
            lifecycleMutation.error ||
            deleteSession.error) && (
            <SanitizedErrorPanel
              error={
                (sessionsQuery.error ??
                  createMutation.error ??
                  updateMutation.error ??
                  lifecycleMutation.error ??
                  deleteSession.error)!
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
                    <DeleteConfirm
                      busy={
                        deleteSession.isPending &&
                        deleteSession.variables?.sessionId === session.id
                      }
                      label={`Delete session ${session.title}`}
                      onDelete={() =>
                        deleteSession.mutate({ sessionId: session.id })
                      }
                      what="session"
                    />
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
                          project={project}
                          records={liveRecords[session.id] ?? []}
                          session={session}
                        />
                        <SessionInsightsPanel
                          project={project}
                          session={session}
                        />
                      </>
                    )}
                    {(session.state === "completed" ||
                      session.state === "failed") && (
                      <SessionSummaryPanel
                        project={project}
                        session={session}
                      />
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
  project,
  records,
  session,
}: {
  project: Project
  records: TranscriptRecord[]
  session: Session
}) {
  /**
   * Which live segments the user has marked, and which mark is in flight.
   *
   * A finalized live segment carries the same id the journal recorded it
   * under, so it can be marked the moment it appears — before the run has
   * published it to `transcript.md`. Rust resolves the segment against the
   * journal for exactly that reason.
   */
  const [marked, setMarked] = useState<Record<string, boolean>>({})
  const [bookmarked, setBookmarked] = useState<Record<string, boolean>>({})
  const [copiedId, setCopiedId] = useState<string>()
  const [busyId, setBusyId] = useState<string>()
  const [notice, setNotice] = useState<string>()
  const [markError, setMarkError] = useState<ApplicationError>()

  /**
   * Annotates the moment the user just heard.
   *
   * A finalized live segment carries the same id the journal recorded it
   * under, so it can be flagged before the run has published it; Rust resolves
   * the segment against the journal for exactly that reason.
   */
  async function annotate(segmentId: string, annotation: SegmentAnnotation) {
    const request = annotateTranscriptSegmentRequestSchema.safeParse({
      projectId: project.id,
      sessionId: session.id,
      segmentId,
      annotation,
    })
    if (!request.success) return
    setBusyId(segmentId)
    setMarkError(undefined)
    try {
      const updated = await annotateTranscriptSegment(request.data)
      setMarked((current) => ({
        ...current,
        [segmentId]: updated.important === true,
      }))
      setBookmarked((current) => ({
        ...current,
        [segmentId]: updated.bookmarked === true,
      }))
    } catch (caught: unknown) {
      setMarkError(toApplicationError(caught))
    } finally {
      setBusyId(undefined)
    }
  }

  async function copySegment(segment: {
    id: string
    source: "microphone" | "system_output"
    startMs: number
    text: string
  }) {
    try {
      await navigator.clipboard.writeText(segmentAsText(segment))
      setCopiedId(segment.id)
      setNotice(undefined)
    } catch {
      setCopiedId(undefined)
      setNotice("The clipboard is not available.")
    }
  }

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
                      <TranscriptMessage
                        bookmarked={
                          record.kind === "segment" &&
                          bookmarked[record.segment.id] === true
                        }
                        busy={
                          record.kind === "segment" &&
                          busyId === record.segment.id
                        }
                        copied={
                          record.kind === "segment" &&
                          copiedId === record.segment.id
                        }
                        important={
                          record.kind === "segment" &&
                          marked[record.segment.id] === true
                        }
                        onBookmark={(segmentId, next) =>
                          void annotate(segmentId, {
                            kind: "bookmark",
                            bookmarked: next,
                          })
                        }
                        onCopy={(segment) => void copySegment(segment)}
                        onMark={(segmentId, next) =>
                          void annotate(segmentId, {
                            kind: "importance",
                            important: next,
                          })
                        }
                        record={record}
                      />
                    </MessageScrollerItem>
                  ))}
                </MessageScrollerContent>
              </MessageScrollerViewport>
              <MessageScrollerButton />
            </MessageScroller>
          </MessageScrollerProvider>
        )}
      </div>
      {notice && (
        <p className="text-muted-foreground text-xs" role="status">
          {notice}
        </p>
      )}
      {markError && <SanitizedErrorPanel error={markError} />}
    </section>
  )
}

/**
 * One live record.
 *
 * Marking is offered on finalized segments only. A partial has no journal
 * record to name, so there is nothing to mark; correcting is deliberately left
 * to the reading view, where the text can be seen and edited properly rather
 * than typed into a scrolling pane during a meeting.
 */
function TranscriptMessage({
  bookmarked,
  busy,
  copied,
  important,
  onBookmark,
  onCopy,
  onMark,
  record,
}: {
  bookmarked: boolean
  busy: boolean
  copied: boolean
  important: boolean
  onBookmark: (segmentId: string, bookmarked: boolean) => void
  onCopy: (segment: {
    id: string
    source: "microphone" | "system_output"
    startMs: number
    text: string
  }) => void
  onMark: (segmentId: string, important: boolean) => void
  record: TranscriptRecord
}) {
  const source =
    record.kind === "segment" ? record.segment.source : record.source
  const startMs =
    record.kind === "segment" ? record.segment.startMs : record.startMs
  const status = record.kind === "segment" ? record.segment.status : "gap"
  const markable = markableSegmentId(record)
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
        {markable !== undefined && record.kind === "segment" && (
          <div className="flex flex-wrap items-center gap-1">
            <Button
              aria-pressed={important}
              className={cn(important && "border-primary")}
              disabled={busy}
              onClick={() => onMark(markable, !important)}
              size="xs"
              type="button"
              variant="ghost"
            >
              <Star aria-hidden="true" data-icon="inline-start" />
              {important ? "Important" : "Mark important"}
            </Button>
            {/*
              A bookmark dropped mid-meeting carries no name. Naming is what
              makes a list of pins scannable, but a text box in a scrolling
              pane during a call is the wrong moment to ask for one — the
              saved transcript is where a pin gets named.
            */}
            <Button
              aria-pressed={bookmarked}
              className={cn(bookmarked && "border-primary")}
              disabled={busy}
              onClick={() => onBookmark(markable, !bookmarked)}
              size="xs"
              type="button"
              variant="ghost"
            >
              <Bookmark aria-hidden="true" data-icon="inline-start" />
              {bookmarked ? "Bookmarked" : "Bookmark"}
            </Button>
            <Button
              onClick={() => onCopy(record.segment)}
              size="xs"
              type="button"
              variant="ghost"
            >
              <Copy aria-hidden="true" data-icon="inline-start" />
              {copied ? "Copied" : "Copy"}
            </Button>
          </div>
        )}
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
