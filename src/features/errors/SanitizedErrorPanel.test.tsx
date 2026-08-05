import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import { ApplicationError } from "@/contracts/app-error"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { createSanitizedErrorReport } from "@/features/errors/sanitized-error-report"

const error = new ApplicationError({
  code: "settings_unavailable",
  userMessage: "Settings are unavailable.",
  technicalDetail: "The default settings service is unavailable.",
  severity: "error",
  retryable: true,
  correlationId: "1a5986f2-4647-440e-a125-87a8f46fa80a",
})

describe("SanitizedErrorPanel", () => {
  it("copies only the bounded application error fields", async () => {
    const user = userEvent.setup()
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    })

    render(<SanitizedErrorPanel error={error} />)
    await user.click(screen.getByRole("button", { name: /copy sanitized/i }))

    expect(writeText).toHaveBeenCalledWith(createSanitizedErrorReport(error))
    expect(await screen.findByText(/sanitized report copied/i)).toBeVisible()
  })

  it("reports clipboard denial without exposing another error", async () => {
    const user = userEvent.setup()
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn().mockRejectedValue(new Error("secret-canary")),
      },
    })

    render(<SanitizedErrorPanel error={error} />)
    await user.click(screen.getByRole("button", { name: /copy sanitized/i }))

    expect(await screen.findByText(/copy is unavailable/i)).toBeVisible()
    expect(screen.queryByText(/secret-canary/i)).not.toBeInTheDocument()
  })
})
