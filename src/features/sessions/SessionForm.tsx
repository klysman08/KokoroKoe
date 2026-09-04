import { useMemo, useState, type FormEvent } from "react"

import { Button } from "@/components/ui/button"
import {
  createSessionInputSchema,
  updateSessionInputSchema,
  type CreateSessionInput,
  type Project,
  type Session,
  type UpdateSessionInput,
} from "@/contracts/projects"
import { type AudioDevice, type AudioDeviceList } from "@/contracts/audio"
import { presetSnapshotOf, type Preset } from "@/contracts/presets"
import { type AppSettings } from "@/contracts/settings"
import { INSIGHT_LABELS } from "@/features/insights/insight-labels"

type SessionFormProps = {
  project: Project
  session?: Session | undefined
  devices: AudioDeviceList
  presets: Preset[]
  settings: AppSettings | undefined
  busy: boolean
  onCancel: () => void
  onCreate: (input: CreateSessionInput) => void
  onUpdate: (input: UpdateSessionInput) => void
}

const fieldClass =
  "border-input bg-background focus-visible:ring-ring w-full rounded-md border px-3 py-2 text-sm outline-none focus-visible:ring-2"

function snapshot(device: AudioDevice) {
  return {
    endpointId: device.endpointId,
    friendlyName: device.friendlyName,
    selection: { kind: "fixed" as const, endpointId: device.endpointId },
    nativeSampleRate: device.sampleRate,
    nativeChannels: device.channels,
  }
}

type PresetChoice = {
  id: string
  name: string
  description: string
  insightTypes: Preset["insightTypes"]
  snapshot: Session["preset"]
}

// What the picker may offer: the library, plus the preset this Session already
// froze when the library no longer lists it. A Session that was created under
// an older preset keeps reading the way it did, so editing its title must not
// quietly rewrite the instructions it was started with.
function presetChoices(
  presets: Preset[],
  session: Session | undefined,
): PresetChoice[] {
  const library = presets.map((preset) => ({
    id: preset.id,
    name: preset.name,
    description: preset.description,
    insightTypes: preset.insightTypes,
    snapshot: presetSnapshotOf(preset),
  }))
  const frozen = session?.preset
  if (!frozen || library.some((choice) => choice.id === frozen.id)) {
    return library
  }
  return [
    {
      id: frozen.id,
      name: frozen.name,
      description: "Kept as this session recorded it.",
      insightTypes: frozen.insightTypes,
      snapshot: frozen,
    },
    ...library,
  ]
}

export function SessionForm({
  project,
  session,
  devices,
  presets,
  settings,
  busy,
  onCancel,
  onCreate,
  onUpdate,
}: SessionFormProps) {
  const [title, setTitle] = useState(session?.title ?? "")
  const [objective, setObjective] = useState(session?.objective ?? "")
  const [sessionContext, setSessionContext] = useState(
    session?.sessionContext ?? "",
  )
  const [language, setLanguage] = useState(session?.language ?? "en-GB")
  const [microphoneId, setMicrophoneId] = useState(
    session?.microphone.endpointId ?? devices.inputs[0]?.endpointId ?? "",
  )
  const [outputId, setOutputId] = useState(
    session?.systemOutput.endpointId ?? devices.outputs[0]?.endpointId ?? "",
  )
  const [retainAudio, setRetainAudio] = useState(session?.retainAudio ?? false)
  const choices = useMemo(
    () => presetChoices(presets, session),
    [presets, session],
  )
  const [presetId, setPresetId] = useState(
    () =>
      [session?.preset.id, project.defaultPresetId].find((candidate) =>
        choices.some((choice) => choice.id === candidate),
      ) ??
      choices[0]?.id ??
      "",
  )
  const [validationMessage, setValidationMessage] = useState("")
  const chosenPreset = choices.find((choice) => choice.id === presetId)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const microphone = devices.inputs.find(
      (device) => device.endpointId === microphoneId,
    )
    const output = devices.outputs.find(
      (device) => device.endpointId === outputId,
    )
    if (!microphone || !output) {
      setValidationMessage("Select an available microphone and output device.")
      return
    }
    if (!chosenPreset) {
      setValidationMessage("Select a preset for this session.")
      return
    }
    const editable = {
      title,
      objective,
      sessionContext,
      preset: chosenPreset.snapshot,
      language,
      microphone: snapshot(microphone),
      systemOutput: snapshot(output),
      transcriptionModelId:
        session?.transcriptionModelId ?? project.defaultTranscriptionModelId,
      llmModels:
        session?.llmModels ??
        mergeLlmModels(project.preferredLlmModels, settings?.defaultLlmModels),
      spendingLimitUsd:
        session?.spendingLimitUsd ?? settings?.defaultSessionBudgetUsd,
      maxTokensPerRequest:
        session?.maxTokensPerRequest ?? settings?.maxTokensPerRequest,
      retainAudio,
    }
    if (session) {
      const parsed = updateSessionInputSchema.safeParse(editable)
      if (!parsed.success) {
        setValidationMessage("Check the session fields and try again.")
        return
      }
      onUpdate(parsed.data)
      return
    }
    const parsed = createSessionInputSchema.safeParse(editable)
    if (!parsed.success) {
      setValidationMessage("Check the session fields and project defaults.")
      return
    }
    onCreate(parsed.data)
  }

  return (
    <form className="grid gap-4 rounded-xl border p-4" onSubmit={submit}>
      <div className="grid gap-4 md:grid-cols-2">
        <label className="grid gap-1.5 text-sm font-medium">
          Session title
          <input
            className={fieldClass}
            disabled={busy}
            maxLength={256}
            onChange={(event) => setTitle(event.target.value)}
            required
            value={title}
          />
        </label>
        <label className="grid gap-1.5 text-sm font-medium">
          Language
          <input
            className={fieldClass}
            disabled={busy}
            maxLength={64}
            onChange={(event) => setLanguage(event.target.value)}
            required
            value={language}
          />
        </label>
      </div>
      <label className="grid gap-1.5 text-sm font-medium">
        Objective
        <textarea
          className={fieldClass}
          disabled={busy}
          maxLength={4096}
          onChange={(event) => setObjective(event.target.value)}
          rows={2}
          value={objective}
        />
      </label>
      <label className="grid gap-1.5 text-sm font-medium">
        Session context
        <textarea
          className={fieldClass}
          disabled={busy}
          maxLength={32768}
          onChange={(event) => setSessionContext(event.target.value)}
          rows={3}
          value={sessionContext}
        />
      </label>
      <div className="grid gap-1.5">
        <label className="grid gap-1.5 text-sm font-medium">
          Preset
          <select
            className={fieldClass}
            disabled={busy}
            onChange={(event) => setPresetId(event.target.value)}
            required
            value={presetId}
          >
            {choices.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.name}
              </option>
            ))}
          </select>
        </label>
        {chosenPreset && (
          <p className="text-muted-foreground text-sm">
            {chosenPreset.description} Insights:{" "}
            {chosenPreset.insightTypes
              .map((type) => INSIGHT_LABELS[type])
              .join(", ")}
            .
          </p>
        )}
      </div>
      <div className="grid gap-4 md:grid-cols-2">
        <label className="grid gap-1.5 text-sm font-medium">
          Microphone
          <select
            className={fieldClass}
            disabled={busy}
            onChange={(event) => setMicrophoneId(event.target.value)}
            required
            value={microphoneId}
          >
            {devices.inputs.map((device) => (
              <option key={device.endpointId} value={device.endpointId}>
                {device.friendlyName}
              </option>
            ))}
          </select>
        </label>
        <label className="grid gap-1.5 text-sm font-medium">
          System output
          <select
            className={fieldClass}
            disabled={busy}
            onChange={(event) => setOutputId(event.target.value)}
            required
            value={outputId}
          >
            {devices.outputs.map((device) => (
              <option key={device.endpointId} value={device.endpointId}>
                {device.friendlyName}
              </option>
            ))}
          </select>
        </label>
      </div>
      <label className="flex items-center gap-2 text-sm">
        <input
          checked={retainAudio}
          disabled={busy}
          onChange={(event) => setRetainAudio(event.target.checked)}
          type="checkbox"
        />
        Retain original audio when capture persistence is added
      </label>
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
        <Button
          disabled={
            busy || devices.inputs.length === 0 || devices.outputs.length === 0
          }
          type="submit"
        >
          {busy ? "Savingâ€¦" : "Save session"}
        </Button>
      </div>
    </form>
  )
}

function mergeLlmModels(
  project: Project["preferredLlmModels"],
  defaults: AppSettings["defaultLlmModels"] | undefined,
) {
  return {
    insights: project.insights ?? defaults?.insights,
    summaries: project.summaries ?? defaults?.summaries,
    manualQuestions: project.manualQuestions ?? defaults?.manualQuestions,
  }
}
