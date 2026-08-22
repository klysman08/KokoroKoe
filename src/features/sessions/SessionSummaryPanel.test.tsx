import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import projectSessionFixture from "../../../fixtures/contracts/project-session-v1.json"
import summaryFixture from "../../../fixtures/contracts/session-summary-v1.json"
import { projectSessionFixtureSchema } from "@/contracts/projects"
import {
  generateSessionSummaryRequestSchema,
  getSessionSummaryRequestSchema,
} from "@/contracts/summaries"
import { SessionSummaryPanel } from "./SessionSummaryPanel"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const scope = projectSessionFixtureSchema.parse(projectSessionFixture)
const invokeMock = vi.mocked(invoke)

function scoped<T extends { projectId: string; sessionId: string }>(value: T) {
  return { ...value, projectId: scope.project.id, sessionId: scope.session.id }
}

function renderPanel() {
  return render(
    <SessionSummaryPanel project={scope.project} session={scope.session} />,
  )
}

describe("SessionSummaryPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("reads the saved summary on mount and renders it as inert Markdown", async () => {
    invokeMock.mockImplementation(async (command, args) => {
      expect(command).toBe("get_session_summary")
      const request = getSessionSummaryRequestSchema.parse(
        (args as { request: unknown }).request,
      )
      expect(Object.keys(request)).toEqual(["projectId", "sessionId"])
      return {
        ...scoped(summaryFixture.status),
        document: scoped(summaryFixture.status.document),
      }
    })
    renderPanel()

    await waitFor(() =>
      expect(screen.getByText("Executive summary")).toBeInTheDocument(),
    )
    expect(screen.getByText(/40 of 42 segments used/i)).toBeInTheDocument()
    expect(
      screen.getByRole("button", { name: /regenerate summary/i }),
    ).toBeInTheDocument()
  })

  it("offers generation when no summary is saved yet", async () => {
    invokeMock.mockResolvedValue(scoped(summaryFixture.absentStatus))
    renderPanel()

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /generate summary/i }),
      ).toBeEnabled(),
    )
    expect(screen.getByText(/no summary has been saved/i)).toBeInTheDocument()
    expect(invokeMock).toHaveBeenCalledTimes(1)
  })

  it("publishes a summary on request and shows its coverage", async () => {
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "get_session_summary")
        return scoped(summaryFixture.absentStatus)
      expect(command).toBe("generate_session_summary")
      const request = generateSessionSummaryRequestSchema.parse(
        (args as { request: unknown }).request,
      )
      return {
        ...summaryFixture.generateResponse,
        requestId: request.requestId,
        document: scoped(summaryFixture.generateResponse.document),
      }
    })
    renderPanel()
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /generate summary/i }),
      ).toBeEnabled(),
    )

    await userEvent.click(
      screen.getByRole("button", { name: /generate summary/i }),
    )

    await waitFor(() =>
      expect(screen.getByText("Decisions")).toBeInTheDocument(),
    )
    expect(screen.getByText("Open questions")).toBeInTheDocument()
    expect(screen.getByText(/larger than this model/i)).toBeInTheDocument()
  })

  it("shows the sanitized panel when generation fails", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_session_summary")
        return scoped(summaryFixture.absentStatus)
      throw {
        error: {
          code: "summary_session_not_finished",
          userMessage: "Stop the Session before generating its final summary.",
          technicalDetail: "summary_session_not_finished",
          severity: "warning",
          retryable: false,
          correlationId: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
        },
      }
    })
    renderPanel()
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /generate summary/i }),
      ).toBeEnabled(),
    )

    await userEvent.click(
      screen.getByRole("button", { name: /generate summary/i }),
    )

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /stop the session before generating/i,
      ),
    )
  })
})
