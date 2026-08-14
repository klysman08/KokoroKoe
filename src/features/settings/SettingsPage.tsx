import { useState, type FormEvent } from "react"
import { useQuery } from "@tanstack/react-query"
import { Cpu, Database, Download, HardDrive, ShieldCheck } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { type AppSettings, type AppSettingsUpdate } from "@/contracts/settings"
import { ApplicationError } from "@/contracts/app-error"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { AudioDeviceSettings } from "@/features/settings/AudioDeviceSettings"
import { OpenRouterCredentialCard } from "@/features/settings/OpenRouterCredentialCard"
import { openRouterModelQueryKey } from "@/features/settings/openrouter-query"
import { type OpenRouterModel } from "@/contracts/openrouter"
import { listOpenRouterModels } from "@/lib/tauri/openrouter"
import {
  useChooseWorkspaceMutation,
  useUpdateSettingsMutation,
} from "@/features/settings/use-settings-mutations"
import { useSettingsQuery } from "@/features/settings/use-settings-query"
import {
  useCancelModelMutation,
  useDeleteModelMutation,
  useDownloadModelMutation,
  useModelProgressEvents,
  useModelsQuery,
  useResumeModelMutation,
  useSetDefaultModelMutation,
} from "@/features/settings/use-model-management"

export function SettingsPage() {
  const settingsQuery = useSettingsQuery()
  const workspaceMutation = useChooseWorkspaceMutation()
  const workspaceCancelled =
    workspaceMutation.error?.details.code === "workspace_selection_cancelled"

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <p className="text-muted-foreground text-sm font-medium">
            Local configuration
          </p>
          <h1 className="text-3xl font-semibold tracking-tight">Settings</h1>
          <p className="text-muted-foreground mt-2 max-w-2xl text-sm leading-6">
            Choose where KokoroKoe keeps local meeting data and configure
            privacy-safe defaults. Secrets are not stored in this settings
            database.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Badge variant="outline">Local SQLite settings</Badge>
          <Badge variant="secondary">No external telemetry</Badge>
        </div>
      </div>

      <Separator />

      <Card>
        <CardHeader>
          <div className="flex items-center gap-3">
            <span className="bg-muted grid size-9 place-items-center rounded-lg">
              <Database aria-hidden="true" className="size-4" />
            </span>
            <div>
              <CardTitle>Workspace onboarding</CardTitle>
              <CardDescription>
                Only an existing, writable local Windows folder without links or
                reparse points can be selected.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <p className="text-sm font-medium">Current workspace</p>
            <p className="text-muted-foreground mt-1 text-sm break-all">
              {settingsQuery.data?.workspacePath ?? "Workspace not loaded"}
            </p>
          </div>
          <Button
            type="button"
            disabled={workspaceMutation.isPending}
            onClick={() => workspaceMutation.mutate()}
          >
            <HardDrive aria-hidden="true" className="size-4" />
            {workspaceMutation.isPending
              ? "Waiting for folder selection…"
              : "Choose workspace folder"}
          </Button>
          {workspaceMutation.isSuccess && (
            <div role="status" aria-live="polite" className="text-sm">
              <p className="font-medium">Workspace is writable and saved.</p>
              <p className="text-muted-foreground mt-1">
                Approximately{" "}
                {formatFreeSpace(workspaceMutation.data.freeBytes)}
                available.
              </p>
              {workspaceMutation.data.warning && (
                <p className="text-amber-700 dark:text-amber-300">
                  {workspaceMutation.data.warning}
                </p>
              )}
            </div>
          )}
          {workspaceCancelled && (
            <p
              role="status"
              aria-live="polite"
              className="text-muted-foreground text-sm"
            >
              Folder selection was cancelled; the current workspace was not
              changed.
            </p>
          )}
          {workspaceMutation.isError && !workspaceCancelled && (
            <SanitizedErrorPanel error={workspaceMutation.error} />
          )}
        </CardContent>
      </Card>

      <AudioDeviceSettings />

      <OpenRouterCredentialCard />

      {settingsQuery.isSuccess && (
        <ModelManagement settings={settingsQuery.data} />
      )}

      {settingsQuery.isPending && (
        <Card aria-live="polite">
          <CardHeader>
            <CardTitle>Loading local settings</CardTitle>
            <CardDescription>
              Reading the versioned non-secret settings record from Rust.
            </CardDescription>
          </CardHeader>
        </Card>
      )}

      {settingsQuery.isError && (
        <SanitizedErrorPanel error={settingsQuery.error} />
      )}

      {settingsQuery.isSuccess && (
        <PreferencesForm settings={settingsQuery.data} />
      )}

      <Card>
        <CardHeader>
          <div className="flex items-center gap-3">
            <ShieldCheck aria-hidden="true" className="size-5" />
            <CardTitle>Recording and transcription notice</CardTitle>
          </div>
          <CardDescription className="leading-6">
            You are responsible for obtaining any consent required by local
            recording, privacy, employment, and transcription laws before
            starting a meeting session. A folder synchronized by Windows or a
            third party may copy local meeting data outside this device.
          </CardDescription>
        </CardHeader>
      </Card>
    </div>
  )
}

function ModelManagement({ settings }: { settings: AppSettings | undefined }) {
  const models = useModelsQuery()
  const download = useDownloadModelMutation()
  const resume = useResumeModelMutation()
  const cancel = useCancelModelMutation()
  const remove = useDeleteModelMutation()
  const select = useSetDefaultModelMutation()
  useModelProgressEvents()

  const error =
    models.error ??
    download.error ??
    resume.error ??
    cancel.error ??
    remove.error ??
    select.error

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-3">
          <span className="bg-muted grid size-9 place-items-center rounded-lg">
            <Cpu aria-hidden="true" className="size-4" />
          </span>
          <div>
            <CardTitle>Local transcription models</CardTitle>
            <CardDescription>
              Download and verify a curated Whisper model for local-only
              transcription. Model files never pass through React.
            </CardDescription>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {models.isPending && (
          <p role="status" className="text-muted-foreground text-sm">
            Checking local model installations…
          </p>
        )}
        {models.data?.map((model) => {
          const job = model.downloadJob
          const active =
            job && ["queued", "downloading", "verifying"].includes(job.status)
          const progress = job
            ? Math.round((job.bytesDownloaded / job.totalBytes) * 100)
            : 0
          const busy =
            download.isPending ||
            resume.isPending ||
            cancel.isPending ||
            remove.isPending ||
            select.isPending
          return (
            <section
              key={model.descriptor.id}
              className="border-border space-y-3 rounded-lg border p-4"
              aria-label={model.descriptor.name}
            >
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <div className="flex flex-wrap items-center gap-2">
                    <h3 className="font-medium">{model.descriptor.name}</h3>
                    {model.selectedAsDefault && <Badge>Default</Badge>}
                    <Badge variant="outline">
                      {model.status.replace("_", " ")}
                    </Badge>
                  </div>
                  <p className="text-muted-foreground mt-1 text-sm">
                    {model.descriptor.languages.join(", ")} ·{" "}
                    {formatBytes(model.descriptor.downloadBytes)} download ·
                    about {formatBytes(model.descriptor.approximateMemoryBytes)}{" "}
                    memory · {model.descriptor.performanceClass}
                  </p>
                  <p className="text-muted-foreground mt-1 text-xs">
                    Backends available here:{" "}
                    {model.availableBackends.join(", ").toUpperCase()} · License{" "}
                    {model.descriptor.licenseSpdx}
                  </p>
                </div>
                <Badge
                  variant={
                    model.compatibility.diskCompatible &&
                    model.compatibility.memoryCompatible
                      ? "secondary"
                      : "destructive"
                  }
                >
                  {model.compatibility.diskCompatible &&
                  model.compatibility.memoryCompatible
                    ? "Compatible"
                    : "Check resources"}
                </Badge>
              </div>
              {job && (
                <div className="space-y-1" aria-live="polite">
                  <div
                    className="bg-muted h-2 overflow-hidden rounded-full"
                    role="progressbar"
                    aria-label={`${model.descriptor.name} download`}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={progress}
                  >
                    <div
                      className="bg-primary h-full"
                      style={{ width: `${progress}%` }}
                    />
                  </div>
                  <p className="text-muted-foreground text-xs">
                    {job.status} · {formatBytes(job.bytesDownloaded)} of{" "}
                    {formatBytes(job.totalBytes)} ({progress}%)
                  </p>
                </div>
              )}
              {model.lastError && (
                <SanitizedErrorPanel
                  error={new ApplicationError(model.lastError)}
                />
              )}
              <div className="flex flex-wrap gap-2">
                {model.status !== "installed" && !active && !job?.resumable && (
                  <Button
                    type="button"
                    size="sm"
                    disabled={busy || !model.compatibility.diskCompatible}
                    onClick={() => download.mutate(model.descriptor.id)}
                  >
                    <Download aria-hidden="true" className="size-4" /> Download
                  </Button>
                )}
                {!active && job?.resumable && (
                  <Button
                    type="button"
                    size="sm"
                    disabled={busy}
                    onClick={() => resume.mutate(model.descriptor.id)}
                  >
                    Resume
                  </Button>
                )}
                {active && (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={busy}
                    onClick={() => cancel.mutate(job.requestId)}
                  >
                    Cancel download
                  </Button>
                )}
                {model.status === "installed" &&
                  !model.selectedAsDefault &&
                  settings && (
                    <Button
                      type="button"
                      size="sm"
                      disabled={busy}
                      onClick={() =>
                        select.mutate({
                          modelId: model.descriptor.id,
                          expectedSettingsRevision: settings.revision,
                        })
                      }
                    >
                      Set as default
                    </Button>
                  )}
                {model.status === "installed" && !model.selectedAsDefault && (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={busy}
                    onClick={() => remove.mutate(model.descriptor.id)}
                  >
                    Delete model
                  </Button>
                )}
              </div>
            </section>
          )
        })}
        {error && <SanitizedErrorPanel error={error} />}
      </CardContent>
    </Card>
  )
}

function PreferencesForm({ settings }: { settings: AppSettings }) {
  const mutation = useUpdateSettingsMutation()
  const catalog = useQuery<OpenRouterModel[], ApplicationError>({
    queryKey: openRouterModelQueryKey,
    queryFn: () => listOpenRouterModels(false),
    enabled: false,
  })
  const [retainAudio, setRetainAudio] = useState(settings.retainAudioByDefault)
  const [requireZdr, setRequireZdr] = useState(
    settings.requireZeroDataRetention,
  )
  const [denyCollection, setDenyCollection] = useState(
    settings.denyProviderDataCollection,
  )
  const [maxTokens, setMaxTokens] = useState(
    String(settings.maxTokensPerRequest),
  )
  const [budget, setBudget] = useState(settings.defaultSessionBudgetUsd)
  const [llmModels, setLlmModels] = useState(settings.defaultLlmModels)
  const [retentionConfirmed, setRetentionConfirmed] = useState(false)
  const [privacyRelaxationConfirmed, setPrivacyRelaxationConfirmed] =
    useState(false)

  const parsedTokens = Number(maxTokens)
  const tokensValid =
    Number.isInteger(parsedTokens) &&
    parsedTokens >= 1 &&
    parsedTokens <= 1_000_000
  const budgetValid = budget.length <= 18 && /^\d+(?:\.\d{2})$/.test(budget)
  const retentionNeedsConfirmation =
    retainAudio && !settings.retainAudioByDefault
  const privacyNeedsConfirmation =
    (!requireZdr && settings.requireZeroDataRetention) ||
    (!denyCollection && settings.denyProviderDataCollection)
  const modelSelectionChanged =
    JSON.stringify(llmModels) !== JSON.stringify(settings.defaultLlmModels)
  const patch: AppSettingsUpdate = {
    ...(retainAudio !== settings.retainAudioByDefault && {
      retainAudioByDefault: retainAudio,
    }),
    ...(requireZdr !== settings.requireZeroDataRetention && {
      requireZeroDataRetention: requireZdr,
    }),
    ...(denyCollection !== settings.denyProviderDataCollection && {
      denyProviderDataCollection: denyCollection,
    }),
    ...(tokensValid &&
      parsedTokens !== settings.maxTokensPerRequest && {
        maxTokensPerRequest: parsedTokens,
      }),
    ...(budgetValid &&
      budget !== settings.defaultSessionBudgetUsd && {
        defaultSessionBudgetUsd: budget,
      }),
    ...(modelSelectionChanged && { defaultLlmModels: llmModels }),
  }
  const dirty = Object.keys(patch).length > 0
  const confirmationsValid =
    (!retentionNeedsConfirmation || retentionConfirmed) &&
    (!privacyNeedsConfirmation || privacyRelaxationConfirmed)
  const canSave =
    dirty &&
    tokensValid &&
    budgetValid &&
    confirmationsValid &&
    !mutation.isPending

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!canSave) return
    mutation.mutate({ expectedRevision: settings.revision, value: patch })
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <CardTitle>Privacy and usage defaults</CardTitle>
          <Badge variant="secondary">Revision {settings.revision}</Badge>
        </div>
        <CardDescription>
          These settings remain local. OpenRouter and audio capture are not
          enabled by this screen.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form className="space-y-5" onSubmit={submit}>
          <CheckboxField
            id="retain-audio"
            checked={retainAudio}
            onChange={setRetainAudio}
            label="Retain original session audio by default"
            description="Disabled is the privacy-safe default."
          />
          {retentionNeedsConfirmation && (
            <CheckboxField
              id="confirm-retention"
              checked={retentionConfirmed}
              onChange={setRetentionConfirmed}
              label="I understand retained audio increases sensitive local data"
              description="This confirmation is required before saving."
            />
          )}
          <CheckboxField
            id="require-zdr"
            checked={requireZdr}
            onChange={setRequireZdr}
            label="Require zero-data-retention providers"
            description="Keep enabled unless you explicitly accept provider retention."
          />
          <CheckboxField
            id="deny-collection"
            checked={denyCollection}
            onChange={setDenyCollection}
            label="Deny providers that collect request data"
            description="Only applies after optional OpenRouter features are configured."
          />
          {privacyNeedsConfirmation && (
            <CheckboxField
              id="confirm-privacy-relaxation"
              checked={privacyRelaxationConfirmed}
              onChange={setPrivacyRelaxationConfirmed}
              label="I understand this relaxes the external-provider privacy policy"
              description="This confirmation is required before saving."
            />
          )}
          <FieldGroup>
            <div className="grid gap-4 lg:grid-cols-3">
              <ModelRoleField
                id="insights-model"
                label="Fast insights model"
                description="Frozen into new sessions for future real-time insights."
                models={catalog.data ?? []}
                value={llmModels.insights}
                onChange={(insights) =>
                  setLlmModels((current) => ({ ...current, insights }))
                }
              />
              <ModelRoleField
                id="summaries-model"
                label="Summary model"
                description="Frozen into new sessions for accumulated and final summaries."
                models={catalog.data ?? []}
                value={llmModels.summaries}
                onChange={(summaries) =>
                  setLlmModels((current) => ({ ...current, summaries }))
                }
              />
              <ModelRoleField
                id="questions-model"
                label="Manual questions model"
                description="Frozen into new sessions for future transcript questions."
                models={catalog.data ?? []}
                value={llmModels.manualQuestions}
                onChange={(manualQuestions) =>
                  setLlmModels((current) => ({
                    ...current,
                    manualQuestions,
                  }))
                }
              />
            </div>
            {!catalog.data && (
              <p className="text-muted-foreground text-sm">
                Validate the credential and load the privacy-filtered catalog
                above before choosing role models.
              </p>
            )}
            <div className="grid gap-4 sm:grid-cols-2">
              <Field data-invalid={!tokensValid}>
                <FieldLabel htmlFor="max-tokens">
                  Maximum tokens per request
                </FieldLabel>
                <Input
                  id="max-tokens"
                  inputMode="numeric"
                  value={maxTokens}
                  aria-invalid={!tokensValid}
                  aria-describedby="max-tokens-help"
                  onChange={(event) => setMaxTokens(event.currentTarget.value)}
                />
                <FieldDescription id="max-tokens-help">
                  Enter a whole number from 1 to 1,000,000.
                </FieldDescription>
              </Field>
              <Field data-invalid={!budgetValid}>
                <FieldLabel htmlFor="session-budget">
                  Default session budget (USD)
                </FieldLabel>
                <Input
                  id="session-budget"
                  inputMode="decimal"
                  value={budget}
                  aria-invalid={!budgetValid}
                  aria-describedby="session-budget-help"
                  onChange={(event) => setBudget(event.currentTarget.value)}
                />
                <FieldDescription id="session-budget-help">
                  Use a fixed amount such as 0.00 or 5.00.
                </FieldDescription>
              </Field>
            </div>
          </FieldGroup>
          <div className="flex flex-wrap items-center gap-3">
            <Button type="submit" disabled={!canSave}>
              {mutation.isPending ? "Saving…" : "Save settings"}
            </Button>
            {mutation.isSuccess && (
              <p role="status" aria-live="polite" className="text-sm">
                Settings saved at revision {mutation.data.revision}.
              </p>
            )}
          </div>
          {mutation.isError && <SanitizedErrorPanel error={mutation.error} />}
        </form>
      </CardContent>
    </Card>
  )
}

const noModelValue = "__none__"

function ModelRoleField({
  id,
  label,
  description,
  models,
  value,
  onChange,
}: {
  id: string
  label: string
  description: string
  models: OpenRouterModel[]
  value: string | undefined
  onChange: (value: string | undefined) => void
}) {
  const currentMissing =
    value !== undefined && !models.some((model) => model.id === value)
  return (
    <Field>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Select
        value={value ?? noModelValue}
        onValueChange={(next) =>
          onChange(next === noModelValue || next === null ? undefined : next)
        }
        disabled={models.length === 0}
      >
        <SelectTrigger id={id} className="w-full">
          <SelectValue placeholder="Choose a model" />
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          <SelectGroup>
            <SelectItem value={noModelValue}>Not selected</SelectItem>
            {currentMissing && value && (
              <SelectItem value={value} disabled>
                Unavailable · {value}
              </SelectItem>
            )}
            {models.map((model) => (
              <SelectItem key={model.id} value={model.id}>
                {model.name} · {model.provider}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <FieldDescription>{description}</FieldDescription>
    </Field>
  )
}

function CheckboxField({
  id,
  checked,
  onChange,
  label,
  description,
}: {
  id: string
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
  description: string
}) {
  return (
    <div className="flex items-start gap-3">
      <input
        id={id}
        type="checkbox"
        className="mt-1 size-4"
        checked={checked}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
      <label htmlFor={id} className="text-sm">
        <span className="font-medium">{label}</span>
        <span className="text-muted-foreground mt-1 block">{description}</span>
      </label>
    </div>
  )
}

function formatFreeSpace(bytes: number) {
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB`
}

function formatBytes(bytes: number) {
  return bytes >= 1024 ** 3
    ? `${(bytes / 1024 ** 3).toFixed(1)} GiB`
    : `${(bytes / 1024 ** 2).toFixed(0)} MiB`
}
