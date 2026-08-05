import { Database, KeyRound, MonitorCog, RadioTower } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { useSettingsQuery } from "@/features/settings/use-settings-query"

const settingsSections = [
  {
    title: "Workspace",
    description:
      "Local folder selection and validated settings storage are scheduled for a later Phase 2 task.",
    icon: Database,
  },
  {
    title: "Audio devices",
    description:
      "Microphone and system-output enumeration begin after the Windows audio prototype gates.",
    icon: RadioTower,
  },
  {
    title: "OpenRouter",
    description:
      "Credential Manager and text-only LLM access remain disabled and unimplemented.",
    icon: KeyRound,
  },
  {
    title: "Windows",
    description:
      "Independent transcript and insight window controls are planned for Phase 6.",
    icon: MonitorCog,
  },
] as const

export function SettingsPage() {
  const settingsQuery = useSettingsQuery()

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <p className="text-muted-foreground text-sm font-medium">
            Configuration
          </p>
          <h1 className="text-3xl font-semibold tracking-tight">Settings</h1>
          <p className="text-muted-foreground mt-2 max-w-2xl text-sm leading-6">
            Rust provides a validated, privacy-safe default snapshot. Editing
            remains unavailable until the persistent settings service is
            implemented.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Badge variant="outline">No persisted settings</Badge>
          {settingsQuery.isSuccess && (
            <Badge variant="secondary">Read-only defaults</Badge>
          )}
        </div>
      </div>

      <Separator />

      {settingsQuery.isPending && (
        <Card aria-live="polite">
          <CardHeader>
            <CardTitle>Loading local defaults</CardTitle>
            <CardDescription>
              Reading the non-secret foundation settings from Rust.
            </CardDescription>
          </CardHeader>
        </Card>
      )}

      {settingsQuery.isError && (
        <SanitizedErrorPanel error={settingsQuery.error} />
      )}

      {settingsQuery.isSuccess && (
        <Card>
          <CardHeader>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <CardTitle>Foundation defaults</CardTitle>
              <Badge variant="secondary">
                Revision {settingsQuery.data.revision}
              </Badge>
            </div>
            <CardDescription>
              Read-only preview. Persistence and editing are intentionally out
              of scope for this checkpoint.
            </CardDescription>
            <dl className="text-muted-foreground grid gap-3 pt-3 text-sm sm:grid-cols-2">
              <div>
                <dt className="text-foreground font-medium">Workspace</dt>
                <dd className="mt-1">Windows Documents / KokoroKoe</dd>
              </div>
              <div>
                <dt className="text-foreground font-medium">
                  Provisional model identifier
                </dt>
                <dd className="mt-1">
                  {settingsQuery.data.defaultTranscriptionModelId}
                </dd>
              </div>
              <div>
                <dt className="text-foreground font-medium">
                  Zero-data-retention providers
                </dt>
                <dd className="mt-1">
                  {settingsQuery.data.requireZeroDataRetention
                    ? "Required"
                    : "Not required"}
                </dd>
              </div>
              <div>
                <dt className="text-foreground font-medium">
                  Data-collecting providers
                </dt>
                <dd className="mt-1">
                  {settingsQuery.data.denyProviderDataCollection
                    ? "Denied"
                    : "Allowed"}
                </dd>
              </div>
              <div>
                <dt className="text-foreground font-medium">LLM analysis</dt>
                <dd className="mt-1">
                  {settingsQuery.data.llmEnabled ? "Enabled" : "Disabled"}
                </dd>
              </div>
              <div>
                <dt className="text-foreground font-medium">Audio retention</dt>
                <dd className="mt-1">
                  {settingsQuery.data.retainAudioByDefault
                    ? "Enabled"
                    : "Disabled"}
                </dd>
              </div>
            </dl>
          </CardHeader>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-2">
        {settingsSections.map(({ title, description, icon: Icon }) => (
          <Card key={title} className="opacity-80">
            <CardHeader>
              <div className="flex items-center justify-between gap-3">
                <span className="bg-muted grid size-9 place-items-center rounded-lg">
                  <Icon aria-hidden="true" className="size-4" />
                </span>
                <Badge variant="secondary">Planned</Badge>
              </div>
              <CardTitle>{title}</CardTitle>
              <CardDescription>{description}</CardDescription>
            </CardHeader>
          </Card>
        ))}
      </div>
    </div>
  )
}
