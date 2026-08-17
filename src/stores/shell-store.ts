import { create } from "zustand"

type ShellState = {
  navigationCollapsed: boolean
  theme: "light" | "dark"
  toggleNavigation: () => void
  toggleTheme: () => void
}

const THEME_STORAGE_KEY = "kokorokoe-theme"

function initialTheme(): "light" | "dark" {
  try {
    const stored = globalThis.localStorage?.getItem(THEME_STORAGE_KEY)
    if (stored === "light" || stored === "dark") return stored
  } catch {
    // Appearance persistence is optional when storage is unavailable.
  }
  return globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light"
}

export const useShellStore = create<ShellState>((set) => ({
  navigationCollapsed: false,
  theme: initialTheme(),
  toggleNavigation: () => {
    set((state) => ({ navigationCollapsed: !state.navigationCollapsed }))
  },
  toggleTheme: () => {
    set((state) => {
      const theme = state.theme === "dark" ? "light" : "dark"
      try {
        globalThis.localStorage?.setItem(THEME_STORAGE_KEY, theme)
      } catch {
        // The in-memory theme still applies for this run.
      }
      return { theme }
    })
  },
}))
