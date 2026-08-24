import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import projectSessionFixture from "../../../fixtures/contracts/project-session-v1.json"
import recentInsightsFixture from "../../../fixtures/contracts/recent-insights-v1.json"
import { generateRecentInsightsRequestSchema } from "@/contracts/insights"
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

  /// The window is Rust-owned: the control sends nothing that could name or
  /// redirect a webview.
  it("pops the insights window out without sending any argument", async () => {
    invokeMock.mockResolvedValue(undefined)
    renderPanel()

    await userEvent.click(
      screen.getByRole("button", { name: /pop out insights/i }),
    )
    await userEvent.click(
      screen.getByRole("button", { name: /close insights window/i }),
    )

    expect(invokeMock.mock.calls).toEqual([
      ["open_insights_window"],
      ["close_insights_window"],
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
