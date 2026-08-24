import { useEffect, useState } from "react"
import { ChevronLeft, ChevronRight } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import type { RecentInsight } from "@/contracts/insights"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"
import { listenToSessionInsights } from "@/lib/tauri/insights"

/**
 * The detached insights window.
 *
 * Like the transcript window it runs in its own webview whose capability grants
 * event subscription only: it invokes no command, so it can neither ask for a
 * generation nor retrieve an earlier one. It shows the batches that arrive
 * while it is open, one insight at a time, and keeps them in view state only —
 * insights are never saved, here or anywhere else.
 */
export function DetachedInsightsWindow() {
  const [insights, setInsights] = useState<RecentInsight[]>([])
  const [position, setPosition] = useState(0)
  const [error, setError] = useState<ApplicationError>()

  useEffect(() => {
    let disposed = false
    let dispose: (() => void) | undefined

    void listenToSessionInsights((publication) => {
      // A new batch replaces the previous one rather than accumulating: these
      // are observations about the last few minutes, not a growing record.
      setInsights(publication.insights)
      setPosition(0)
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

  const current = insights[position]

  return (
    <div
      className="text-foreground bg-background flex h-svh flex-col gap-2 p-3"
      data-testid="detached-insights-window"
    >
      <header className="flex items-center justify-between gap-2">
        <p className="text-sm font-medium">Insights</p>
        <Badge variant={insights.length > 0 ? "secondary" : "outline"}>
          {insights.length > 0
            ? `${position + 1} of ${insights.length}`
            : "Waiting for insights"}
        </Badge>
      </header>

      {error && <SanitizedErrorPanel error={error} />}

      <div className="min-h-0 flex-1 overflow-y-auto rounded-lg border p-3">
        {current ? (
          <InsightCard insight={current} />
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

function InsightCard({ insight }: { insight: RecentInsight }) {
  return (
    <article className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="secondary">{INSIGHT_LABELS[insight.type]}</Badge>
        {insight.confidence !== undefined && (
          <Badge variant="outline">
            {Math.round(insight.confidence * 100)}% confidence
          </Badge>
        )}
      </div>
      <h1 className="text-sm font-medium break-words">{insight.title}</h1>
      <p className="text-sm leading-6 break-words whitespace-pre-wrap">
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
