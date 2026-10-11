import { Copy, Download, Pencil, Plus } from "lucide-react"
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
import { type Preset } from "@/contracts/presets"
import { type Project } from "@/contracts/projects"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"
import { PresetForm } from "@/features/presets/PresetForm"
import { DeleteConfirm } from "@/features/projects/DeleteConfirm"
import {
  useCreatePresetMutation,
  useDeletePresetMutation,
  useDuplicatePresetMutation,
  useExportPresetMutation,
  usePresetsQuery,
  useUpdatePresetMutation,
} from "@/features/presets/use-presets"

/**
 * The preset library of one project.
 *
 * Built-in presets are listed but never editable: they are a compile-time
 * catalog in Rust, and duplicating one is how a user starts from something that
 * already works. Custom presets live in this project's own folder, so they are
 * shown here rather than in Settings.
 */
export function ProjectPresets({
  project,
  onClose,
}: {
  project: Project
  onClose: () => void
}) {
  const presetsQuery = usePresetsQuery(project.id)
  const createMutation = useCreatePresetMutation(project.id)
  const updateMutation = useUpdatePresetMutation(project.id)
  const duplicateMutation = useDuplicatePresetMutation(project.id)
  const deleteMutation = useDeletePresetMutation(project.id)
  const exportMutation = useExportPresetMutation(project.id)
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<Preset>()

  const presets = presetsQuery.data ?? []
  const custom = presets.filter((preset) => !preset.builtIn)
  const builtIn = presets.filter((preset) => preset.builtIn)
  const busy =
    createMutation.isPending ||
    updateMutation.isPending ||
    duplicateMutation.isPending
  const mutationError =
    createMutation.error ??
    updateMutation.error ??
    duplicateMutation.error ??
    deleteMutation.error ??
    exportMutation.error ??
    presetsQuery.error

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
          <CardTitle>{project.name} presets</CardTitle>
          <CardDescription>
            Presets are saved as portable Markdown inside this project folder.
          </CardDescription>
        </div>
        <div className="flex gap-2">
          <Button
            disabled={presetsQuery.data === undefined}
            onClick={() => {
              setEditing(undefined)
              setCreating(true)
            }}
            size="sm"
          >
            <Plus />
            New preset
          </Button>
          <Button onClick={onClose} size="sm" variant="outline">
            Close
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {(creating || editing) && (
          <PresetForm
            busy={busy}
            key={editing?.id ?? "create"}
            onCancel={closeForm}
            onCreate={(value) =>
              createMutation.mutate(value, { onSuccess: closeForm })
            }
            onUpdate={(value) => {
              if (!editing) return
              updateMutation.mutate(
                {
                  presetId: editing.id,
                  expectedRevision: editing.revision,
                  value,
                },
                { onSuccess: closeForm },
              )
            }}
            preset={editing}
          />
        )}

        {mutationError && <SanitizedErrorPanel error={mutationError} />}
        {exportMutation.data && (
          <p aria-live="polite" className="text-muted-foreground text-sm">
            Exported to {exportMutation.data.path}
          </p>
        )}

        <section aria-labelledby="custom-presets-title" className="space-y-3">
          <h3 className="text-sm font-medium" id="custom-presets-title">
            Your presets
          </h3>
          {custom.length === 0 ? (
            <p className="text-muted-foreground text-sm">
              No presets of your own yet. Duplicate a built-in one below to
              start from something that already works.
            </p>
          ) : (
            custom.map((preset) => (
              <PresetRow
                actions={
                  <>
                    <Button
                      aria-label={`Edit ${preset.name}`}
                      disabled={busy}
                      onClick={() => {
                        setCreating(false)
                        setEditing(preset)
                      }}
                      size="sm"
                      variant="ghost"
                    >
                      <Pencil data-icon="inline-start" /> Edit
                    </Button>
                    <Button
                      aria-label={`Duplicate ${preset.name}`}
                      disabled={busy}
                      onClick={() =>
                        duplicateMutation.mutate({
                          presetId: preset.id,
                          name: copyName(preset.name),
                        })
                      }
                      size="sm"
                      variant="ghost"
                    >
                      <Copy data-icon="inline-start" /> Duplicate
                    </Button>
                    <Button
                      aria-label={`Export ${preset.name}`}
                      disabled={exportMutation.isPending}
                      onClick={() =>
                        exportMutation.mutate({ presetId: preset.id })
                      }
                      size="sm"
                      variant="ghost"
                    >
                      <Download data-icon="inline-start" /> Export
                    </Button>
                    <DeleteConfirm
                      busy={deleteMutation.isPending}
                      label={`Delete ${preset.name}`}
                      onDelete={() =>
                        deleteMutation.mutate({
                          presetId: preset.id,
                          expectedRevision: preset.revision,
                        })
                      }
                      what="preset"
                    />
                  </>
                }
                key={preset.id}
                preset={preset}
              />
            ))
          )}
        </section>

        <section aria-labelledby="built-in-presets-title" className="space-y-3">
          <h3 className="text-sm font-medium" id="built-in-presets-title">
            Built in
          </h3>
          {builtIn.map((preset) => (
            <PresetRow
              actions={
                <Button
                  aria-label={`Duplicate ${preset.name}`}
                  disabled={busy}
                  onClick={() =>
                    duplicateMutation.mutate({
                      presetId: preset.id,
                      name: copyName(preset.name),
                    })
                  }
                  size="sm"
                  variant="ghost"
                >
                  <Copy data-icon="inline-start" /> Duplicate
                </Button>
              }
              key={preset.id}
              preset={preset}
            />
          ))}
        </section>
      </CardContent>
    </Card>
  )
}

function PresetRow({
  preset,
  actions,
}: {
  preset: Preset
  actions: React.ReactNode
}) {
  return (
    <div className="space-y-2 rounded-lg border p-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="font-medium">{preset.name}</p>
          <p className="text-muted-foreground mt-1 text-xs">
            {preset.description || "No description"}
          </p>
        </div>
        <Badge variant={preset.builtIn ? "secondary" : "outline"}>
          {preset.builtIn ? "Built in" : `Version ${preset.version}`}
        </Badge>
      </div>
      <p className="text-muted-foreground text-xs">
        Insights:{" "}
        {preset.insightTypes.map((type) => INSIGHT_LABELS[type]).join(", ")}
      </p>
      <div className="flex flex-wrap items-center gap-1">{actions}</div>
    </div>
  )
}

/** A copy is named for what it came from, within the name's own limit. */
function copyName(name: string) {
  return `${name} (copy)`.slice(0, 128)
}
