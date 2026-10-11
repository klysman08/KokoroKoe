import { useMutation } from "@tanstack/react-query"
import {
  AudioLines,
  ChartColumn,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  FolderKanban,
  Home,
  Mic,
  Moon,
  Pause,
  Settings,
  Square,
  Sun,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import type { ApplicationError } from "@/contracts/app-error"
import { DashboardPage } from "@/features/dashboard/DashboardPage"
import { HomePage } from "@/features/home/HomePage"
import { SettingsPage } from "@/features/settings/SettingsPage"
import { MeetingPage } from "@/features/meeting/MeetingPage"
import { ProjectsPage } from "@/features/projects/ProjectsPage"
import { useSessionLifecycleMutation } from "@/features/sessions/use-sessions"
import { useAppliedTheme } from "@/app/shell/use-applied-theme"
import {
  EXTERNAL_PAGE_URLS,
  type ExternalPage,
  openExternalPage,
} from "@/lib/tauri/about"
import { cn } from "@/lib/utils"
import { DetachedWindowControls } from "@/features/windows/DetachedWindowControls"
import {
  type ActiveSession,
  useActiveSessionStore,
} from "@/stores/active-session-store"
import {
  type AppRoute,
  routeHref,
  useNavigationStore,
} from "@/stores/navigation-store"
import { useMeetingStore } from "@/stores/meeting-store"
import { useShellStore } from "@/stores/shell-store"

const navigation = [
  {
    label: "Home",
    route: "home",
    icon: Home,
    description: "Start a meeting or pick up a recent one",
  },
  {
    label: "Meeting",
    route: "meeting",
    icon: Mic,
    description: "Live transcript, insights, and summary",
  },
  {
    label: "Projects",
    route: "projects",
    icon: FolderKanban,
    description: "Every project and its sessions",
  },
  {
    label: "Dashboard",
    route: "dashboard",
    icon: ChartColumn,
    description: "Local usage, cost, and disk space",
  },
  {
    label: "Settings",
    route: "settings",
    icon: Settings,
    description: "Workspace, models, devices, and OpenRouter",
  },
] as const

export function AppShell() {
  const activeRoute = useNavigationStore((state) => state.activeRoute)
  const navigate = useNavigationStore((state) => state.navigate)
  const collapsed = useShellStore((state) => state.navigationCollapsed)
  const toggleNavigation = useShellStore((state) => state.toggleNavigation)
  const theme = useShellStore((state) => state.theme)
  const toggleTheme = useShellStore((state) => state.toggleTheme)
  const activeSession = useActiveSessionStore((state) => state.activeSession)
  const activePage =
    navigation.find(({ route }) => route === activeRoute) ?? navigation[0]

  useAppliedTheme()

  const navigateFromLink = (event: React.MouseEvent, route: AppRoute) => {
    event.preventDefault()
    navigate(route)
  }

  return (
    <div className="bg-background text-foreground flex min-h-svh">
      <aside
        aria-label="Primary navigation"
        className={cn(
          // `overflow-hidden` plus a scrolling middle section is what keeps the
          // sidebar inside the viewport: it holds two window-control panels and
          // an active-session panel, which together are taller than any screen.
          "bg-sidebar text-sidebar-foreground sticky top-0 flex h-svh shrink-0 flex-col overflow-hidden border-r px-3 py-4 transition-[width] duration-200",
          collapsed ? "w-18" : "w-64",
        )}
      >
        <div className="flex h-11 shrink-0 items-center gap-3 px-2">
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

        <div className="-mx-1 flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto px-1">
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

          <div className="mt-6">
            {activeSession && (
              <ActiveSessionControls
                collapsed={collapsed}
                session={activeSession}
              />
            )}
            <DetachedWindowControls collapsed={collapsed} window="transcript" />
            <DetachedWindowControls collapsed={collapsed} window="insights" />
            {!collapsed && (
              <div className="bg-card text-muted-foreground mb-3 rounded-xl border p-3 text-xs leading-relaxed">
                Audio stays local. External analysis remains off until
                explicitly enabled.
              </div>
            )}
          </div>
        </div>

        {/* Outside the scroll area: collapsing must always be reachable, even
            while a Session and both window panels fill the sidebar. */}
        <ProjectCredits collapsed={collapsed} />
        <Button
          aria-label={collapsed ? "Expand navigation" : "Collapse navigation"}
          className="mt-2 w-full shrink-0"
          onClick={toggleNavigation}
          size={collapsed ? "icon" : "default"}
          variant="ghost"
        >
          {collapsed ? <ChevronRight /> : <ChevronLeft />}
          {!collapsed && <span>Collapse</span>}
        </Button>
      </aside>

      <div className="min-w-0 flex-1">
        <header className="bg-background/90 flex h-16 items-center justify-between border-b px-6 backdrop-blur">
          <div>
            <p className="text-sm font-medium">{activePage.label}</p>
            <p className="text-muted-foreground text-xs">
              {activePage.description}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Button
              aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
              onClick={toggleTheme}
              size="icon"
              variant="ghost"
            >
              {theme === "dark" ? <Sun /> : <Moon />}
            </Button>
            <div className="text-muted-foreground flex items-center gap-2 text-xs">
              <BadgeDot />
              Local mode
            </div>
          </div>
        </header>
        <main className="mx-auto w-full max-w-7xl p-6 lg:p-8">
          {activeRoute === "settings" ? (
            <SettingsPage />
          ) : activeRoute === "meeting" ? (
            <MeetingPage />
          ) : activeRoute === "projects" ? (
            <ProjectsPage />
          ) : activeRoute === "dashboard" ? (
            <DashboardPage />
          ) : (
            <HomePage />
          )}
        </main>
      </div>
    </div>
  )
}

function ActiveSessionControls({
  collapsed,
  session,
}: {
  collapsed: boolean
  session: ActiveSession
}) {
  const lifecycle = useSessionLifecycleMutation(session.projectId)
  const navigate = useNavigationStore((state) => state.navigate)
  const openMeeting = useMeetingStore((state) => state.open)
  const boundary = {
    projectId: session.projectId,
    sessionId: session.id,
    expectedRevision: session.revision,
  }

  if (collapsed) {
    return (
      <div className="mb-3 flex justify-center" title={session.title}>
        <span className="bg-primary text-primary-foreground grid size-9 place-items-center rounded-lg">
          <AudioLines aria-hidden="true" />
          <span className="sr-only">Active session: {session.title}</span>
        </span>
      </div>
    )
  }

  return (
    <section aria-label="Active session" className="mb-3 rounded-xl border p-3">
      <div className="flex items-center gap-2 text-xs font-medium">
        <BadgeDot />
        {session.state === "paused" ? "Session paused" : "Now transcribing"}
      </div>
      <p className="mt-2 truncate text-sm font-medium">{session.title}</p>
      <div className="mt-3 flex gap-2">
        <Button
          onClick={() => {
            openMeeting({ projectId: session.projectId, sessionId: session.id })
            navigate("meeting")
          }}
          size="sm"
        >
          Open
        </Button>
        {session.state === "transcribing" && (
          <Button
            disabled={lifecycle.isPending}
            onClick={() =>
              lifecycle.mutate({ kind: "pause", request: boundary })
            }
            size="sm"
            variant="outline"
          >
            <Pause data-icon="inline-start" /> Pause
          </Button>
        )}
        <Button
          disabled={lifecycle.isPending}
          onClick={() => lifecycle.mutate({ kind: "stop", request: boundary })}
          size="sm"
          variant="outline"
        >
          <Square data-icon="inline-start" /> Stop
        </Button>
      </div>
      {lifecycle.error && (
        <p className="text-destructive mt-2 text-xs" role="alert">
          {lifecycle.error.details.userMessage}
        </p>
      )}
    </section>
  )
}

/**
 * The repository link and the author's credit. Each link carries its real
 * address so hovering shows it, but a click asks Rust to open the page in the
 * default browser: the webview itself never navigates away from the app.
 */
function ProjectCredits({ collapsed }: { collapsed: boolean }) {
  const open = useMutation<void, ApplicationError, ExternalPage>({
    mutationFn: openExternalPage,
    retry: false,
  })
  const openPage = (event: React.MouseEvent, page: ExternalPage) => {
    event.preventDefault()
    open.mutate(page)
  }

  if (collapsed) {
    return (
      <a
        aria-label="KokoroKoe on GitHub"
        className="text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground mt-2 grid h-10 shrink-0 place-items-center rounded-lg transition-colors"
        href={EXTERNAL_PAGE_URLS.repository}
        onClick={(event) => openPage(event, "repository")}
        title={`KokoroKoe v${__APP_VERSION__} on GitHub`}
      >
        <GitHubMark className="size-4" />
      </a>
    )
  }

  return (
    <footer className="mt-2 shrink-0 border-t pt-3 text-xs">
      <a
        className="text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground flex h-9 items-center gap-2 rounded-lg px-3 font-medium transition-colors"
        href={EXTERNAL_PAGE_URLS.repository}
        onClick={(event) => openPage(event, "repository")}
      >
        <GitHubMark className="size-4 shrink-0" />
        <span className="truncate">KokoroKoe on GitHub</span>
        <ExternalLink aria-hidden="true" className="ml-auto size-3 shrink-0" />
      </a>
      <p className="text-muted-foreground mt-1 truncate px-3">
        Made by{" "}
        <a
          className="text-foreground font-medium underline-offset-2 hover:underline"
          href={EXTERNAL_PAGE_URLS.author}
          onClick={(event) => openPage(event, "author")}
        >
          klysman08
        </a>{" "}
        · v{__APP_VERSION__}
      </p>
      {open.error && (
        <p className="text-destructive mt-1 px-3" role="alert">
          {open.error.details.userMessage}
        </p>
      )}
    </footer>
  )
}

/** The GitHub mark from Octicons (MIT); Lucide no longer ships brand icons. */
function GitHubMark({ className }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={className}
      fill="currentColor"
      viewBox="0 0 16 16"
    >
      <path d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
    </svg>
  )
}

function BadgeDot() {
  return <span className="bg-primary size-2 rounded-full" aria-hidden="true" />
}
