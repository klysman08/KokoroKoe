import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/recent-insights-v1.json"
import {
  generateRecentInsightsRequestSchema,
  recentInsightsFixtureSchema,
  recentInsightsResponseSchema,
} from "@/contracts/insights"

describe("recent insights contract", () => {
  it("parses the shared Rust/Zod fixture", () => {
    expect(recentInsightsFixtureSchema.parse(fixture)).toEqual(fixture)
  })

  it("rejects unknown request fields and nullable optional usage", () => {
    expect(
      generateRecentInsightsRequestSchema.safeParse({
        ...fixture.request,
        extra: true,
      }).success,
    ).toBe(false)
    expect(
      recentInsightsResponseSchema.safeParse({
        ...fixture.response,
        repairUsage: null,
      }).success,
    ).toBe(false)
  })

  it("rejects controlled, out-of-range, and duplicated generated content", () => {
    const contract = recentInsightsFixtureSchema.parse(fixture)
    const [first, ...rest] = contract.response.insights
    if (!first) throw new Error("the fixture must carry at least one insight")
    const bell = String.fromCharCode(7)
    expect(
      recentInsightsResponseSchema.safeParse({
        ...fixture.response,
        insights: [
          { ...first, title: "control" + bell + "character" },
          ...rest,
        ],
      }).success,
    ).toBe(false)
    expect(
      recentInsightsResponseSchema.safeParse({
        ...fixture.response,
        insights: [{ ...first, confidence: 1.5 }, ...rest],
      }).success,
    ).toBe(false)
    expect(
      recentInsightsResponseSchema.safeParse({
        ...fixture.response,
        insights: [first, first],
      }).success,
    ).toBe(false)
    expect(
      recentInsightsResponseSchema.safeParse({
        ...fixture.response,
        insights: [
          {
            ...first,
            relatedSegmentIds: [
              first.relatedSegmentIds[0],
              first.relatedSegmentIds[0],
            ],
          },
          ...rest,
        ],
      }).success,
    ).toBe(false)
  })

  it("rejects unpriced costs and repair mismatches", () => {
    expect(
      recentInsightsResponseSchema.safeParse({
        ...fixture.response,
        sessionActualCostUsd: "0.1",
      }).success,
    ).toBe(false)
    expect(
      recentInsightsResponseSchema.safeParse({
        ...fixture.response,
        repaired: true,
      }).success,
    ).toBe(false)
  })
})
