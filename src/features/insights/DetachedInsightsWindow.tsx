import { useEffect, useState } from "react"
import type { UnlistenFn } from "@tauri-apps/api/event"
import { ChevronLeft, ChevronRight, Copy, Pin, X } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import type { DetachedWindowAppearance } from "@/contracts/windows"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"
import {
  dismissInsight,
  EMPTY_INSIGHT_BOARD,
  focusInsight,
  insightAsText,
  MAXIMUM_PINNED,
  pinnedCount,
  receiveInsightBatch,
  toggleInsightPin,
  type InsightCard,
} from "@/features/insights/insight-board"
import { listenToSessionInsights } from "@/lib/tauri/insights"
import {
  listenToDetachedWindowAppearance,
  listenToDetachedWindowInteraction,
} from "@/lib/tauri/windows"
import { cn } from "@/lib/utils"

type CopyStatus = "idle" | "copied" | "failed"

/**
 * The detached insights window.
 *
 * Like the transcript window it runs in its own webview whose capability grants
 * event subscription only: it invokes no command, so it can neither ask for a
 * generation nor retrieve an earlier one, and its own opacity, compact layout,
 * and pointer state reach it only as Rust-owned events.
 *
 * Pinning, dismissing, and copying are all view-local for the same reason —
 * none of them needs a command, so none of them widens what this window may
 * do. Nothing here is saved: closing the window discards every pin and every
 * dismissal along with the insights themselves.
 */
export function DetachedInsightsWindow() {
  const [board, setBoard] = useState(EMPTY_INSIGHT_BOARD)
  const [appearance, setAppearance] = useState<DetachedWindowAppearance>()
  const [clickThrough, setClickThrough] = useState(false)
  const [copyStatus, setCopyStatus] = useState<CopyStatus>("idle")
  const [error, setError] = useState<ApplicationError>()

  useEffect(() => {
    let disposed = false
    const unlisteners: UnlistenFn[] = []
    const hold = (subscription: Promise<UnlistenFn>) => {
      void subscription
        .then((unlisten) =>
          disposed ? unlisten() : unlisteners.push(unlisten),
        )
        .catch((caught: unknown) => {
          if (!disposed) setError(toApplicationError(caught))
        })
    }

    hold(
      listenToSessionInsights((publication) => {
        setBoard((current) =>
          receiveInsightBatch(current, publication.insights),
        )
      }),
    )
    hold(listenToDetachedWindowAppearance("insights", setAppearance))
    hold(
      listenToDetachedWindowInteraction("insights", (next) => {
        setClickThrough(next.clickThrough)
      }),
    )

    return () => {
      disposed = true
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, [])

  const cards = board.cards
  const shown = board.focus
  const current = cards[shown]
  const pinned = pinnedCount(board)

  const copy = async (card: InsightCard) => {
    try {
      await navigator.clipboard.writeText(insightAsText(card.insight))
      setCopyStatus("copied")
    } catch {
      setCopyStatus("failed")
    }
  }

  const compact = appearance?.compact ?? false
  // Only the background carries the alpha. Text and borders stay fully opaque
  // so lowering opacity never costs readability.
  const backgroundStyle = {
    "--insights-window-opacity": String(appearance?.backgroundOpacity ?? 1),
  } as React.CSSProperties

  return (
    <div
      className={cn(
        "text-foreground relative flex h-svh flex-col",
        compact ? "gap-1.5 p-1.5" : "gap-2 p-3",
      )}
      data-compact={compact ? "true" : undefined}
      data-testid="detached-insights-window"
      style={backgroundStyle}
    >
      <div
        aria-hidden="true"
        className="bg-background absolute inset-0 -z-10"
        style={{ opacity: "var(--insights-window-opacity)" }}
      />
      <header className="flex items-center justify-between gap-2">
        <p className={cn("font-medium", compact ? "text-xs" : "text-sm")}>
          Insights
        </p>
        <div className="flex items-center gap-1">
          {clickThrough && (
            // Without this a click-through window looks identical to a frozen
            // one, and the user has no cue why the mouse does nothing.
            <Badge variant="outline">Clicks pass through</Badge>
          )}
          {pinned > 0 && <Badge variant="outline">{pinned} pinned</Badge>}
          <Badge variant={cards.length > 0 ? "secondary" : "outline"}>
            {cards.length > 0
              ? `${shown + 1} of ${cards.length}`
              : "Waiting for insights"}
          </Badge>
        </div>
      </header>

      {error && <SanitizedErrorPanel error={error} />}

      <div
        className={cn(
          "min-h-0 flex-1 overflow-y-auto rounded-lg border",
          compact ? "p-1.5" : "p-3",
        )}
      >
        {current ? (
          <InsightCardView compact={compact} card={current} />
        ) : (
          <div className="grid size-full place-items-center text-center">
            <p className="text-muted-foreground text-xs">
              Insights generated while this window is open appear here. Use
              <strong> Generate insights</strong> in the main window.
            </p>
          </div>
        )}
      </div>

      {current && (
        <div className="flex flex-wrap items-center gap-1">
          <Button
            aria-pressed={current.pinned}
            className={cn(current.pinned && "border-primary")}
            disabled={!current.pinned && pinned >= MAXIMUM_PINNED}
            onClick={() =>
              setBoard((state) => toggleInsightPin(state, current.key))
            }
            size="sm"
            variant="outline"
          >
            <Pin data-icon="inline-start" />
            {current.pinned ? "Pinned" : "Pin"}
          </Button>
          <Button
            onClick={() => {
              setCopyStatus("idle")
              void copy(current)
            }}
            size="sm"
            variant="outline"
          >
            <Copy data-icon="inline-start" /> Copy
          </Button>
          <Button
            onClick={() =>
              setBoard((state) => dismissInsight(state, current.key))
            }
            size="sm"
            variant="ghost"
          >
            <X data-icon="inline-start" /> Dismiss
          </Button>
          {copyStatus !== "idle" && (
            <span className="text-muted-foreground text-xs" role="status">
              {copyStatus === "copied"
                ? "Copied to the clipboard"
                : "The clipboard is not available"}
            </span>
          )}
          {!current.pinned && pinned >= MAXIMUM_PINNED && (
            <span className="text-muted-foreground text-xs">
              {MAXIMUM_PINNED} pinned is the limit — unpin one first.
            </span>
          )}
        </div>
      )}

      {cards.length > 1 && (
        <nav aria-label="Insight navigation" className="flex gap-2">
          <Button
            className="flex-1"
            disabled={shown === 0}
            onClick={() => setBoard((state) => focusInsight(state, shown - 1))}
            size="sm"
            variant="outline"
          >
            <ChevronLeft data-icon="inline-start" /> Previous
          </Button>
          <Button
            className="flex-1"
            disabled={shown === cards.length - 1}
            onClick={() => setBoard((state) => focusInsight(state, shown + 1))}
            size="sm"
            variant="outline"
          >
            Next <ChevronRight data-icon="inline-end" />
          </Button>
        </nav>
      )}
    </div>
  )
}

function InsightCardView({
  card,
  compact,
}: {
  card: InsightCard
  compact: boolean
}) {
  const { insight } = card
  return (
    <article className={cn(compact ? "space-y-1" : "space-y-2")}>
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="secondary">{INSIGHT_LABELS[insight.type]}</Badge>
        {card.pinned && <Badge variant="outline">Pinned</Badge>}
        {insight.confidence !== undefined && (
          <Badge variant="outline">
            {Math.round(insight.confidence * 100)}% confidence
          </Badge>
        )}
      </div>
      <h1 className="text-sm font-medium break-words">{insight.title}</h1>
      <p
        className={cn(
          "break-words whitespace-pre-wrap",
          compact ? "text-xs leading-5" : "text-sm leading-6",
        )}
      >
        {insight.content}
      </p>
      {insight.rationale && (
        <p className="text-muted-foreground text-xs break-words whitespace-pre-wrap">
          {insight.rationale}
        </p>
      )}
      {insight.relatedSegmentIds.length > 0 && (
        <p className="text-muted-foreground text-xs">
          From {insight.relatedSegmentIds.length} transcript{" "}
          {insight.relatedSegmentIds.length === 1 ? "segment" : "segments"}
        </p>
      )}
    </article>
  )
}
