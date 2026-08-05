import {
  ApplicationError,
  commandErrorSchema,
  createUnexpectedApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import commandErrorFixture from "../../fixtures/contracts/command-error-v1.json"

describe("application errors", () => {
  it("parses the shared Rust and TypeScript command-error fixture", () => {
    expect(commandErrorSchema.parse(commandErrorFixture)).toEqual(
      commandErrorFixture,
    )
  })

  it.each([
    ["code", ""],
    ["userMessage", ""],
    ["technicalDetail", null],
    ["technicalDetail", "😀".repeat(257)],
    ["correlationId", "not-a-uuid"],
  ])("rejects an invalid %s", (field, invalidValue) => {
    expect(() =>
      commandErrorSchema.parse({
        error: {
          ...commandErrorFixture.error,
          [field]: invalidValue,
        },
      }),
    ).toThrow()
  })

  it("accepts only the typed command envelope", () => {
    const error = toApplicationError({
      error: {
        code: "settings_unavailable",
        userMessage: "Settings are unavailable.",
        severity: "error",
        retryable: true,
        correlationId: "1a5986f2-4647-440e-a125-87a8f46fa80a",
      },
    })

    expect(error).toBeInstanceOf(ApplicationError)
    expect(error.details.code).toBe("settings_unavailable")
  })

  it("does not preserve untrusted rejection content", () => {
    const error = toApplicationError({
      rawResponse: "secret-canary C:\\Users\\Example\\meeting.md",
    })

    expect(JSON.stringify(error.details)).not.toContain("secret-canary")
    expect(JSON.stringify(error.details)).not.toContain("meeting.md")
  })

  it("creates a UUID correlation ID for unexpected failures", () => {
    expect(createUnexpectedApplicationError().details.correlationId).toMatch(
      /^[0-9a-f-]{36}$/,
    )
  })
})
