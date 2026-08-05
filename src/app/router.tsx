import { useEffect } from "react"

import { AppShell } from "@/app/shell/AppShell"
import { useNavigationStore } from "@/stores/navigation-store"

function routeFromHash(hash: string) {
  return hash === "#/settings" ? "settings" : "home"
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
