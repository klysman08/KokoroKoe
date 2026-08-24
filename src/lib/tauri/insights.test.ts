import { invoke } from "@tauri-apps/api/core"
import { beforeEach, describe, expect, it, vi } from "vitest"

import fixture from "../../../fixtures/contracts/recent-insights-v1.json"
import { recentInsightsFixtureSchema } from "@/contracts/insights"
import {
  closeInsightsWindow,
  generateRecentInsights,
  openInsightsWindow,
} from "./insights"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const contract = recentInsightsFixtureSchema.parse(fixture)

describe("recent insights Tauri adapter", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
  })

  it("invokes only the strict identity-scoped request wrapper", async () => {
    vi.mocked(invoke).mockResolvedValue(contract.response)

    await expect(generateRecentInsights(contract.request)).resolves.toEqual(
      contract.response,
    )
    expect(invoke).toHaveBeenCalledWith("generate_recent_insights", {
      request: contract.request,
    })
    const call = vi.mocked(invoke).mock.calls[0]
    expect(call).toBeDefined()
    expect(Object.keys(call?.[1] as object)).toEqual(["request"])
    expect(Object.keys(contract.request)).toEqual([
      "requestId",
      "projectId",
      "sessionId",
    ])
  })

  it("rejects malformed and cross-scope success data", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...contract.response,
      sessionId: "99999999-9999-4999-8999-999999999999",
    })
    await expect(
      generateRecentInsights(contract.request),
    ).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })

    vi.mocked(invoke).mockResolvedValue({
      ...contract.response,
      insights: [
        { ...contract.response.insights[0], confidence: 1.5 },
        ...contract.response.insights.slice(1),
      ],
    })
    await expect(
      generateRecentInsights(contract.request),
    ).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })

    await expect(
      generateRecentInsights({
        ...contract.request,
        sessionId: "not-a-uuid" as unknown as typeof contract.request.sessionId,
      }),
    ).rejects.toMatchObject({
      details: { code: "invalid_request_contract" },
    })
  })
})

describe("insights window Tauri adapter", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
  })

  it("sends no label, url, path, or dimension to Rust", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    await openInsightsWindow()
    await closeInsightsWindow()

    expect(vi.mocked(invoke).mock.calls).toEqual([
      ["open_insights_window"],
      ["close_insights_window"],
    ])
  })

  it("surfaces sanitized backend failures", async () => {
    vi.mocked(invoke).mockRejectedValue({
      error: {
        code: "window_open_failed",
        userMessage: "KokoroKoe could not open the transcript window.",
        technicalDetail: "window_open_failed",
        severity: "error",
        retryable: true,
        correlationId: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
      },
    })

    await expect(openInsightsWindow()).rejects.toMatchObject({
      details: { code: "window_open_failed" },
    })
  })
})
