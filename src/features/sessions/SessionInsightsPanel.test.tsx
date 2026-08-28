import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import projectSessionFixture from "../../../fixtures/contracts/project-session-v1.json"
import recentInsightsFixture from "../../../fixtures/contracts/recent-insights-v1.json"
import {
  generateAlternativeInsightRequestSchema,
  generateRecentInsightsRequestSchema,
} from "@/contracts/insights"
import { projectSessionFixtureSchema } from "@/contracts/projects"
import { SessionInsightsPanel } from "./SessionInsightsPanel"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const scope = projectSessionFixtureSchema.parse(projectSessionFixture)
const invokeMock = vi.mocked(invoke)

function renderPanel() {
  return render(
    <SessionInsightsPanel project={scope.project} session={scope.session} />,
  )
}

describe("SessionInsightsPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("generates nothing until the user asks", () => {
    renderPanel()

    expect(invokeMock).not.toHaveBeenCalled()
    expect(
      screen.getByRole("button", { name: /generate insights/i }),
    ).toBeEnabled()
  })

  it("sends only the scoped identity and renders the typed insights", async () => {
    invokeMock.mockImplementation(async (command, args) => {
      expect(command).toBe("generate_recent_insights")
      const payload = args as { request: unknown }
      expect(Object.keys(payload)).toEqual(["request"])
      const request = generateRecentInsightsRequestSchema.parse(payload.request)
      expect(Object.keys(request)).toEqual([
        "requestId",
        "projectId",
        "sessionId",
      ])
      expect(request.projectId).toBe(scope.project.id)
      expect(request.sessionId).toBe(scope.session.id)
      return {
        ...recentInsightsFixture.response,
        requestId: request.requestId,
        projectId: request.projectId,
        sessionId: request.sessionId,
      }
    })
    renderPanel()

    await userEvent.click(
      screen.getByRole("button", { name: /generate insights/i }),
    )

    await waitFor(() =>
      expect(
        screen.getByText("Transcript stays on this machine"),
      ).toBeInTheDocument(),
    )
    expect(screen.getByText("Decision")).toBeInTheDocument()
    expect(screen.getByText("Action item")).toBeInTheDocument()
    expect(
      screen.getByText(
        `$${recentInsightsFixture.response.availableBudgetUsd}`,
        {
          exact: false,
        },
      ),
    ).toBeInTheDocument()
  })

  it("shows the sanitized panel when generation fails", async () => {
    invokeMock.mockRejectedValue({
      error: {
        code: "insight_transcript_empty",
        userMessage:
          "No finalized transcript is available yet. Wait for speech to be transcribed and try again.",
        technicalDetail: "insight_transcript_empty",
        severity: "info",
        retryable: true,
        correlationId: "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
      },
    })
    renderPanel()

    await userEvent.click(
      screen.getByRole("button", { name: /generate insights/i }),
    )

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /no finalized transcript is available yet/i,
      ),
    )
  })

  /// The window is Rust-owned: the control names one of the two windows and
  /// sends no label, URL, path, or dimension.
  it("pops the insights window out by naming it and nothing else", async () => {
    invokeMock.mockResolvedValue(undefined)
    renderPanel()

    await userEvent.click(
      screen.getByRole("button", { name: /pop out insights/i }),
    )

    expect(invokeMock.mock.calls).toEqual([
      ["open_detached_window", { request: { window: "insights" } }],
    ])
  })

  it("shows a sanitized failure when the window cannot open", async () => {
    invokeMock.mockRejectedValue({
      error: {
        code: "window_open_failed",
        userMessage: "KokoroKoe could not open the transcript window.",
        technicalDetail: "window_open_failed",
        severity: "error",
        retryable: true,
        correlationId: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
      },
    })
    renderPanel()

    await userEvent.click(
      screen.getByRole("button", { name: /pop out insights/i }),
    )

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /could not open the transcript window/i,
      ),
    )
  })
})

describe("SessionInsightsPanel alternatives", () => {
  const insight = recentInsightsFixture.response.insights[0]!

  beforeEach(() => {
    invokeMock.mockReset()
  })

  /// An alternative asks again about the insight's own moments: it names the
  /// type and the segments it cited, and carries none of the model's earlier
  /// text — the transcript is the input, not what the model said before.
  it("asks again about one insight's own moments and keeps the original", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command, args) => {
      const request = (args as { request: Record<string, unknown> }).request
      if (command === "generate_recent_insights")
        return {
          ...recentInsightsFixture.response,
          requestId: request.requestId,
          projectId: request.projectId,
          sessionId: request.sessionId,
        }
      if (command === "generate_alternative_insight") {
        const parsed = generateAlternativeInsightRequestSchema.parse(request)
        expect(parsed.insightType).toBe(insight.type)
        expect(parsed.segmentIds).toEqual(insight.relatedSegmentIds)
        expect(JSON.stringify(request)).not.toContain(insight.content)
        return {
          ...recentInsightsFixture.response,
          requestId: parsed.requestId,
          projectId: parsed.projectId,
          sessionId: parsed.sessionId,
          insights: [
            {
              ...insight,
              title: "A second reading",
              content: "Another way to read the same moment.",
            },
          ],
          sessionActualCostUsd: "0.001500000000",
          availableBudgetUsd: "0.998500000000",
        }
      }
      throw new Error(`Unexpected command: ${command}`)
    })
    renderPanel()
    await user.click(screen.getByRole("button", { name: /generate insights/i }))
    await screen.findByText(insight.title)

    await user.click(screen.getAllByRole("button", { name: /ask again/i })[0]!)

    expect(await screen.findByText("A second reading")).toBeInTheDocument()
    // The original is still there to compare against.
    expect(screen.getByText(insight.title)).toBeInTheDocument()
    // Asking again costs money, so the figures move with it.
    expect(screen.getByText(/0\.001500000000 session cost/)).toBeInTheDocument()
  })

  it("surfaces a sanitized failure when asking again is refused", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command, args) => {
      const request = (args as { request: Record<string, unknown> }).request
      if (command === "generate_recent_insights")
        return {
          ...recentInsightsFixture.response,
          requestId: request.requestId,
          projectId: request.projectId,
          sessionId: request.sessionId,
        }
      throw {
        error: {
          code: "insight_moment_unavailable",
          userMessage:
            "That moment is no longer in the recent transcript window.",
          technicalDetail: "insight_moment_unavailable",
          severity: "info",
          retryable: false,
          correlationId: "ffffffff-ffff-4fff-8fff-ffffffffffff",
        },
      }
    })
    renderPanel()
    await user.click(screen.getByRole("button", { name: /generate insights/i }))
    await screen.findByText(insight.title)

    await user.click(screen.getAllByRole("button", { name: /ask again/i })[0]!)

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /no longer in the recent transcript window/i,
    )
  })
})
