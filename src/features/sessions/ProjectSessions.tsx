import { useQuery } from "@tanstack/react-query"
import { CalendarClock, Pencil, Plus } from "lucide-react"
import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { type Project, type Session } from "@/contracts/projects"
import { type ApplicationError } from "@/contracts/app-error"
import { type AudioDeviceList } from "@/contracts/audio"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { SessionForm } from "@/features/sessions/SessionForm"
import {
  useCreateSessionMutation,
  useSessionsQuery,
  useUpdateSessionMutation,
} from "@/features/sessions/use-sessions"
import { listAudioDevices } from "@/lib/tauri/audio"

export function ProjectSessions({
  project,
  onClose,
}: {
  project: Project
  onClose: () => void
}) {
  const sessionsQuery = useSessionsQuery(project.id, true)
  const devicesQuery = useQuery<AudioDeviceList, ApplicationError>({
    queryKey: ["audio-devices", "session-form"],
    queryFn: listAudioDevices,
    gcTime: 0,
  })
  const createMutation = useCreateSessionMutation(project.id)
  const updateMutation = useUpdateSessionMutation(project.id)
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<Session>()
  const sessions = sessionsQuery.data?.pages.flatMap((page) => page.items) ?? []
  const busy = createMutation.isPending || updateMutation.isPending
  const closeForm = () => {
    setCreating(false)
    setEditing(undefined)
    createMutation.reset()
    updateMutation.reset()
  }

  return (
    <Card>
      <CardHeader className="flex-row items-start justify-between gap-4">
        <div>
          <CardTitle>{project.name} sessions</CardTitle>
          <CardDescription>
            Session metadata is saved to project-scoped portable Markdown.
          </CardDescription>
        </div>
        <div className="flex gap-2">
          <Button
            disabled={!devicesQuery.data}
            onClick={() => {
              setEditing(undefined)
              setCreating(true)
            }}
            size="sm"
          >
            <Plus />
            New session
          </Button>
          <Button onClick={onClose} size="sm" variant="outline">
            Close
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {(creating || editing) && devicesQuery.data && (
          <SessionForm
            busy={busy}
            devices={devicesQuery.data}
            key={editing?.id ?? "create"}
            onCancel={closeForm}
            onCreate={(value) =>
              createMutation.mutate(
                { projectId: project.id, value },
                { onSuccess: closeForm },
              )
            }
            onUpdate={(value) =>
              editing &&
              updateMutation.mutate(
                {
                  projectId: project.id,
                  sessionId: editing.id,
                  expectedRevision: editing.revision,
                  value,
                },
                { onSuccess: closeForm },
              )
            }
            project={project}
            session={editing}
          />
        )}
        {devicesQuery.error && (
          <SanitizedErrorPanel error={devicesQuery.error} />
        )}
        {(sessionsQuery.error ||
          createMutation.error ||
          updateMutation.error) && (
          <SanitizedErrorPanel
            error={
              (sessionsQuery.error ??
                createMutation.error ??
                updateMutation.error)!
            }
          />
        )}
        {sessionsQuery.isPending ? (
          <p className="text-muted-foreground text-sm">Loading sessionsâ€¦</p>
        ) : sessions.length === 0 ? (
          <div className="bg-muted/30 grid min-h-28 place-items-center rounded-xl border border-dashed p-6 text-center">
            <div>
              <CalendarClock className="text-muted-foreground mx-auto size-5" />
              <p className="mt-2 text-sm font-medium">
                No sessions in this project
              </p>
            </div>
          </div>
        ) : (
          <div className="grid gap-3 md:grid-cols-2">
            {sessions.map((session) => (
              <div className="rounded-xl border p-4" key={session.id}>
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <p className="font-medium">{session.title}</p>
                    <p className="text-muted-foreground mt-1 line-clamp-2 text-xs">
                      {session.objective || "No objective"}
                    </p>
                  </div>
                  <Button
                    aria-label={`Edit ${session.title}`}
                    disabled={session.state !== "idle"}
                    onClick={() => {
                      setCreating(false)
                      setEditing(session)
                    }}
                    size="icon"
                    variant="ghost"
                  >
                    <Pencil />
                  </Button>
                </div>
                <div className="mt-3 flex items-center gap-2">
                  <Badge variant="secondary">{session.state}</Badge>
                  <span className="text-muted-foreground text-xs">
                    Revision {session.revision}
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
        {sessionsQuery.hasNextPage && (
          <div className="flex justify-center">
            <Button
              disabled={sessionsQuery.isFetchingNextPage}
              onClick={() => void sessionsQuery.fetchNextPage()}
              variant="outline"
            >
              {sessionsQuery.isFetchingNextPage ? "Loadingâ€¦" : "Load more"}
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  )
}
