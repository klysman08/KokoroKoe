import { useEffect, useState } from "react"

import { AppShell } from "@/app/shell/AppShell"
import { DetachedTranscriptWindow } from "@/features/transcript/DetachedTranscriptWindow"
import { useNavigationStore } from "@/stores/navigation-store"

/**
 * The hash the Rust-owned transcript window is opened with. It selects a
 * standalone surface instead of the main application shell.
 */
const DETACHED_TRANSCRIPT_HASH = "#/transcript-window"

function routeFromHash(hash: string) {
  if (hash === "#/settings") return "settings"
  if (hash === "#/transcript") return "transcript"
  return "home"
}

export function AppRouter() {
  const setRoute = useNavigationStore((state) => state.setRoute)
  const [detached, setDetached] = useState(
    () => window.location.hash === DETACHED_TRANSCRIPT_HASH,
  )

  useEffect(() => {
    const synchronizeRoute = () => {
      const isDetached = window.location.hash === DETACHED_TRANSCRIPT_HASH
      setDetached(isDetached)
      if (!isDetached) setRoute(routeFromHash(window.location.hash))
    }

    synchronizeRoute()
    window.addEventListener("hashchange", synchronizeRoute)

    return () => window.removeEventListener("hashchange", synchronizeRoute)
  }, [setRoute])

  return detached ? <DetachedTranscriptWindow /> : <AppShell />
}
