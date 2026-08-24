import { useEffect, useState } from "react"
import { PanelRightOpen, PanelRightClose, Pin, Rows3 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import {
  MAXIMUM_BACKGROUND_OPACITY,
  MINIMUM_BACKGROUND_OPACITY,
  type DetachedWindow,
  type DetachedWindowAppearance,
  type DetachedWindowInteraction,
  type DetachedWindowShortcutStatus,
} from "@/contracts/windows"
import { cn } from "@/lib/utils"
import {
  closeDetachedWindow,
  getDetachedWindowAppearance,
  getDetachedWindowInteraction,
  getDetachedWindowShortcut,
  openDetachedWindow,
  setDetachedWindowAppearance,
  setDetachedWindowInteraction,
  setDetachedWindowShortcut,
} from "@/lib/tauri/windows"

const OPACITY_STEP = 0.05

const WINDOW_LABELS: Record<DetachedWindow, string> = {
  transcript: "Transcript window",
  insights: "Insights window",
}

/**
 * Controls for one detached window.
 *
 * These live in the main window on purpose: a detached window holds no command
 * permission, so its state is changed here and Rust pushes the result to it.
 * The same component serves every window because the state is keyed by window,
 * which is also what keeps the two from sharing a setting by accident.
 */
export function DetachedWindowControls({
  collapsed,
  window,
}: {
  collapsed: boolean
  window: DetachedWindow
}) {
  const [appearance, setAppearance] = useState<DetachedWindowAppearance>()
  const [shortcut, setShortcut] = useState<DetachedWindowShortcutStatus>()
  const [interaction, setInteraction] = useState<DetachedWindowInteraction>()
  const [binding, setBinding] = useState("")
  const [error, setError] = useState<ApplicationError>()
  const [pending, setPending] = useState(false)

  useEffect(() => {
    if (collapsed) return
    let disposed = false
    void getDetachedWindowAppearance(window)
      .then((current) => {
        if (!disposed) setAppearance(current)
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
    void getDetachedWindowShortcut(window)
      .then((current) => {
        if (disposed) return
        setShortcut(current)
        setBinding(current.binding)
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
    void getDetachedWindowInteraction(window)
      .then((current) => {
        if (!disposed) setInteraction(current)
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
    return () => {
      disposed = true
    }
  }, [collapsed, window])

  async function run(operation: () => Promise<void>) {
    setError(undefined)
    setPending(true)
    try {
      await operation()
    } catch (caught: unknown) {
      setError(toApplicationError(caught))
    } finally {
      setPending(false)
    }
  }

  async function update(changes: Partial<DetachedWindowAppearance>) {
    if (!appearance) return
    const next = await setDetachedWindowAppearance({
      window,
      backgroundOpacity:
        changes.backgroundOpacity ?? appearance.backgroundOpacity,
      alwaysOnTop: changes.alwaysOnTop ?? appearance.alwaysOnTop,
      compact: changes.compact ?? appearance.compact,
    })
    setAppearance(next)
  }

  async function updateShortcut(changes: {
    binding?: string
    enabled?: boolean
  }) {
    if (!shortcut) return
    const next = await setDetachedWindowShortcut({
      window,
      binding: (changes.binding ?? shortcut.binding).trim(),
      enabled: changes.enabled ?? shortcut.enabled,
    })
    setShortcut(next)
    setBinding(next.binding)
  }

  async function updateInteraction(changes: { clickThrough: boolean }) {
    setInteraction(await setDetachedWindowInteraction({ window, ...changes }))
  }

  if (collapsed) return null

  const title = WINDOW_LABELS[window]
  const opacityPercent = Math.round(
    (appearance?.backgroundOpacity ?? MAXIMUM_BACKGROUND_OPACITY) * 100,
  )
  const opacityId = `${window}-window-opacity`
  const shortcutId = `${window}-window-shortcut`

  return (
    <section aria-label={title} className="mb-3 rounded-xl border p-3">
      <p className="text-xs font-medium">{title}</p>
      <div className="mt-2 flex gap-2">
        <Button
          className="flex-1"
          disabled={pending}
          onClick={() => void run(() => openDetachedWindow(window))}
          size="sm"
          variant="outline"
        >
          <PanelRightOpen data-icon="inline-start" /> Pop out
        </Button>
        <Button
          aria-label={`Close ${title.toLowerCase()}`}
          disabled={pending}
          onClick={() => void run(() => closeDetachedWindow(window))}
          size="icon"
          variant="ghost"
        >
          <PanelRightClose />
        </Button>
      </div>

      <div className="mt-3 flex gap-2">
        <Button
          aria-pressed={appearance?.alwaysOnTop ?? false}
          className={cn("flex-1", appearance?.alwaysOnTop && "border-primary")}
          disabled={pending || !appearance}
          onClick={() =>
            void run(() => update({ alwaysOnTop: !appearance?.alwaysOnTop }))
          }
          size="sm"
          variant="outline"
        >
          <Pin data-icon="inline-start" /> On top
        </Button>
        <Button
          aria-pressed={appearance?.compact ?? false}
          className={cn("flex-1", appearance?.compact && "border-primary")}
          disabled={pending || !appearance}
          onClick={() =>
            void run(() => update({ compact: !appearance?.compact }))
          }
          size="sm"
          variant="outline"
        >
          <Rows3 data-icon="inline-start" /> Compact
        </Button>
      </div>

      <label className="mt-3 block text-xs" htmlFor={opacityId}>
        Background opacity
        <span className="text-muted-foreground ml-1">{opacityPercent}%</span>
      </label>
      <input
        className="mt-1 w-full"
        disabled={pending || !appearance}
        id={opacityId}
        max={MAXIMUM_BACKGROUND_OPACITY}
        min={MINIMUM_BACKGROUND_OPACITY}
        onChange={(event) =>
          void run(() =>
            update({ backgroundOpacity: Number(event.target.value) }),
          )
        }
        step={OPACITY_STEP}
        type="range"
        value={appearance?.backgroundOpacity ?? MAXIMUM_BACKGROUND_OPACITY}
      />
      <p className="text-muted-foreground mt-1 text-xs">
        Dims the window background only. Text stays fully opaque.
      </p>

      <label className="mt-3 block text-xs" htmlFor={shortcutId}>
        Show/hide shortcut
      </label>
      <div className="mt-1 flex gap-2">
        <Input
          className="h-8 text-xs"
          disabled={pending || !shortcut}
          id={shortcutId}
          onChange={(event) => setBinding(event.target.value)}
          placeholder="Ctrl+Shift+T"
          value={binding}
        />
        <Button
          disabled={pending || !shortcut || binding.trim().length === 0}
          onClick={() => void run(() => updateShortcut({ binding }))}
          size="sm"
          variant="outline"
        >
          Apply
        </Button>
      </div>
      <label className="mt-2 flex items-start gap-2 text-xs">
        <input
          checked={shortcut?.enabled ?? false}
          disabled={pending || !shortcut}
          onChange={(event) =>
            void run(() => updateShortcut({ enabled: event.target.checked }))
          }
          type="checkbox"
        />
        <span>Use this shortcut system-wide</span>
      </label>
      {shortcut?.enabled && !shortcut.registered && (
        <p className="text-xs text-amber-700" role="status">
          Windows did not accept {shortcut.binding}. Another application — or
          this app's other window — is probably using it; try a different
          combination.
        </p>
      )}
      <p className="text-muted-foreground mt-1 text-xs">
        Needs Ctrl, Alt, or Win, and each window needs its own combination. The
        same shortcut hides and shows the window, and <strong>Pop out</strong>{" "}
        always brings it back.
      </p>

      <label className="mt-3 flex items-start gap-2 text-xs">
        <input
          checked={interaction?.clickThrough ?? false}
          disabled={pending || !interaction}
          onChange={(event) =>
            void run(() =>
              updateInteraction({ clickThrough: event.target.checked }),
            )
          }
          type="checkbox"
        />
        <span>Let clicks pass through this window</span>
      </label>
      <p className="text-muted-foreground mt-1 text-xs">
        The window stops accepting the mouse entirely — you cannot move, resize,
        or close it while this is on. Turn it off here; it also switches off if
        this window closes, and never survives a restart.
      </p>

      {error && (
        <p className="text-destructive mt-2 text-xs" role="alert">
          {error.details.userMessage}
        </p>
      )}
    </section>
  )
}
