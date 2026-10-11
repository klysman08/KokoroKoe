import { CircleAlert, CircleCheck, Settings } from "lucide-react"

import { Button } from "@/components/ui/button"
import { useNavigationStore } from "@/stores/navigation-store"

export type SetupItem = {
  label: string
  done: boolean
  /** What is missing and what it costs the user, in one line. */
  hint: string
}

/**
 * What still stands between the user and a useful meeting.
 *
 * It appears only while something is missing, so a configured app opens
 * straight onto the Start button. Transcription is required; OpenRouter is
 * optional and says what it adds rather than presenting itself as an error.
 */
export function SetupChecklist({ items }: { items: SetupItem[] }) {
  const navigate = useNavigationStore((state) => state.navigate)
  if (items.every((item) => item.done)) return null
  return (
    <section
      aria-label="Setup"
      className="rounded-2xl border border-dashed p-5"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold">Finish setting up</h2>
          <p className="text-muted-foreground text-xs">
            A few things are still missing. Each takes a minute in Settings.
          </p>
        </div>
        <Button
          onClick={() => navigate("settings")}
          size="sm"
          variant="outline"
        >
          <Settings data-icon="inline-start" /> Open Settings
        </Button>
      </div>
      <ul className="mt-4 grid gap-2">
        {items.map((item) => (
          <li className="flex items-start gap-2 text-sm" key={item.label}>
            {item.done ? (
              <CircleCheck
                aria-label="Done"
                className="mt-0.5 size-4 shrink-0 text-emerald-600"
              />
            ) : (
              <CircleAlert
                aria-label="Missing"
                className="mt-0.5 size-4 shrink-0 text-amber-600"
              />
            )}
            <span>
              <span className="font-medium">{item.label}</span>
              {!item.done && (
                <span className="text-muted-foreground"> — {item.hint}</span>
              )}
            </span>
          </li>
        ))}
      </ul>
    </section>
  )
}
