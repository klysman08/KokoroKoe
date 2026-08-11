import { useState, type FormEvent } from "react"

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

type SessionFormProps = {
  project: Project
  session?: Session | undefined
  devices: AudioDeviceList
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

function builtInPreset(project: Project) {
  return {
    id: project.defaultPresetId,
    version: 1,
    name: "Technical interview",
    assistantRole:
      "Identify concrete decisions and unresolved technical risks.",
    analysisObjectives: ["Track decisions", "Capture action items"],
    insightTypes: ["decision", "action_item", "risk"] as const,
    responseTone: "Concise and factual",
    finalSummarySections: ["Decisions", "Actions", "Risks"],
    highlightInstructions: ["Highlight owners and deadlines"],
    prohibitedBehaviors: ["Do not invent facts"],
  }
}

export function SessionForm({
  project,
  session,
  devices,
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
  const [validationMessage, setValidationMessage] = useState("")

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
    const editable = {
      title,
      objective,
      sessionContext,
      preset: session?.preset ?? builtInPreset(project),
      language,
      microphone: snapshot(microphone),
      systemOutput: snapshot(output),
      transcriptionModelId:
        session?.transcriptionModelId ?? project.defaultTranscriptionModelId,
      llmModels: session?.llmModels ?? project.preferredLlmModels,
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
