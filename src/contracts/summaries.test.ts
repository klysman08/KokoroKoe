import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/session-summary-v1.json"
import {
  generateSessionSummaryRequestSchema,
  generateSessionSummaryResponseSchema,
  sessionSummaryFixtureSchema,
  sessionSummaryStatusSchema,
} from "@/contracts/summaries"

describe("session summary contract", () => {
  it("parses the shared Rust/Zod fixture", () => {
    expect(sessionSummaryFixtureSchema.parse(fixture)).toEqual(fixture)
  })

  it("rejects unknown request fields and nullable optional values", () => {
    expect(
      generateSessionSummaryRequestSchema.safeParse({
        ...fixture.generateRequest,
        extra: true,
      }).success,
    ).toBe(false)
    expect(
      sessionSummaryStatusSchema.safeParse({
        ...fixture.status,
        document: null,
      }).success,
    ).toBe(false)
    expect(
      generateSessionSummaryResponseSchema.safeParse({
        ...fixture.generateResponse,
        repairUsage: null,
      }).success,
    ).toBe(false)
  })

  it("rejects a saved document that does not match its own scope", () => {
    expect(
      sessionSummaryStatusSchema.safeParse({
        ...fixture.status,
        document: {
          ...fixture.status.document,
          sessionId: "99999999-9999-4999-8999-999999999999",
        },
      }).success,
    ).toBe(false)
  })

  it("rejects impossible coverage, carriage returns, and unpriced costs", () => {
    const { document } = fixture.generateResponse
    expect(
      generateSessionSummaryResponseSchema.safeParse({
        ...fixture.generateResponse,
        document: { ...document, segmentsIncluded: 9_999 },
      }).success,
    ).toBe(false)
    expect(
      generateSessionSummaryResponseSchema.safeParse({
        ...fixture.generateResponse,
        document: { ...document, markdown: "# Summary\r\n" },
      }).success,
    ).toBe(false)
    expect(
      generateSessionSummaryResponseSchema.safeParse({
        ...fixture.generateResponse,
        availableBudgetUsd: "0.5",
      }).success,
    ).toBe(false)
    expect(
      generateSessionSummaryResponseSchema.safeParse({
        ...fixture.generateResponse,
        repaired: true,
      }).success,
    ).toBe(false)
  })
})
