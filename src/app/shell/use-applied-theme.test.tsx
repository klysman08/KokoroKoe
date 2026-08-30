import { render } from "@testing-library/react"
import { act } from "react"
import { beforeEach, describe, expect, it } from "vitest"

import { useShellStore } from "@/stores/shell-store"
import { THEME_STORAGE_KEY, useAppliedTheme } from "./use-applied-theme"

function Surface() {
  useAppliedTheme()
  return <p>a window</p>
}

describe("useAppliedTheme", () => {
  beforeEach(() => {
    document.documentElement.classList.remove("dark")
    document.documentElement.style.colorScheme = ""
    useShellStore.setState({ theme: "light" })
  })

  /// The bug this exists for: a detached window renders the same document
  /// under a different route, so without this it stayed light while the rest
  /// of the application was dark.
  it("paints any surface that uses it, not only the main shell", () => {
    useShellStore.setState({ theme: "dark" })

    render(<Surface />)

    expect(document.documentElement).toHaveClass("dark")
    expect(document.documentElement.style.colorScheme).toBe("dark")
  })

  /// The windows share one origin, so the choice travels between them as the
  /// browser's own cross-document storage event.
  it("follows a theme another window chose", () => {
    render(<Surface />)
    expect(document.documentElement).not.toHaveClass("dark")

    act(() => {
      globalThis.dispatchEvent(
        new StorageEvent("storage", {
          key: THEME_STORAGE_KEY,
          newValue: "dark",
        }),
      )
    })

    expect(useShellStore.getState().theme).toBe("dark")
    expect(document.documentElement).toHaveClass("dark")
  })

  it("ignores storage changes that are not the theme", () => {
    useShellStore.setState({ theme: "dark" })
    render(<Surface />)

    act(() => {
      globalThis.dispatchEvent(
        new StorageEvent("storage", {
          key: "something-else",
          newValue: "light",
        }),
      )
    })

    expect(useShellStore.getState().theme).toBe("dark")
  })
})
