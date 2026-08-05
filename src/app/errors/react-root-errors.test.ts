import { vi } from "vitest"

import { reactRootErrorHandlers } from "@/app/errors/react-root-errors"

describe("React root error handlers", () => {
  it("never forwards raw React failures to the console", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {})
    const error = new Error("secret-canary C:\\Users\\Example\\meeting.md")
    const componentStack = "at SecretCanary (C:\\Users\\Example\\Secret.tsx:1)"

    reactRootErrorHandlers.onCaughtError?.(error, { componentStack })
    reactRootErrorHandlers.onUncaughtError?.(error, { componentStack })
    reactRootErrorHandlers.onRecoverableError?.(error, { componentStack })

    expect(consoleError).not.toHaveBeenCalled()
    consoleError.mockRestore()
  })
})
