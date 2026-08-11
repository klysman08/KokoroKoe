import { useEffect } from "react"

import { AppShell } from "@/app/shell/AppShell"
import { useNavigationStore } from "@/stores/navigation-store"

function routeFromHash(hash: string) {
  if (hash === "#/settings") return "settings"
  if (hash === "#/transcript") return "transcript"
  return "home"
}

export function AppRouter() {
  const setRoute = useNavigationStore((state) => state.setRoute)

  useEffect(() => {
    const synchronizeRoute = () => {
      setRoute(routeFromHash(window.location.hash))
    }

    synchronizeRoute()
    window.addEventListener("hashchange", synchronizeRoute)

    return () => window.removeEventListener("hashchange", synchronizeRoute)
  }, [setRoute])

  return <AppShell />
}
