import { create } from "zustand"

type ShellState = {
  navigationCollapsed: boolean
  toggleNavigation: () => void
}

export const useShellStore = create<ShellState>((set) => ({
  navigationCollapsed: false,
  toggleNavigation: () => {
    set((state) => ({ navigationCollapsed: !state.navigationCollapsed }))
  },
}))
