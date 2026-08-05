import {
  AudioLines,
  ChevronLeft,
  ChevronRight,
  Home,
  Settings,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { HomePage } from "@/features/home/HomePage"
import { SettingsPage } from "@/features/settings/SettingsPage"
import { cn } from "@/lib/utils"
import {
  type AppRoute,
  routeHref,
  useNavigationStore,
} from "@/stores/navigation-store"
import { useShellStore } from "@/stores/shell-store"

const navigation = [
  { label: "Home", route: "home", icon: Home },
  { label: "Settings", route: "settings", icon: Settings },
] as const

export function AppShell() {
  const activeRoute = useNavigationStore((state) => state.activeRoute)
  const navigate = useNavigationStore((state) => state.navigate)
  const collapsed = useShellStore((state) => state.navigationCollapsed)
  const toggleNavigation = useShellStore((state) => state.toggleNavigation)

  const navigateFromLink = (event: React.MouseEvent, route: AppRoute) => {
    event.preventDefault()
    navigate(route)
  }

  return (
    <div className="bg-background text-foreground flex min-h-svh">
      <aside
        aria-label="Primary navigation"
        className={cn(
          "bg-sidebar text-sidebar-foreground sticky top-0 flex h-svh shrink-0 flex-col border-r px-3 py-4 transition-[width] duration-200",
          collapsed ? "w-18" : "w-64",
        )}
      >
        <div className="flex h-11 items-center gap-3 px-2">
          <span className="bg-primary text-primary-foreground grid size-9 shrink-0 place-items-center rounded-xl shadow-sm">
            <AudioLines aria-hidden="true" className="size-5" />
          </span>
          {!collapsed && (
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold">KokoroKoe</p>
              <p className="text-muted-foreground truncate text-xs">
                Local meeting assistant
              </p>
            </div>
          )}
        </div>

        <nav className="mt-8 grid gap-1">
          {navigation.map(({ label, route, icon: Icon }) => (
            <a
              key={route}
              aria-label={collapsed ? label : undefined}
              aria-current={activeRoute === route ? "page" : undefined}
              className={cn(
                "flex h-10 items-center gap-3 rounded-lg px-3 text-sm font-medium transition-colors",
                activeRoute === route
                  ? "bg-sidebar-accent text-sidebar-accent-foreground"
                  : "text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground",
                collapsed && "justify-center px-0",
              )}
              href={routeHref[route]}
              onClick={(event) => navigateFromLink(event, route)}
            >
              <Icon aria-hidden="true" className="size-4 shrink-0" />
              {!collapsed && <span>{label}</span>}
            </a>
          ))}
        </nav>

        <div className="mt-auto">
          {!collapsed && (
            <div className="bg-card text-muted-foreground mb-3 rounded-xl border p-3 text-xs leading-relaxed">
              Audio stays local. External analysis remains off until explicitly
              enabled.
            </div>
          )}
          <Button
            aria-label={collapsed ? "Expand navigation" : "Collapse navigation"}
            className="w-full"
            onClick={toggleNavigation}
            size={collapsed ? "icon" : "default"}
            variant="ghost"
          >
            {collapsed ? <ChevronRight /> : <ChevronLeft />}
            {!collapsed && <span>Collapse</span>}
          </Button>
        </div>
      </aside>

      <div className="min-w-0 flex-1">
        <header className="bg-background/90 flex h-16 items-center justify-between border-b px-6 backdrop-blur">
          <div>
            <p className="text-sm font-medium">Windows MVP</p>
            <p className="text-muted-foreground text-xs">
              Foundation checkpoint
            </p>
          </div>
          <div className="text-muted-foreground flex items-center gap-2 text-xs">
            <span
              className="size-2 rounded-full bg-emerald-500"
              aria-hidden="true"
            />
            Local mode
          </div>
        </header>
        <main className="mx-auto w-full max-w-7xl p-6 lg:p-8">
          {activeRoute === "settings" ? <SettingsPage /> : <HomePage />}
        </main>
      </div>
    </div>
  )
}
