import { useState, type FormEvent } from "react"

import {
  createProjectInputSchema,
  updateProjectInputSchema,
  type CreateProjectInput,
  type LlmRoleModels,
  type Project,
  type UpdateProjectInput,
} from "@/contracts/projects"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

type ProjectFormProps = {
  project?: Project | undefined
  defaultPresetId?: string | undefined
  defaultTranscriptionModelId?: string | undefined
  defaultLlmModels?: LlmRoleModels | undefined
  busy: boolean
  onCancel: () => void
  onCreate: (input: CreateProjectInput) => void
  onUpdate: (input: UpdateProjectInput) => void
}

const fieldClass =
  "border-input bg-background focus-visible:ring-ring w-full rounded-md border px-3 py-2 text-sm outline-none focus-visible:ring-2"

function commaList(value: string) {
  return [
    ...new Set(
      value
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  ]
}

export function ProjectForm({
  project,
  defaultPresetId,
  defaultTranscriptionModelId,
  defaultLlmModels,
  busy,
  onCancel,
  onCreate,
  onUpdate,
}: ProjectFormProps) {
  const [name, setName] = useState(project?.name ?? "")
  const [description, setDescription] = useState(project?.description ?? "")
  const [globalContext, setGlobalContext] = useState(
    project?.globalContext ?? "",
  )
  const [participants, setParticipants] = useState(
    project?.participants.join(", ") ?? "",
  )
  const [tags, setTags] = useState(project?.tags.join(", ") ?? "")
  const [validationMessage, setValidationMessage] = useState("")

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const editable = {
      name,
      description,
      globalContext,
      participants: commaList(participants),
      tags: commaList(tags),
    }
    if (project) {
      const parsed = updateProjectInputSchema.safeParse(editable)
      if (!parsed.success) {
        setValidationMessage("Check the project fields and try again.")
        return
      }
      onUpdate(parsed.data)
      return
    }
    const parsed = createProjectInputSchema.safeParse({
      ...editable,
      defaultPresetId,
      defaultTranscriptionModelId,
      preferredLlmModels: defaultLlmModels ?? {},
    })
    if (!parsed.success) {
      setValidationMessage("Check the project fields and setup defaults.")
      return
    }
    onCreate(parsed.data)
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{project ? "Edit project" : "Create project"}</CardTitle>
        <CardDescription>
          Project metadata is saved as portable local Markdown. New projects
          inherit the current preset and transcription-model defaults.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form className="grid gap-4" onSubmit={submit}>
          <label className="grid gap-1.5 text-sm font-medium">
            Project name
            <input
              className={fieldClass}
              disabled={busy}
              maxLength={128}
              onChange={(event) => setName(event.target.value)}
              required
              value={name}
            />
          </label>
          <label className="grid gap-1.5 text-sm font-medium">
            Description
            <textarea
              className={fieldClass}
              disabled={busy}
              maxLength={4096}
              onChange={(event) => setDescription(event.target.value)}
              rows={3}
              value={description}
            />
          </label>
          <label className="grid gap-1.5 text-sm font-medium">
            Global context
            <textarea
              className={fieldClass}
              disabled={busy}
              maxLength={32768}
              onChange={(event) => setGlobalContext(event.target.value)}
              rows={4}
              value={globalContext}
            />
          </label>
          <div className="grid gap-4 md:grid-cols-2">
            <label className="grid gap-1.5 text-sm font-medium">
              Participants
              <input
                className={fieldClass}
                disabled={busy}
                onChange={(event) => setParticipants(event.target.value)}
                placeholder="Product lead, Engineering lead"
                value={participants}
              />
              <span className="text-muted-foreground text-xs">
                Separate entries with commas.
              </span>
            </label>
            <label className="grid gap-1.5 text-sm font-medium">
              Tags
              <input
                className={fieldClass}
                disabled={busy}
                onChange={(event) => setTags(event.target.value)}
                placeholder="product, weekly"
                value={tags}
              />
              <span className="text-muted-foreground text-xs">
                Separate entries with commas.
              </span>
            </label>
          </div>
          <p aria-live="polite" className="text-destructive text-sm">
            {validationMessage}
          </p>
          <div className="flex justify-end gap-3">
            <Button
              disabled={busy}
              onClick={onCancel}
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
            <Button disabled={busy} type="submit">
              {busy ? "Saving…" : "Save project"}
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  )
}
