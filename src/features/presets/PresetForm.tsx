import { useState, type FormEvent } from "react"

import { Button } from "@/components/ui/button"
import { insightTypeSchema } from "@/contracts/projects"
import {
  createPresetInputSchema,
  updatePresetInputSchema,
  type CreatePresetInput,
  type Preset,
  type UpdatePresetInput,
} from "@/contracts/presets"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"

const fieldClass =
  "border-input bg-background focus-visible:ring-ring w-full rounded-md border px-3 py-2 text-sm outline-none focus-visible:ring-2"

const INSIGHT_TYPES = insightTypeSchema.options

function lines(value: string) {
  return [
    ...new Set(
      value
        .split("\n")
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  ]
}

export function PresetForm({
  preset,
  busy,
  onCancel,
  onCreate,
  onUpdate,
}: {
  /** The preset being edited, or undefined when writing a new one. */
  preset?: Preset | undefined
  busy: boolean
  onCancel: () => void
  onCreate: (input: CreatePresetInput) => void
  onUpdate: (input: UpdatePresetInput) => void
}) {
  const [name, setName] = useState(preset?.name ?? "")
  const [description, setDescription] = useState(preset?.description ?? "")
  const [assistantRole, setAssistantRole] = useState(
    preset?.assistantRole ?? "",
  )
  const [responseTone, setResponseTone] = useState(preset?.responseTone ?? "")
  const [objectives, setObjectives] = useState(
    preset?.analysisObjectives.join("\n") ?? "",
  )
  const [sections, setSections] = useState(
    preset?.finalSummarySections.join("\n") ?? "",
  )
  const [highlights, setHighlights] = useState(
    preset?.highlightInstructions.join("\n") ?? "",
  )
  const [prohibited, setProhibited] = useState(
    preset?.prohibitedBehaviors.join("\n") ?? "",
  )
  const [insightTypes, setInsightTypes] = useState<string[]>([
    ...(preset?.insightTypes ?? ["decision", "action_item"]),
  ])
  const [validationMessage, setValidationMessage] = useState("")

  const toggleInsightType = (type: string, enabled: boolean) =>
    setInsightTypes((current) =>
      enabled
        ? [...current, type]
        : current.filter((existing) => existing !== type),
    )

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const editable = {
      name,
      description,
      assistantRole,
      analysisObjectives: lines(objectives),
      insightTypes: INSIGHT_TYPES.filter((type) => insightTypes.includes(type)),
      responseTone,
      finalSummarySections: lines(sections),
      highlightInstructions: lines(highlights),
      prohibitedBehaviors: lines(prohibited),
    }
    if (preset) {
      const parsed = updatePresetInputSchema.safeParse(editable)
      if (!parsed.success) {
        setValidationMessage("Check the preset fields and try again.")
        return
      }
      onUpdate(parsed.data)
      return
    }
    const parsed = createPresetInputSchema.safeParse(editable)
    if (!parsed.success) {
      setValidationMessage("Check the preset fields and try again.")
      return
    }
    onCreate(parsed.data)
  }

  return (
    <form className="grid gap-4 rounded-xl border p-4" onSubmit={submit}>
      <div className="grid gap-4 md:grid-cols-2">
        <label className="grid gap-1.5 text-sm font-medium">
          Preset name
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
          Response tone
          <input
            className={fieldClass}
            disabled={busy}
            maxLength={1024}
            onChange={(event) => setResponseTone(event.target.value)}
            required
            value={responseTone}
          />
        </label>
      </div>
      <label className="grid gap-1.5 text-sm font-medium">
        Description
        <input
          className={fieldClass}
          disabled={busy}
          maxLength={1024}
          onChange={(event) => setDescription(event.target.value)}
          value={description}
        />
        <span className="text-muted-foreground text-xs">
          Shown beside the preset when choosing one. Not sent to the model.
        </span>
      </label>
      <label className="grid gap-1.5 text-sm font-medium">
        Assistant role
        <textarea
          className={fieldClass}
          disabled={busy}
          maxLength={4096}
          onChange={(event) => setAssistantRole(event.target.value)}
          required
          rows={3}
          value={assistantRole}
        />
      </label>
      <fieldset className="grid gap-2">
        <legend className="text-sm font-medium">Insight types</legend>
        <div className="grid gap-1.5 sm:grid-cols-2">
          {INSIGHT_TYPES.map((type) => (
            <label className="flex items-center gap-2 text-sm" key={type}>
              <input
                checked={insightTypes.includes(type)}
                disabled={busy}
                onChange={(event) =>
                  toggleInsightType(type, event.target.checked)
                }
                type="checkbox"
              />
              {INSIGHT_LABELS[type]}
            </label>
          ))}
        </div>
        <span className="text-muted-foreground text-xs">
          A Session using this preset asks for exactly these.
        </span>
      </fieldset>
      <div className="grid gap-4 md:grid-cols-2">
        <label className="grid gap-1.5 text-sm font-medium">
          Analysis objectives
          <textarea
            className={fieldClass}
            disabled={busy}
            maxLength={4096}
            onChange={(event) => setObjectives(event.target.value)}
            required
            rows={4}
            value={objectives}
          />
          <span className="text-muted-foreground text-xs">One per line.</span>
        </label>
        <label className="grid gap-1.5 text-sm font-medium">
          Final summary sections
          <textarea
            className={fieldClass}
            disabled={busy}
            maxLength={2048}
            onChange={(event) => setSections(event.target.value)}
            required
            rows={4}
            value={sections}
          />
          <span className="text-muted-foreground text-xs">One per line.</span>
        </label>
        <label className="grid gap-1.5 text-sm font-medium">
          Highlight instructions
          <textarea
            className={fieldClass}
            disabled={busy}
            maxLength={4096}
            onChange={(event) => setHighlights(event.target.value)}
            rows={3}
            value={highlights}
          />
          <span className="text-muted-foreground text-xs">
            One per line. Optional.
          </span>
        </label>
        <label className="grid gap-1.5 text-sm font-medium">
          Prohibited behaviors
          <textarea
            className={fieldClass}
            disabled={busy}
            maxLength={4096}
            onChange={(event) => setProhibited(event.target.value)}
            rows={3}
            value={prohibited}
          />
          <span className="text-muted-foreground text-xs">
            One per line. Optional.
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
          {busy ? "Saving…" : "Save preset"}
        </Button>
      </div>
    </form>
  )
}
