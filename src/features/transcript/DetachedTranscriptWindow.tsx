import { useEffect, useState } from "react"
import type { UnlistenFn } from "@tauri-apps/api/event"

import { Badge } from "@/components/ui/badge"
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
import {
  reduceLiveRecords,
  type TranscriptRecord,
} from "@/features/transcript/live-transcript-records"
import type { TranscriptWindowAppearance } from "@/contracts/windows"
import {
  listenToSessionTranscriptionFinals,
  listenToSessionTranscriptionGaps,
  listenToSessionTranscriptionPartials,
} from "@/lib/tauri/sessions"
import { listenToTranscriptWindowAppearance } from "@/lib/tauri/windows"
import { cn } from "@/lib/utils"

type ScopedTranscriptEvent =
  | SessionTranscriptionPartial
  | SessionTranscriptionFinal
  | SessionTranscriptionGap

/**
 * The detached live-transcript window.
 *
 * This surface runs in its own webview whose capability grants event
 * subscription only: it invokes no command and reads no file. It shows records
 * that arrive while it is open, so opening it mid-Session does not replay
 * earlier speech; the saved transcript remains the record of the whole Session.
 */
export function DetachedTranscriptWindow() {
  const [records, setRecords] = useState<TranscriptRecord[]>([])
  const [scope, setScope] = useState<{ sessionId: string }>()
  const [error, setError] = useState<ApplicationError>()
  const [appearance, setAppearance] = useState<TranscriptWindowAppearance>()

  useEffect(() => {
    let disposed = false
    let dispose: UnlistenFn | undefined
    void listenToTranscriptWindowAppearance((next) => {
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
    }
  }, [])

  useEffect(() => {
    let disposed = false
    const unlisteners: Array<() => void> = []
    const receive = (event: ScopedTranscriptEvent) => {
      setScope((current) =>
        current?.sessionId === event.sessionId
          ? current
          : { sessionId: event.sessionId },
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
          <Badge variant={scope ? "secondary" : "outline"}>
            {scope ? "Receiving" : "Waiting for a Session"}
          </Badge>
        </header>
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
                        <DetachedRecord compact={compact} record={record} />
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
  record,
}: {
  compact: boolean
  record: TranscriptRecord
}) {
  const source =
    record.kind === "segment" ? record.segment.source : record.source
  const startMs =
    record.kind === "segment" ? record.segment.startMs : record.startMs
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
    </article>
  )
}

function formatTimeline(milliseconds: number) {
  const seconds = Math.floor(milliseconds / 1_000)
  return `${Math.floor(seconds / 60)
    .toString()
    .padStart(2, "0")}:${(seconds % 60).toString().padStart(2, "0")}`
}
