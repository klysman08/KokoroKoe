import { useEffect } from "react"

import { useShellStore } from "@/stores/shell-store"

export const THEME_STORAGE_KEY = "kokorokoe-theme"

/**
 * Paints this window in the chosen theme, and keeps it there.
 *
 * Every surface needs this, not only the main shell: a detached window renders
 * the same document under a different route, so without it a popped-out
 * transcript stays light while the rest of the application is dark.
 *
 * The windows share one origin, so the theme travels between them as a
 * `storage` event — the browser's own cross-document channel. That keeps a
 * pure view preference out of Rust, which owns the appearance a detached
 * window cannot decide for itself (opacity, compact mode, geometry) rather
 * than the one the whole application already agrees on.
 */
export function useAppliedTheme() {
  const theme = useShellStore((state) => state.theme)
  const adoptTheme = useShellStore((state) => state.adoptTheme)

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark")
    document.documentElement.style.colorScheme = theme
  }, [theme])

  useEffect(() => {
    const receive = (event: StorageEvent) => {
      if (event.key !== THEME_STORAGE_KEY) return
      if (event.newValue === "light" || event.newValue === "dark")
        adoptTheme(event.newValue)
    }
    globalThis.addEventListener?.("storage", receive)
    return () => globalThis.removeEventListener?.("storage", receive)
  }, [adoptTheme])
}
