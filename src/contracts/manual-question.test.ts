import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/manual-question-v1.json"
import {
  askManualQuestionRequestSchema,
  manualQuestionFixtureSchema,
  manualQuestionResponseSchema,
} from "@/contracts/manual-question"

describe("manual question contract", () => {
  it("parses the shared Rust/Zod fixture", () => {
    expect(manualQuestionFixtureSchema.parse(fixture)).toEqual(fixture)
  })

  it("rejects unknown, untrimmed, controlled, duplicate, and nullable values", () => {
    expect(
      askManualQuestionRequestSchema.safeParse({
        ...fixture.request,
        question: ` ${fixture.request.question}`,
      }).success,
    ).toBe(false)
    expect(
      askManualQuestionRequestSchema.safeParse({
        ...fixture.request,
        question: "unsafe\u0000question",
      }).success,
    ).toBe(false)
    expect(
      askManualQuestionRequestSchema.safeParse({
        ...fixture.request,
        extra: true,
      }).success,
    ).toBe(false)
    expect(
      manualQuestionResponseSchema.safeParse({
        ...fixture.response,
        relatedSegmentIds: [
          fixture.response.selectedSegmentId,
          fixture.response.selectedSegmentId,
        ],
      }).success,
    ).toBe(false)
    expect(
      manualQuestionResponseSchema.safeParse({
        ...fixture.response,
        repairUsage: null,
      }).success,
    ).toBe(false)
  })
})
