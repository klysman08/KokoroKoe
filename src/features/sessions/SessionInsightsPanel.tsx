import { useState } from "react"
import { Lightbulb, PanelRightClose, PanelRightOpen } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import {
  generateRecentInsightsRequestSchema,
  type RecentInsightsResponse,
} from "@/contracts/insights"
import type { Project, Session } from "@/contracts/projects"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"
import {
  closeInsightsWindow,
  generateRecentInsights,
  openInsightsWindow,
} from "@/lib/tauri/insights"

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

  async function generate() {
    const request = generateRecentInsightsRequestSchema.safeParse({
      requestId: globalThis.crypto.randomUUID(),
      projectId: project.id,
      sessionId: session.id,
    })
    if (!request.success) return

    setError(undefined)
    setResponse(undefined)
    setPending(true)
    try {
      setResponse(await generateRecentInsights(request.data))
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
              void openInsightsWindow().catch((caught: unknown) =>
                setError(toApplicationError(caught)),
              )
            }
            size="icon"
            type="button"
            variant="ghost"
          >
            <PanelRightOpen />
          </Button>
          <Button
            aria-label="Close insights window"
            onClick={() =>
              void closeInsightsWindow().catch((caught: unknown) =>
                setError(toApplicationError(caught)),
              )
            }
            size="icon"
            type="button"
            variant="ghost"
          >
            <PanelRightClose />
          </Button>
        </div>
      </div>
      <p className="text-muted-foreground mb-2 text-xs">
        Sends only the most recent finalized transcript text to the Session's
        insights model. Results are shown once and are never saved. Popping the
        insights out shows each generated batch in its own window, one insight
        at a time.
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
            <Badge variant="outline">
              ${response.sessionActualCostUsd} session cost
            </Badge>
            <Badge variant="outline">
              ${response.availableBudgetUsd} remaining
            </Badge>
          </div>
        </div>
      )}
    </section>
  )
}
