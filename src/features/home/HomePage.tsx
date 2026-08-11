import { CalendarClock, FolderKanban, Pencil, Plus } from "lucide-react"
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
import { type Project } from "@/contracts/projects"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { ProjectForm } from "@/features/projects/ProjectForm"
import { ProjectSessions } from "@/features/sessions/ProjectSessions"
import {
  useCreateProjectMutation,
  useProjectsQuery,
  useUpdateProjectMutation,
} from "@/features/projects/use-projects"
import { useSettingsQuery } from "@/features/settings/use-settings-query"

export function HomePage() {
  const projectsQuery = useProjectsQuery()
  const settingsQuery = useSettingsQuery()
  const createMutation = useCreateProjectMutation()
  const updateMutation = useUpdateProjectMutation()
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<Project>()
  const [sessionsProject, setSessionsProject] = useState<Project>()
  const projects = projectsQuery.data?.pages.flatMap((page) => page.items) ?? []
  const mutationError = createMutation.error ?? updateMutation.error
  const busy = createMutation.isPending || updateMutation.isPending

  const closeForm = () => {
    setCreating(false)
    setEditing(undefined)
    createMutation.reset()
    updateMutation.reset()
  }

  return (
    <div className="space-y-8">
      <section className="bg-card flex flex-col gap-5 rounded-2xl border p-6 shadow-sm md:flex-row md:items-center md:justify-between md:p-8">
        <div className="max-w-2xl space-y-3">
          <Badge variant="secondary">Local project workspace</Badge>
          <div>
            <h1 className="text-3xl font-semibold tracking-tight">
              Keep every meeting grounded in context.
            </h1>
            <p className="text-muted-foreground mt-2 text-sm leading-6 md:text-base">
              Projects organize participants, reusable context, and local model
              defaults. Markdown remains the portable source of truth.
            </p>
          </div>
        </div>
        <Button
          disabled={settingsQuery.data === undefined}
          onClick={() => {
            setEditing(undefined)
            setCreating(true)
          }}
          size="lg"
        >
          <Plus data-icon="inline-start" />
          Create project
        </Button>
      </section>

      {(creating || editing) && (
        <ProjectForm
          busy={busy}
          defaultPresetId={settingsQuery.data?.defaultPresetId}
          defaultTranscriptionModelId={
            settingsQuery.data?.defaultTranscriptionModelId
          }
          key={editing?.id ?? "create"}
          onCancel={closeForm}
          onCreate={(input) =>
            createMutation.mutate(input, { onSuccess: closeForm })
          }
          onUpdate={(value) => {
            if (!editing) return
            updateMutation.mutate(
              {
                projectId: editing.id,
                expectedRevision: editing.revision,
                value,
              },
              { onSuccess: closeForm },
            )
          }}
          project={editing}
        />
      )}

      {mutationError && <SanitizedErrorPanel error={mutationError} />}
      {projectsQuery.error && (
        <SanitizedErrorPanel error={projectsQuery.error} />
      )}

      <section aria-labelledby="projects-title" className="space-y-4">
        <div className="flex items-end justify-between gap-4">
          <div>
            <p className="text-muted-foreground text-sm font-medium">
              Portable workspace
            </p>
            <h2 className="text-xl font-semibold" id="projects-title">
              Projects
            </h2>
          </div>
          <Badge variant="outline">{projects.length} loaded</Badge>
        </div>

        {projectsQuery.isPending ? (
          <Card>
            <CardContent className="text-muted-foreground p-6 text-sm">
              Loading local projects…
            </CardContent>
          </Card>
        ) : projects.length === 0 && !projectsQuery.error ? (
          <Card>
            <CardContent className="grid min-h-40 place-items-center p-6 text-center">
              <div className="space-y-2">
                <FolderKanban className="text-muted-foreground mx-auto size-5" />
                <p className="text-sm font-medium">No projects yet</p>
                <p className="text-muted-foreground text-xs">
                  Create one to establish reusable meeting context.
                </p>
              </div>
            </CardContent>
          </Card>
        ) : (
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {projects.map((project) => (
              <Card key={project.id}>
                <CardHeader>
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <CardTitle className="truncate">{project.name}</CardTitle>
                      <CardDescription className="mt-1 line-clamp-2">
                        {project.description || "No description"}
                      </CardDescription>
                    </div>
                    <Button
                      aria-label={`Edit ${project.name}`}
                      onClick={() => {
                        setCreating(false)
                        setEditing(project)
                      }}
                      size="icon"
                      variant="ghost"
                    >
                      <Pencil />
                    </Button>
                  </div>
                </CardHeader>
                <CardContent className="space-y-3">
                  <p className="text-muted-foreground line-clamp-3 text-xs leading-5">
                    {project.globalContext || "No global context"}
                  </p>
                  <div className="flex flex-wrap gap-1.5">
                    {project.tags.map((tag) => (
                      <Badge key={tag} variant="secondary">
                        {tag}
                      </Badge>
                    ))}
                  </div>
                  <p className="text-muted-foreground text-xs">
                    Revision {project.revision} · {project.participants.length}{" "}
                    participants
                  </p>
                  <Button
                    onClick={() => setSessionsProject(project)}
                    size="sm"
                    variant="outline"
                  >
                    <CalendarClock /> Manage sessions
                  </Button>
                </CardContent>
              </Card>
            ))}
          </div>
        )}

        {projectsQuery.hasNextPage && (
          <div className="flex justify-center">
            <Button
              disabled={projectsQuery.isFetchingNextPage}
              onClick={() => void projectsQuery.fetchNextPage()}
              variant="outline"
            >
              {projectsQuery.isFetchingNextPage ? "Loading…" : "Load more"}
            </Button>
          </div>
        )}
      </section>

      {sessionsProject && (
        <ProjectSessions
          key={sessionsProject.id}
          onClose={() => setSessionsProject(undefined)}
          project={sessionsProject}
        />
      )}
    </div>
  )
}
