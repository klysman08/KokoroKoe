import {
  ArrowRight,
  FolderKanban,
  Mic2,
  ShieldCheck,
  Sparkles,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { useNavigationStore } from "@/stores/navigation-store"

const readiness = [
  {
    title: "Local transcription",
    description: "Audio and Whisper integration begin in Phase 3.",
    icon: Mic2,
  },
  {
    title: "Private by default",
    description:
      "Audio retention and external LLM features will start disabled.",
    icon: ShieldCheck,
  },
  {
    title: "Portable workspace",
    description:
      "Projects and sessions will be materialized as local Markdown.",
    icon: FolderKanban,
  },
] as const

export function HomePage() {
  const navigate = useNavigationStore((state) => state.navigate)

  return (
    <div className="space-y-8">
      <section className="bg-card flex flex-col gap-5 rounded-2xl border p-6 shadow-sm md:flex-row md:items-center md:justify-between md:p-8">
        <div className="max-w-2xl space-y-3">
          <Badge variant="secondary">Foundation ready</Badge>
          <div>
            <h1 className="text-3xl font-semibold tracking-tight">
              Meetings stay understandable—and yours.
            </h1>
            <p className="text-muted-foreground mt-2 text-sm leading-6 md:text-base">
              KokoroKoe is being built as a local-first Windows meeting
              assistant. This checkpoint establishes the secure desktop shell
              before audio or external services are connected.
            </p>
          </div>
        </div>
        <Button onClick={() => navigate("settings")} size="lg">
          Review setup
          <ArrowRight data-icon="inline-end" />
        </Button>
      </section>

      <section aria-labelledby="readiness-title">
        <div className="mb-4 flex items-end justify-between gap-4">
          <div>
            <p className="text-muted-foreground text-sm font-medium">
              MVP foundations
            </p>
            <h2 className="text-xl font-semibold" id="readiness-title">
              Privacy boundaries are visible from day one
            </h2>
          </div>
          <Badge variant="outline">P2-001</Badge>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          {readiness.map(({ title, description, icon: Icon }) => (
            <Card key={title}>
              <CardHeader>
                <span className="bg-muted mb-2 grid size-9 place-items-center rounded-lg">
                  <Icon aria-hidden="true" className="size-4" />
                </span>
                <CardTitle>{title}</CardTitle>
                <CardDescription>{description}</CardDescription>
              </CardHeader>
            </Card>
          ))}
        </div>
      </section>

      <Card>
        <CardHeader className="flex-row items-center justify-between gap-4">
          <div>
            <CardTitle>Recent sessions</CardTitle>
            <CardDescription>
              Sessions will appear here after project persistence is
              implemented.
            </CardDescription>
          </div>
          <Button disabled variant="outline">
            New session
          </Button>
        </CardHeader>
        <CardContent>
          <div className="bg-muted/30 grid min-h-36 place-items-center rounded-xl border border-dashed p-6 text-center">
            <div className="space-y-2">
              <Sparkles
                aria-hidden="true"
                className="text-muted-foreground mx-auto size-5"
              />
              <p className="text-sm font-medium">No session data yet</p>
              <p className="text-muted-foreground max-w-md text-xs leading-5">
                This scaffold deliberately contains no audio capture, transcript
                storage, or network access.
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
