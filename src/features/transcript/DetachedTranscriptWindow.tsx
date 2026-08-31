import { useEffect, useState } from "react"
import { Copy, Star } from "lucide-react"
import type { UnlistenFn } from "@tauri-apps/api/event"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
} from "@/components/ui/message-scroller"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import type {
  SessionTranscriptionFinal,
  SessionTranscriptionGap,
  SessionTranscriptionPartial,
} from "@/contracts/session-lifecycle"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { markTranscriptSegmentRequestSchema } from "@/contracts/transcripts"
import {
  markableSegmentId,
  reduceLiveRecords,
  type TranscriptRecord,
} from "@/features/transcript/live-transcript-records"
import { markTranscriptSegment } from "@/lib/tauri/transcripts"
import { segmentAsText } from "@/features/transcript/segment-actions"
import type { DetachedWindowAppearance } from "@/contracts/windows"
import {
  listenToSessionTranscriptionFinals,
  listenToSessionTranscriptionGaps,
  listenToSessionTranscriptionPartials,
} from "@/lib/tauri/sessions"
import {
  listenToDetachedWindowAppearance,
  listenToDetachedWindowInteraction,
} from "@/lib/tauri/windows"
import { useAppliedTheme } from "@/app/shell/use-applied-theme"
import { cn } from "@/lib/utils"

type ScopedTranscriptEvent =
  | SessionTranscriptionPartial
  | SessionTranscriptionFinal
  | SessionTranscriptionGap

/**
 * The detached live-transcript window.
 *
 * This surface runs in its own webview. Its capability grants event
 * subscription and exactly one command — marking a finalized segment important
 * — because the window exists to be watched while the main window is hidden,
 * so an action taken here cannot be routed through a window that may not be
 * there. ADR 0010 records that decision; the window still reads no file and
 * holds nothing that can write text into the record.
 *
 * It shows records that arrive while it is open, so opening it mid-Session does
 * not replay earlier speech; the saved transcript remains the record of the
 * whole Session.
 */
export function DetachedTranscriptWindow() {
  const [records, setRecords] = useState<TranscriptRecord[]>([])
  const [scope, setScope] = useState<{ projectId: string; sessionId: string }>()
  const [error, setError] = useState<ApplicationError>()
  const [marked, setMarked] = useState<Record<string, boolean>>({})
  const [markingId, setMarkingId] = useState<string>()
  const [copiedId, setCopiedId] = useState<string>()
  const [notice, setNotice] = useState<string>()
  const [appearance, setAppearance] = useState<DetachedWindowAppearance>()
  const [clickThrough, setClickThrough] = useState(false)

  // A popped-out window is the same application, so it wears the same theme.
  useAppliedTheme()

  useEffect(() => {
    let disposed = false
    let dispose: UnlistenFn | undefined
    let interactionDispose: UnlistenFn | undefined
    void listenToDetachedWindowInteraction("transcript", (next) => {
      setClickThrough(next.clickThrough)
    })
      .then((unlisten) => {
        if (disposed) unlisten()
        else interactionDispose = unlisten
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
    void listenToDetachedWindowAppearance("transcript", (next) => {
      setAppearance(next)
    })
      .then((unlisten) => {
        if (disposed) unlisten()
        else dispose = unlisten
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
    return () => {
      disposed = true
      dispose?.()
      interactionDispose?.()
    }
  }, [])

  useEffect(() => {
    let disposed = false
    const unlisteners: Array<() => void> = []
    const receive = (event: ScopedTranscriptEvent) => {
      setScope((current) =>
        current?.sessionId === event.sessionId
          ? current
          : { projectId: event.projectId, sessionId: event.sessionId },
      )
      setRecords((current) => reduceLiveRecords(current, event.event))
    }

    for (const subscription of [
      listenToSessionTranscriptionPartials(receive),
      listenToSessionTranscriptionFinals(receive),
      listenToSessionTranscriptionGaps(receive),
    ]) {
      void subscription
        .then((unlisten) =>
          disposed ? unlisten() : unlisteners.push(unlisten),
        )
        .catch((caught: unknown) => {
          if (!disposed) setError(toApplicationError(caught))
        })
    }
    return () => {
      disposed = true
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, [])

  /**
   * Flags the moment the user just heard.
   *
   * The scope comes from the events this window received, never from anything
   * it composed, so it can only ever name the Session it is displaying.
   */
  async function mark(segmentId: string, important: boolean) {
    if (!scope) return
    const request = markTranscriptSegmentRequestSchema.safeParse({
      projectId: scope.projectId,
      sessionId: scope.sessionId,
      segmentId,
      important,
    })
    if (!request.success) return
    setMarkingId(segmentId)
    setError(undefined)
    try {
      const updated = await markTranscriptSegment(request.data)
      setMarked((current) => ({
        ...current,
        [segmentId]: updated.important === true,
      }))
    } catch (caught: unknown) {
      setError(toApplicationError(caught))
    } finally {
      setMarkingId(undefined)
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

  const compact = appearance?.compact ?? false
  // Only the background carries the alpha. Text and borders stay fully opaque
  // so lowering opacity never costs readability.
  const backgroundStyle = {
    "--transcript-window-opacity": String(appearance?.backgroundOpacity ?? 1),
  } as React.CSSProperties

  return (
    <div
      className="text-foreground relative flex h-svh flex-col"
      data-compact={compact ? "true" : undefined}
      data-testid="detached-transcript-window"
      style={backgroundStyle}
    >
      <div
        aria-hidden="true"
        className="bg-background absolute inset-0 -z-10"
        style={{ opacity: "var(--transcript-window-opacity)" }}
      />
      <div
        className={cn(
          "flex min-h-0 flex-1 flex-col gap-2",
          compact ? "p-1.5" : "p-3",
        )}
      >
        <header className="flex items-center justify-between gap-2">
          <p className={cn("font-medium", compact ? "text-xs" : "text-sm")}>
            Live transcript
          </p>
          <div className="flex items-center gap-1">
            {clickThrough && (
              // Without this a click-through window looks identical to a frozen
              // one, and the user has no cue why the mouse does nothing.
              <Badge variant="outline">Clicks pass through</Badge>
            )}
            <Badge variant={scope ? "secondary" : "outline"}>
              {scope ? "Receiving" : "Waiting for a Session"}
            </Badge>
          </div>
        </header>
        {notice && (
          <p className="text-muted-foreground text-xs" role="status">
            {notice}
          </p>
        )}
        {error && <SanitizedErrorPanel error={error} />}
        <div className="min-h-0 flex-1 overflow-hidden rounded-lg border">
          {records.length === 0 ? (
            <div className="grid size-full place-items-center p-4 text-center">
              <p className="text-muted-foreground text-xs">
                Speech transcribed while this window is open appears here.
              </p>
            </div>
          ) : (
            <MessageScrollerProvider autoScroll>
              <MessageScroller>
                <MessageScrollerViewport>
                  <MessageScrollerContent
                    className={cn(compact ? "gap-1.5 p-1.5" : "gap-3 p-3")}
                  >
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
                        <DetachedRecord
                          compact={compact}
                          copied={
                            record.kind === "segment" &&
                            copiedId === record.segment.id
                          }
                          important={
                            record.kind === "segment" &&
                            marked[record.segment.id] === true
                          }
                          marking={
                            record.kind === "segment" &&
                            markingId === record.segment.id
                          }
                          onCopy={(segment) => void copySegment(segment)}
                          onMark={mark}
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
      </div>
    </div>
  )
}

function DetachedRecord({
  compact,
  copied,
  important,
  marking,
  onCopy,
  onMark,
  record,
}: {
  compact: boolean
  copied: boolean
  important: boolean
  marking: boolean
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
  const markable = markableSegmentId(record)
  return (
    <article className={cn(compact ? "space-y-0.5" : "space-y-1")}>
      <div className="text-muted-foreground flex items-center gap-2 text-xs">
        <span className="text-foreground font-medium">
          {source === "microphone" ? "You" : "System"}
        </span>
        <span>{formatTimeline(startMs)}</span>
        {record.kind === "segment" && record.segment.status === "partial" && (
          <Badge variant="outline">partial</Badge>
        )}
      </div>
      {record.kind === "segment" ? (
        <p
          className={cn(
            "break-words whitespace-pre-wrap",
            compact ? "text-xs leading-5" : "text-sm leading-6",
          )}
        >
          {record.segment.text}
        </p>
      ) : (
        <p className="text-muted-foreground rounded-md border border-dashed px-2 py-1 text-xs">
          Audio gap ({record.code.replaceAll("_", " ")})
        </p>
      )}
      {markable !== undefined && record.kind === "segment" && (
        <div className="flex flex-wrap items-center gap-1">
          <Button
            aria-pressed={important}
            className={cn(important && "border-primary")}
            disabled={marking}
            onClick={() => onMark(markable, !important)}
            size="xs"
            type="button"
            variant="ghost"
          >
            <Star aria-hidden="true" data-icon="inline-start" />
            {important ? "Important" : "Mark important"}
          </Button>
          {/*
            Copying is view-local: the clipboard is a browser API, so this
            needs no command and widens nothing this window may do. It is the
            one per-segment action a display-only window can simply have.
          */}
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
    </article>
  )
}

function formatTimeline(milliseconds: number) {
  const seconds = Math.floor(milliseconds / 1_000)
  return `${Math.floor(seconds / 60)
    .toString()
    .padStart(2, "0")}:${(seconds % 60).toString().padStart(2, "0")}`
}
