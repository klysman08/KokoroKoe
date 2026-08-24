import { useEffect, useState } from "react"

import { AppShell } from "@/app/shell/AppShell"
import { DetachedInsightsWindow } from "@/features/insights/DetachedInsightsWindow"
import { DetachedTranscriptWindow } from "@/features/transcript/DetachedTranscriptWindow"
import { useNavigationStore } from "@/stores/navigation-store"

/**
 * The hashes the Rust-owned secondary windows are opened with. Each selects a
 * standalone surface instead of the main application shell.
 */
const DETACHED_TRANSCRIPT_HASH = "#/transcript-window"
const DETACHED_INSIGHTS_HASH = "#/insights-window"

function detachedSurfaceFromHash(hash: string) {
  if (hash === DETACHED_TRANSCRIPT_HASH) return "transcript-window"
  if (hash === DETACHED_INSIGHTS_HASH) return "insights-window"
  return undefined
}

function routeFromHash(hash: string) {
  if (hash === "#/settings") return "settings"
  if (hash === "#/transcript") return "transcript"
  return "home"
}

export function AppRouter() {
  const setRoute = useNavigationStore((state) => state.setRoute)
  const [detached, setDetached] = useState(() =>
    detachedSurfaceFromHash(window.location.hash),
  )

  useEffect(() => {
    const synchronizeRoute = () => {
      const surface = detachedSurfaceFromHash(window.location.hash)
      setDetached(surface)
      if (!surface) setRoute(routeFromHash(window.location.hash))
    }

    synchronizeRoute()
    window.addEventListener("hashchange", synchronizeRoute)

    return () => window.removeEventListener("hashchange", synchronizeRoute)
  }, [setRoute])

  if (detached === "transcript-window") return <DetachedTranscriptWindow />
  if (detached === "insights-window") return <DetachedInsightsWindow />
  return <AppShell />
}
