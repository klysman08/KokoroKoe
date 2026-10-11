import dashboardFixture from "../../fixtures/contracts/dashboard-metrics-v1.json"

import {
  dashboardMetricsQuerySchema,
  dashboardMetricsSchema,
} from "@/contracts/dashboard"

describe("dashboard metrics contract", () => {
  it("parses the shared Rust and TypeScript golden fixture", () => {
    expect(dashboardMetricsSchema.parse(dashboardFixture.metrics)).toEqual(
      dashboardFixture.metrics,
    )
    expect(dashboardMetricsQuerySchema.parse(dashboardFixture.query)).toEqual(
      dashboardFixture.query,
    )
  })

  it("accepts an all-time query and rejects a null bound", () => {
    expect(dashboardMetricsQuerySchema.parse({})).toEqual({})
    expect(() => dashboardMetricsQuerySchema.parse({ from: null })).toThrow()
    expect(() => dashboardMetricsQuerySchema.parse({ extra: 1 })).toThrow()
  })

  it.each([
    ["actualCostUsd", "0.0288"],
    ["actualCostUsd", "-1.000000000000"],
    ["transcribedMs", -1],
    ["usageRecordedSince", "yesterday"],
    ["projects", Array(6).fill(dashboardFixture.metrics.projects[0])],
  ])("rejects an invalid %s", (field, value) => {
    expect(() =>
      dashboardMetricsSchema.parse({
        ...dashboardFixture.metrics,
        [field]: value,
      }),
    ).toThrow()
  })

  it("never accepts a technical detail on a recent error", () => {
    const [error] = dashboardFixture.metrics.recentErrors
    expect(() =>
      dashboardMetricsSchema.parse({
        ...dashboardFixture.metrics,
        recentErrors: [{ ...error, technicalDetail: "C:\\Users\\meeting.md" }],
      }),
    ).toThrow()
  })
})
