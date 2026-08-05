import { render, screen } from "@testing-library/react"
import { vi } from "vitest"

import { ErrorBoundary } from "@/app/errors/ErrorBoundary"

function ThrowingChild(): never {
  throw new Error("secret-canary C:\\Users\\Example\\meeting.md")
}

describe("ErrorBoundary", () => {
  it("replaces raw render failures with a sanitized report", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {})

    render(
      <ErrorBoundary>
        <ThrowingChild />
      </ErrorBoundary>,
    )

    expect(screen.getByRole("alert")).toHaveTextContent(
      /unexpected application error/i,
    )
    expect(screen.getByRole("alert")).not.toHaveTextContent("secret-canary")
    expect(screen.getByRole("alert")).not.toHaveTextContent("meeting.md")
    consoleError.mockRestore()
  })
})
