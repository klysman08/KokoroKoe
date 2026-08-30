import { Trash2 } from "lucide-react"
import { useState } from "react"

import { Button } from "@/components/ui/button"

/**
 * A two-step removal.
 *
 * Deleting is reversible — Rust moves the folder to the workspace Trash rather
 * than destroying it — so this asks once rather than demanding the name be
 * typed. The confirmation says where the files go, because "delete" that keeps
 * everything is only reassuring if the user is told so.
 */
export function DeleteConfirm({
  busy,
  label,
  onDelete,
  what,
}: {
  busy: boolean
  /** Accessible name, e.g. "Delete project Planning". */
  label: string
  onDelete: () => void
  /** "project" or "session", used in the confirmation sentence. */
  what: string
}) {
  const [confirming, setConfirming] = useState(false)

  if (!confirming) {
    return (
      <Button
        aria-label={label}
        disabled={busy}
        onClick={() => setConfirming(true)}
        size="sm"
        type="button"
        variant="ghost"
      >
        <Trash2 aria-hidden="true" data-icon="inline-start" /> Delete
      </Button>
    )
  }

  return (
    <span className="flex flex-wrap items-center gap-2">
      <span className="text-muted-foreground text-xs">
        Move this {what} to the workspace Trash folder? Nothing is erased — the
        files stay on disk until you remove them yourself.
      </span>
      <Button
        disabled={busy}
        onClick={onDelete}
        size="sm"
        type="button"
        variant="destructive"
      >
        {busy ? "Moving…" : "Move to Trash"}
      </Button>
      <Button
        disabled={busy}
        onClick={() => setConfirming(false)}
        size="sm"
        type="button"
        variant="ghost"
      >
        Cancel
      </Button>
    </span>
  )
}
