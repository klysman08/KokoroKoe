import { Lightbulb, PanelRightOpen, Settings } from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import {
  generateRecentInsightsRequestSchema,
  type RecentInsight,
} from "@/contracts/insights"
import type { Project, Session } from "@/contracts/projects"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { formatUsd } from "@/features/dashboard/format"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"
import {
  formatInsightMoment,
  navigableMoments,
} from "@/features/insights/insight-moments"
import { generateRecentInsights } from "@/lib/tauri/insights"
import { openDetachedWindow } from "@/lib/tauri/windows"
import { useNavigationStore } from "@/stores/navigation-store"

/** At most one automatic generation this often (ADR 0012). */
export const AUTO_INSIGHT_INTERVAL_MS = 90_000
/** The first automatic generation waits for this much finalized speech. */
export const AUTO_INSIGHT_FIRST_FINALS = 3
const TICK_MS = 5_000
const KEPT_INSIGHTS = 40

/**
 * Failures that the next attempt would repeat. Automatic generation switches
 * itself off on these, so a missing key or an exhausted budget cannot keep
 * spending attempts; anything else is shown and the next tick tries again.
 */
const STOPPING_CODES = new Set([
  "insight_credential_required",
  "insight_authentication_failed",
  "insight_payment_required",
  "insight_model_required",
  "insight_types_required",
  "insight_budget_exceeded",
  "insight_catalog_required",
  "insight_provider_requirements_unavailable",
  "insight_request_rejected",
])

const insightKey = (insight: RecentInsight) =>
  `${insight.type}\u0000${insight.title}\u0000${insight.content}`

/**
 * Insights for the meeting in front of the user.
 *
 * Unlike the project view's panel, new batches are added above earlier ones
 * rather than replacing them, so what was said ten minutes ago is still there
 * to read. Nothing is stored: the list lives as long as this view.
 */
export function MeetingInsights({
  project,
  session,
  finals,
}: {
  project: Project
  session: Session
  /** Finalized segments received live for this session. */
  finals: number
}) {
  const navigate = useNavigationStore((state) => state.navigate)
  const [auto, setAuto] = useState(true)
  const [stopReason, setStopReason] = useState<string>()
  const [insights, setInsights] = useState<RecentInsight[]>([])
  const [error, setError] = useState<ApplicationError>()
  const [pending, setPending] = useState(false)
  const [spent, setSpent] = useState<string>()
  // When the last generation answered and how many insights it returned, so
  // a check that found nothing reads as a check rather than as silence.
  const [lastCheck, setLastCheck] = useState<{ at: number; returned: number }>()
  const lastRun = useRef<{ at: number; finals: number }>({ at: 0, finals: 0 })
  const inFlight = useRef(false)

  const configured =
    session.llmModels.insights !== undefined &&
    session.preset.insightTypes.length > 0

  const generate = useCallback(
    async (trigger: "auto" | "manual") => {
      if (inFlight.current) return
      const request = generateRecentInsightsRequestSchema.safeParse({
        requestId: globalThis.crypto.randomUUID(),
        projectId: project.id,
        sessionId: session.id,
      })
      if (!request.success) return
      inFlight.current = true
      lastRun.current = { at: Date.now(), finals }
      setPending(true)
      setError(undefined)
      try {
        const batch = await generateRecentInsights(request.data)
        setSpent(batch.sessionActualCostUsd)
        setLastCheck({ at: Date.now(), returned: batch.insights.length })
        setInsights((current) => {
          const seen = new Set(current.map(insightKey))
          const fresh = batch.insights.filter(
            (insight) => !seen.has(insightKey(insight)),
          )
          return [...fresh, ...current].slice(0, KEPT_INSIGHTS)
        })
      } catch (caught: unknown) {
        const failure = toApplicationError(caught)
        setError(failure)
        if (trigger === "auto" && STOPPING_CODES.has(failure.details.code)) {
          setAuto(false)
          setStopReason(
            "Automatic insights stopped because the same problem would repeat. Fix it, then switch them back on.",
          )
        }
      } finally {
        inFlight.current = false
        setPending(false)
      }
    },
    [finals, project.id, session.id],
  )

  useEffect(() => {
    if (!auto || !configured || session.state !== "transcribing") return
    const tick = () => {
      const { at, finals: seen } = lastRun.current
      const firstRun = at === 0
      const due = firstRun
        ? finals >= AUTO_INSIGHT_FIRST_FINALS
        : finals > seen && Date.now() - at >= AUTO_INSIGHT_INTERVAL_MS
      if (due) void generate("auto")
    }
    tick()
    const timer = window.setInterval(tick, TICK_MS)
    return () => window.clearInterval(timer)
  }, [auto, configured, finals, generate, session.state])

  if (!configured) {
    return (
      <section aria-label="Meeting insights" className="space-y-3">
        <h2 className="text-sm font-semibold">Insights</h2>
        <div className="rounded-xl border border-dashed p-4 text-sm">
          <p className="font-medium">Insights are not set up</p>
          <p className="text-muted-foreground mt-1 text-xs leading-5">
            Add an OpenRouter key and choose an insights model in Settings. They
            apply to the next meeting you start; this one keeps transcribing
            either way.
          </p>
          <Button
            className="mt-3"
            onClick={() => navigate("settings")}
            size="sm"
            variant="outline"
          >
            <Settings data-icon="inline-start" /> Open Settings
          </Button>
        </div>
      </section>
    )
  }

  return (
    <section aria-label="Meeting insights" className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-sm font-semibold">Insights</h2>
        <div className="flex items-center gap-1">
          <Button
            disabled={pending}
            onClick={() => void generate("manual")}
            size="sm"
            type="button"
            variant="outline"
          >
            <Lightbulb data-icon="inline-start" />
            {pending ? "Thinking…" : "Generate now"}
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

      <label className="flex items-start gap-2 text-xs">
        <input
          checked={auto}
          className="mt-0.5"
          onChange={(event) => {
            setAuto(event.target.checked)
            setStopReason(undefined)
          }}
          type="checkbox"
        />
        <span>
          <span className="font-medium">Automatic insights</span>
          <span className="text-muted-foreground block leading-5">
            While you talk, the latest transcript text — never audio — is sent
            to your insights model at most every{" "}
            {AUTO_INSIGHT_INTERVAL_MS / 1_000} seconds, and only after new
            speech, within this meeting's budget.
            {spent && ` Spent so far: ${formatUsd(spent)}.`}
          </span>
        </span>
      </label>
      {stopReason && (
        <p className="text-xs text-amber-700 dark:text-amber-400" role="status">
          {stopReason}
        </p>
      )}
      {error && <SanitizedErrorPanel error={error} />}
      {lastCheck && (
        <p className="text-muted-foreground text-xs" role="status">
          {lastCheck.returned === 0
            ? `Checked at ${formatClock(lastCheck.at)}: nothing new to add yet.`
            : `Updated at ${formatClock(lastCheck.at)}.`}
        </p>
      )}

      {insights.length === 0 ? (
        <p className="text-muted-foreground rounded-xl border border-dashed p-4 text-xs leading-5">
          {session.state === "transcribing"
            ? auto
              ? "Insights appear here once there is enough conversation."
              : "Press Generate now to get insights about the latest conversation."
            : "Insights are generated while the meeting is live."}
        </p>
      ) : (
        <ul aria-live="polite" className="flex flex-col gap-3">
          {insights.map((insight) => (
            <li className="rounded-lg border p-3" key={insightKey(insight)}>
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
                <p className="text-muted-foreground mt-1 text-xs">
                  From{" "}
                  {navigableMoments(insight.relatedMoments)
                    .map(formatInsightMoment)
                    .join(", ")}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

function formatClock(at: number) {
  return new Intl.DateTimeFormat(undefined, { timeStyle: "short" }).format(
    new Date(at),
  )
}
