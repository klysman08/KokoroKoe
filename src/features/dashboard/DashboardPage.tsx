import {
  AudioLines,
  Coins,
  HardDrive,
  Lightbulb,
  ListChecks,
  RefreshCw,
  TriangleAlert,
  type LucideIcon,
} from "lucide-react"
import { useState, type ReactNode } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { type DashboardMetrics } from "@/contracts/dashboard"
import { projectIdSchema, type ProjectId } from "@/contracts/projects"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { useProjectsQuery } from "@/features/projects/use-projects"

import {
  formatBytes,
  formatCount,
  formatDateTime,
  formatDuration,
  formatUsd,
} from "./format"
import { useDashboardQuery, type DashboardPeriod } from "./use-dashboard-query"

const PERIODS: { value: DashboardPeriod; label: string }[] = [
  { value: "7d", label: "Last 7 days" },
  { value: "30d", label: "Last 30 days" },
  { value: "all", label: "All time" },
]

const fieldClass =
  "border-input bg-background focus-visible:ring-ring rounded-md border px-3 py-1.5 text-sm outline-none focus-visible:ring-2"

export function DashboardPage() {
  const [period, setPeriod] = useState<DashboardPeriod>("30d")
  const [projectId, setProjectId] = useState<ProjectId>()
  const metricsQuery = useDashboardQuery(period, projectId)
  const projectsQuery = useProjectsQuery()
  const projects = projectsQuery.data?.pages.flatMap((page) => page.items) ?? []
  const projectName = (id: ProjectId) =>
    projects.find((project) => project.id === id)?.name ?? "Unlisted project"

  return (
    <div className="space-y-8">
      <section className="flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
        <div className="space-y-2">
          <Badge variant="secondary">Stays on this device</Badge>
          <h1 className="text-3xl font-semibold tracking-tight">Dashboard</h1>
          <p className="text-muted-foreground max-w-2xl text-sm leading-6">
            Sessions, transcription, model spending, and recent problems, read
            from your workspace and a local ledger. Nothing here is sent
            anywhere.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <div
            aria-label="Period"
            className="bg-muted inline-flex rounded-lg p-0.5"
            role="group"
          >
            {PERIODS.map((option) => (
              <Button
                aria-pressed={period === option.value}
                key={option.value}
                onClick={() => setPeriod(option.value)}
                size="sm"
                variant={period === option.value ? "default" : "ghost"}
              >
                {option.label}
              </Button>
            ))}
          </div>
          <label className="sr-only" htmlFor="dashboard-project">
            Project
          </label>
          <select
            className={fieldClass}
            id="dashboard-project"
            onChange={(event) => {
              const parsed = projectIdSchema.safeParse(event.target.value)
              setProjectId(parsed.success ? parsed.data : undefined)
            }}
            value={projectId ?? ""}
          >
            <option value="">All projects</option>
            {projects.map((project) => (
              <option key={project.id} value={project.id}>
                {project.name}
              </option>
            ))}
          </select>
          <Button
            aria-label="Refresh dashboard"
            disabled={metricsQuery.isFetching}
            onClick={() => void metricsQuery.refetch()}
            size="icon"
            variant="outline"
          >
            <RefreshCw
              className={metricsQuery.isFetching ? "animate-spin" : ""}
            />
          </Button>
        </div>
      </section>

      {metricsQuery.error && <SanitizedErrorPanel error={metricsQuery.error} />}

      {metricsQuery.isPending ? (
        <Card>
          <CardContent className="text-muted-foreground p-6 text-sm">
            Reading local metrics…
          </CardContent>
        </Card>
      ) : (
        metricsQuery.data && (
          <MetricsView metrics={metricsQuery.data} projectName={projectName} />
        )
      )}
    </div>
  )
}

function MetricsView({
  metrics,
  projectName,
}: {
  metrics: DashboardMetrics
  projectName: (id: ProjectId) => string
}) {
  return (
    <>
      <section
        aria-label="Totals"
        className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4"
      >
        <Stat
          icon={ListChecks}
          label="Sessions"
          value={formatCount(metrics.sessions)}
          note="Created in this period"
        />
        <Stat
          icon={AudioLines}
          label="Transcribed speech"
          value={formatDuration(metrics.transcribedMs)}
          note="Overlapping speakers counted once"
        />
        <Stat
          icon={Coins}
          label="Estimated cost"
          value={formatUsd(metrics.actualCostUsd)}
          note={`${formatCount(metrics.llmRequests)} requests · ${formatCount(metrics.inputTokens)} in / ${formatCount(metrics.outputTokens)} out tokens`}
        />
        <Stat
          icon={Lightbulb}
          label="Insights generated"
          value={formatCount(metrics.insightsGenerated)}
          note={`Usage recorded since ${formatDateTime(metrics.usageRecordedSince)}`}
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-3">
        <RankedCard
          description="By sessions created"
          empty="No sessions in this period."
          rows={metrics.projects.map((entry) => ({
            key: entry.projectId,
            label: projectName(entry.projectId),
            value: `${formatCount(entry.sessions)} sessions`,
          }))}
          title="Most used projects"
        />
        <RankedCard
          description="By sessions transcribed"
          empty="No sessions in this period."
          rows={metrics.transcriptionModels.map((entry) => ({
            key: entry.modelId,
            label: entry.modelId,
            value: `${formatCount(entry.sessions)} sessions`,
          }))}
          title="Whisper models"
        />
        <RankedCard
          description="By OpenRouter requests"
          empty="No model requests in this period."
          rows={metrics.llmModels.map((entry) => ({
            key: entry.modelId,
            label: entry.modelId,
            value: `${formatCount(entry.requests)} · ${formatUsd(entry.actualCostUsd)}`,
          }))}
          title="Language models"
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <HardDrive className="size-4" /> Disk space
            </CardTitle>
            <CardDescription>
              All time, whichever period is chosen
              {metrics.disk.complete
                ? ""
                : " · at least, the folder is too large to count fully"}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <Row
              label="Workspace"
              value={formatBytes(metrics.disk.workspaceBytes)}
            />
            <Row
              label="of which Trash"
              value={formatBytes(metrics.disk.trashBytes)}
            />
            <Row
              label="Whisper models"
              value={formatBytes(metrics.disk.modelBytes)}
            />
          </CardContent>
        </Card>

        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <TriangleAlert className="size-4" /> Recent errors
            </CardTitle>
            <CardDescription>
              The newest failures in this period, as reported to you at the
              time.
            </CardDescription>
          </CardHeader>
          <CardContent>
            {metrics.recentErrors.length === 0 ? (
              <p className="text-muted-foreground text-sm">
                No errors in this period.
              </p>
            ) : (
              <ul className="divide-y">
                {metrics.recentErrors.map((error) => (
                  <li className="space-y-1 py-2.5" key={error.correlationId}>
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge
                        variant={
                          error.severity === "warning"
                            ? "outline"
                            : "destructive"
                        }
                      >
                        {error.severity}
                      </Badge>
                      <span className="text-sm">{error.userMessage}</span>
                    </div>
                    <p className="text-muted-foreground text-xs">
                      {formatDateTime(error.occurredAt)} · {error.code} ·
                      Reference {error.correlationId}
                    </p>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
      </section>
    </>
  )
}

function Stat({
  icon: Icon,
  label,
  value,
  note,
}: {
  icon: LucideIcon
  label: string
  value: string
  note: ReactNode
}) {
  return (
    <Card>
      <CardHeader>
        <CardDescription className="flex items-center gap-2">
          <Icon className="size-4" /> {label}
        </CardDescription>
        <CardTitle className="text-2xl tabular-nums">{value}</CardTitle>
      </CardHeader>
      <CardContent className="text-muted-foreground text-xs leading-5">
        {note}
      </CardContent>
    </Card>
  )
}

function RankedCard({
  title,
  description,
  empty,
  rows,
}: {
  title: string
  description: string
  empty: string
  rows: { key: string; label: string; value: string }[]
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent>
        {rows.length === 0 ? (
          <p className="text-muted-foreground text-sm">{empty}</p>
        ) : (
          <ol className="space-y-2 text-sm">
            {rows.map((row) => (
              <li key={row.key}>
                <Row label={row.label} value={row.value} />
              </li>
            ))}
          </ol>
        )}
      </CardContent>
    </Card>
  )
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <span className="min-w-0 truncate" title={label}>
        {label}
      </span>
      <span className="text-muted-foreground shrink-0 tabular-nums">
        {value}
      </span>
    </div>
  )
}
