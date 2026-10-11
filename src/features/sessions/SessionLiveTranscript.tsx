import { Bookmark, Copy, Star } from "lucide-react"
import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Bubble, BubbleContent } from "@/components/ui/bubble"
import { Button } from "@/components/ui/button"
import { Message, MessageContent, MessageHeader } from "@/components/ui/message"
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
} from "@/components/ui/message-scroller"
import {
  type ApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import { type Project, type Session } from "@/contracts/projects"
import {
  annotateTranscriptSegmentRequestSchema,
  type SegmentAnnotation,
} from "@/contracts/transcripts"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import {
  markableSegmentId,
  type TranscriptRecord,
} from "@/features/transcript/live-transcript-records"
import { segmentAsText } from "@/features/transcript/segment-actions"
import { annotateTranscriptSegment } from "@/lib/tauri/transcripts"
import { cn } from "@/lib/utils"

export function SessionLiveTranscript({
  project,
  records,
  session,
  viewportClassName = "h-56",
}: {
  project: Project
  records: TranscriptRecord[]
  session: Session
  /** The scrolling area's height; the meeting view gives it most of a screen. */
  viewportClassName?: string
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
      <div
        className={cn(
          "bg-muted/20 overflow-hidden rounded-lg border",
          viewportClassName,
        )}
      >
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
