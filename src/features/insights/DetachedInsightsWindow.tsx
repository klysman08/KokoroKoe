import { useEffect, useState } from "react"
import type { UnlistenFn } from "@tauri-apps/api/event"
import { ChevronLeft, ChevronRight } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import type { RecentInsight } from "@/contracts/insights"
import type { DetachedWindowAppearance } from "@/contracts/windows"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"
import { listenToSessionInsights } from "@/lib/tauri/insights"
import {
  listenToDetachedWindowAppearance,
  listenToDetachedWindowInteraction,
} from "@/lib/tauri/windows"
import { cn } from "@/lib/utils"

/**
 * The detached insights window.
 *
 * Like the transcript window it runs in its own webview whose capability grants
 * event subscription only: it invokes no command, so it can neither ask for a
 * generation nor retrieve an earlier one, and its own opacity, compact layout,
 * and pointer state reach it only as Rust-owned events. It shows the batches
 * that arrive while it is open, one insight at a time, and keeps them in view
 * state only — insights are never saved, here or anywhere else.
 */
export function DetachedInsightsWindow() {
  const [insights, setInsights] = useState<RecentInsight[]>([])
  const [position, setPosition] = useState(0)
  const [appearance, setAppearance] = useState<DetachedWindowAppearance>()
  const [clickThrough, setClickThrough] = useState(false)
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
        // A new batch replaces the previous one rather than accumulating: these
        // are observations about the last few minutes, not a growing record.
        setInsights(publication.insights)
        setPosition(0)
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

  const current = insights[position]
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
          <Badge variant={insights.length > 0 ? "secondary" : "outline"}>
            {insights.length > 0
              ? `${position + 1} of ${insights.length}`
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
          <InsightCard compact={compact} insight={current} />
        ) : (
          <div className="grid size-full place-items-center text-center">
            <p className="text-muted-foreground text-xs">
              Insights generated while this window is open appear here. Use
              <strong> Generate insights</strong> in the main window.
            </p>
          </div>
        )}
      </div>

      {insights.length > 1 && (
        <nav aria-label="Insight navigation" className="flex gap-2">
          <Button
            className="flex-1"
            disabled={position === 0}
            onClick={() => setPosition((index) => index - 1)}
            size="sm"
            variant="outline"
          >
            <ChevronLeft data-icon="inline-start" /> Previous
          </Button>
          <Button
            className="flex-1"
            disabled={position === insights.length - 1}
            onClick={() => setPosition((index) => index + 1)}
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

function InsightCard({
  compact,
  insight,
}: {
  compact: boolean
  insight: RecentInsight
}) {
  return (
    <article className={cn(compact ? "space-y-1" : "space-y-2")}>
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="secondary">{INSIGHT_LABELS[insight.type]}</Badge>
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
