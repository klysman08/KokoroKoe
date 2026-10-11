import { create } from "zustand"

export type AppRoute =
  "home" | "meeting" | "projects" | "dashboard" | "settings"

export const routeHref: Record<AppRoute, string> = {
  home: "#/",
  meeting: "#/meeting",
  projects: "#/projects",
  dashboard: "#/dashboard",
  settings: "#/settings",
}

type NavigationState = {
  activeRoute: AppRoute
  navigate: (route: AppRoute) => void
  setRoute: (route: AppRoute) => void
}

export const useNavigationStore = create<NavigationState>((set) => ({
  activeRoute: "home",
  navigate: (route) => {
    window.location.hash = routeHref[route]
    set({ activeRoute: route })
  },
  setRoute: (route) => set({ activeRoute: route }),
}))
