import { Database, KeyRound, MonitorCog, RadioTower } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"

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
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <p className="text-muted-foreground text-sm font-medium">
            Configuration
          </p>
          <h1 className="text-3xl font-semibold tracking-tight">Settings</h1>
          <p className="text-muted-foreground mt-2 max-w-2xl text-sm leading-6">
            This route reserves the application structure. Controls remain
            unavailable until their Rust-backed services and validation
            contracts are implemented.
          </p>
        </div>
        <Badge variant="outline">No persisted settings</Badge>
      </div>

      <Separator />

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
