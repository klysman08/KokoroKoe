import { useState } from "react"
import { Lightbulb, PanelRightOpen, RefreshCw } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import {
  generateAlternativeInsightRequestSchema,
  generateRecentInsightsRequestSchema,
  type RecentInsight,
  type RecentInsightsResponse,
} from "@/contracts/insights"
import type { Project, Session } from "@/contracts/projects"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"
import {
  formatInsightMoment,
  navigableMoments,
} from "@/features/insights/insight-moments"
import {
  generateAlternativeInsight,
  generateRecentInsights,
} from "@/lib/tauri/insights"
import { openDetachedWindow } from "@/lib/tauri/windows"

/**
 * Transient recent-transcript insights for one active Session.
 *
 * Generation only ever runs from an explicit click, and the returned text is
 * held in component state: it is never stored, persisted, or written to the
 * Session Markdown.
 */
export function SessionInsightsPanel({
  project,
  session,
}: {
  project: Project
  session: Session
}) {
  const [response, setResponse] = useState<RecentInsightsResponse>()
  const [error, setError] = useState<ApplicationError>()
  const [pending, setPending] = useState(false)
  /**
   * Second readings, kept beside the insight they came from.
   *
   * An alternative is another opinion about the same moment, so it belongs
   * next to the original rather than replacing it: the user asked to compare,
   * and throwing the first one away would answer a question they did not ask.
   */
  const [alternatives, setAlternatives] = useState<
    Record<string, RecentInsight[]>
  >({})
  const [alternativeFor, setAlternativeFor] = useState<string>()
  /** The newest money figures, whichever generation produced them. */
  const [spend, setSpend] = useState<{
    sessionActualCostUsd: string
    availableBudgetUsd: string
  }>()

  const insightKey = (insight: RecentInsight) =>
    `${insight.type} ${insight.title}`

  async function generateAlternative(insight: RecentInsight) {
    const key = insightKey(insight)
    const request = generateAlternativeInsightRequestSchema.safeParse({
      requestId: globalThis.crypto.randomUUID(),
      projectId: project.id,
      sessionId: session.id,
      insightType: insight.type,
      segmentIds: insight.relatedSegmentIds,
    })
    if (!request.success) return

    setError(undefined)
    setAlternativeFor(key)
    try {
      const another = await generateAlternativeInsight(request.data)
      setAlternatives((current) => ({
        ...current,
        [key]: [...(current[key] ?? []), ...another.insights],
      }))
      setSpend({
        sessionActualCostUsd: another.sessionActualCostUsd,
        availableBudgetUsd: another.availableBudgetUsd,
      })
    } catch (caught: unknown) {
      setError(toApplicationError(caught))
    } finally {
      setAlternativeFor(undefined)
    }
  }

  async function generate() {
    const request = generateRecentInsightsRequestSchema.safeParse({
      requestId: globalThis.crypto.randomUUID(),
      projectId: project.id,
      sessionId: session.id,
    })
    if (!request.success) return

    setError(undefined)
    setResponse(undefined)
    setAlternatives({})
    setPending(true)
    try {
      const batch = await generateRecentInsights(request.data)
      setResponse(batch)
      setSpend({
        sessionActualCostUsd: batch.sessionActualCostUsd,
        availableBudgetUsd: batch.availableBudgetUsd,
      })
    } catch (caught: unknown) {
      setError(toApplicationError(caught))
    } finally {
      setPending(false)
    }
  }

  return (
    <section aria-label={`Recent insights for ${session.title}`}>
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-xs font-medium">Recent insights</p>
        <div className="flex items-center gap-1">
          <Button
            disabled={pending}
            onClick={() => void generate()}
            size="sm"
            type="button"
            variant="outline"
          >
            <Lightbulb aria-hidden="true" />
            {pending ? "Generating…" : "Generate insights"}
          </Button>
          <Button
            aria-label="Pop out insights"
            onClick={() =>
              void openDetachedWindow("insights").catch((caught: unknown) =>
                setError(toApplicationError(caught)),
              )
            }
            size="icon"
            type="button"
            variant="ghost"
          >
            <PanelRightOpen />
          </Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-2 text-xs">
        Sends only the most recent finalized transcript text to the Session's
        insights model. Results are shown once and are never saved. Popping the
        insights out shows each generated batch in its own window, one insight
        at a time; its opacity, position, and shortcut live in the sidebar.
      </p>
      {error && <SanitizedErrorPanel error={error} />}
      {response && (
        <div aria-live="polite" className="flex flex-col gap-3">
          {response.insights.length === 0 ? (
            <p className="text-muted-foreground text-xs">
              The model returned no insights for the recent transcript.
            </p>
          ) : (
            <ul className="flex flex-col gap-3">
              {response.insights.map((insight) => (
                <li
                  className="rounded-lg border p-3"
                  key={`${insight.type} ${insight.title}`}
                >
                  <div className="mb-1 flex flex-wrap items-center gap-2">
                    <Badge variant="secondary">
                      {INSIGHT_LABELS[insight.type]}
                    </Badge>
                    <span className="text-sm font-medium">{insight.title}</span>
                  </div>
                  <p className="text-sm leading-6 break-words whitespace-pre-wrap">
                    {insight.content}
                  </p>
                  {insight.rationale && (
                    <p className="text-muted-foreground mt-1 text-xs break-words">
                      {insight.rationale}
                    </p>
                  )}
                  {navigableMoments(insight.relatedMoments).length > 0 && (
                    // Which moments this came from. Rust resolved these against
                    // the segments the model was shown, so each one names a
                    // place that is really in the transcript.
                    <p className="text-muted-foreground mt-1 text-xs">
                      From{" "}
                      {navigableMoments(insight.relatedMoments)
                        .map(formatInsightMoment)
                        .join(", ")}
                    </p>
                  )}
                  {insight.relatedSegmentIds.length > 0 && (
                    <Button
                      className="mt-2"
                      disabled={alternativeFor !== undefined}
                      onClick={() => void generateAlternative(insight)}
                      size="xs"
                      type="button"
                      variant="ghost"
                    >
                      <RefreshCw aria-hidden="true" data-icon="inline-start" />
                      {alternativeFor === insightKey(insight)
                        ? "Asking again…"
                        : "Ask again"}
                    </Button>
                  )}
                  {(alternatives[insightKey(insight)] ?? []).map(
                    (alternative, index) => (
                      <div
                        className="mt-2 border-l-2 pl-3"
                        key={`${alternative.title} ${index}`}
                      >
                        <p className="text-muted-foreground text-xs">
                          Another reading
                        </p>
                        <p className="text-sm font-medium">
                          {alternative.title}
                        </p>
                        <p className="text-sm leading-6 break-words whitespace-pre-wrap">
                          {alternative.content}
                        </p>
                        {navigableMoments(alternative.relatedMoments).length >
                          0 && (
                          <p className="text-muted-foreground mt-1 text-xs">
                            From{" "}
                            {navigableMoments(alternative.relatedMoments)
                              .map(formatInsightMoment)
                              .join(", ")}
                          </p>
                        )}
                      </div>
                    ),
                  )}
                </li>
              ))}
            </ul>
          )}
          <div className="flex flex-wrap gap-2">
            <Badge variant="outline">
              {response.primaryUsage.inputTokens +
                response.primaryUsage.outputTokens}{" "}
              tokens
            </Badge>
            {response.repaired && (
              <Badge variant="secondary">JSON repaired</Badge>
            )}
            {/*
              The newest figures, whichever generation produced them. Asking
              again costs money too, so showing the batch's original numbers
              afterwards would understate what the session has spent.
            */}
            <Badge variant="outline">
              ${spend?.sessionActualCostUsd ?? response.sessionActualCostUsd}{" "}
              session cost
            </Badge>
            <Badge variant="outline">
              ${spend?.availableBudgetUsd ?? response.availableBudgetUsd}{" "}
              remaining
            </Badge>
          </div>
        </div>
      )}
    </section>
  )
}
