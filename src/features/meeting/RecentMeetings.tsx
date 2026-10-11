import { useQueries } from "@tanstack/react-query"
import { CalendarClock, ChevronRight } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { type Project } from "@/contracts/projects"
import { formatDateTime } from "@/features/dashboard/format"
import { listSessions } from "@/lib/tauri/sessions"
import { useMeetingStore } from "@/stores/meeting-store"
import { useNavigationStore } from "@/stores/navigation-store"

import { MEETING_STATE_LABELS } from "./meeting-labels"

/** Enough projects for a casual user; the Projects page lists them all. */
const PROJECT_LIMIT = 12
const PER_PROJECT = 12
const SHOWN = 8

/**
 * The latest meetings across every project, newest first.
 *
 * Sessions are listed per project, so this reads the first page of each of the
 * first projects and merges them. A long-lived workspace may hold older
 * meetings than these; they stay reachable from the Projects page.
 */
export function RecentMeetings({ projects }: { projects: Project[] }) {
  const open = useMeetingStore((state) => state.open)
  const navigate = useNavigationStore((state) => state.navigate)
  const scope = projects.slice(0, PROJECT_LIMIT)
  const results = useQueries({
    queries: scope.map((project) => ({
      queryKey: ["recent-meetings", project.id],
      queryFn: () =>
        listSessions({ projectId: project.id, limit: PER_PROJECT }),
      gcTime: 0,
    })),
  })
  const loading = results.some((result) => result.isPending)
  const meetings = results
    .flatMap((result, index) =>
      (result.data?.items ?? []).map((session) => ({
        session,
        project: scope[index]!,
      })),
    )
    .sort((left, right) =>
      right.session.createdAt.localeCompare(left.session.createdAt),
    )
    .slice(0, SHOWN)

  return (
    <section aria-labelledby="recent-meetings-title" className="space-y-3">
      <h2 className="text-lg font-semibold" id="recent-meetings-title">
        Recent meetings
      </h2>
      {loading && meetings.length === 0 ? (
        <p className="text-muted-foreground text-sm">Loading meetings…</p>
      ) : meetings.length === 0 ? (
        <div className="bg-muted/30 grid min-h-28 place-items-center rounded-xl border border-dashed p-6 text-center">
          <div>
            <CalendarClock className="text-muted-foreground mx-auto size-5" />
            <p className="mt-2 text-sm font-medium">No meetings yet</p>
            <p className="text-muted-foreground text-xs">
              Your meetings appear here once you start one.
            </p>
          </div>
        </div>
      ) : (
        <ul className="divide-y rounded-xl border">
          {meetings.map(({ session, project }) => (
            <li key={session.id}>
              <Button
                className="h-auto w-full justify-between gap-3 rounded-none px-4 py-3 text-left"
                onClick={() => {
                  open({ projectId: project.id, sessionId: session.id })
                  navigate("meeting")
                }}
                variant="ghost"
              >
                <span className="min-w-0">
                  <span className="block truncate text-sm font-medium">
                    {session.title}
                  </span>
                  <span className="text-muted-foreground block truncate text-xs font-normal">
                    {project.name} · {formatDateTime(session.createdAt)}
                  </span>
                </span>
                <span className="flex shrink-0 items-center gap-2">
                  <Badge
                    variant={
                      session.state === "transcribing" ? "default" : "secondary"
                    }
                  >
                    {MEETING_STATE_LABELS[session.state]}
                  </Badge>
                  <ChevronRight aria-hidden="true" className="size-4" />
                </span>
              </Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
