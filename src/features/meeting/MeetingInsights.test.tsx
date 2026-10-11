import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import insightsFixture from "../../../fixtures/contracts/recent-insights-v1.json"
import recordsFixture from "../../../fixtures/contracts/project-session-v1.json"

import { projectSchema, sessionSchema } from "@/contracts/projects"
import { MeetingInsights } from "@/features/meeting/MeetingInsights"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const project = projectSchema.parse(recordsFixture.project)
const configured = sessionSchema.parse({
  ...recordsFixture.session,
  state: "transcribing",
  llmModels: { insights: "example/insights" },
})

function generations() {
  return invokeMock.mock.calls.filter(
    ([command]) => command === "generate_recent_insights",
  )
}

function answerWithFixture() {
  invokeMock.mockImplementation(async (command, args) => {
    if (command === "list_openrouter_models") return []
    if (command === "generate_recent_insights") {
      const { request } = args as {
        request: { requestId: string; projectId: string; sessionId: string }
      }
      return { ...insightsFixture.response, ...request }
    }
    throw new Error(`Unexpected command: ${command}`)
  })
}

describe("MeetingInsights", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("waits for enough speech, then generates on its own", async () => {
    answerWithFixture()
    const { rerender } = render(
      <MeetingInsights finals={2} project={project} session={configured} />,
    )
    expect(generations()).toHaveLength(0)

    rerender(
      <MeetingInsights finals={3} project={project} session={configured} />,
    )
    expect(
      await screen.findByText("Transcript stays on this machine"),
    ).toBeInTheDocument()
    expect(generations()).toHaveLength(1)
  })

  /** An empty answer is a result, not a fault; it must not look like silence. */
  it("says when a check found nothing new", async () => {
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "list_openrouter_models") return []
      if (command === "generate_recent_insights") {
        const { request } = args as {
          request: { requestId: string; projectId: string; sessionId: string }
        }
        return { ...insightsFixture.response, ...request, insights: [] }
      }
      throw new Error(`Unexpected command: ${command}`)
    })
    render(
      <MeetingInsights finals={3} project={project} session={configured} />,
    )
    expect(
      await screen.findByText(/nothing new to add yet/i),
    ).toBeInTheDocument()
  })

  it("never generates on its own while the meeting is paused", () => {
    answerWithFixture()
    render(
      <MeetingInsights
        finals={10}
        project={project}
        session={{ ...configured, state: "paused" }}
      />,
    )
    expect(generations()).toHaveLength(0)
  })

  it("does nothing automatically once switched off, but still answers a click", async () => {
    answerWithFixture()
    const user = userEvent.setup()
    const { rerender } = render(
      <MeetingInsights finals={0} project={project} session={configured} />,
    )
    await user.click(
      screen.getByRole("checkbox", { name: /automatic insights/i }),
    )
    rerender(
      <MeetingInsights finals={9} project={project} session={configured} />,
    )
    expect(generations()).toHaveLength(0)

    await user.click(screen.getByRole("button", { name: "Generate now" }))
    await waitFor(() => expect(generations()).toHaveLength(1))
  })

  /** A missing key would fail the same way every 90 seconds, at a cost. */
  it("switches itself off on a failure that would repeat", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_openrouter_models") return []
      if (command === "generate_recent_insights")
        throw {
          error: {
            code: "insight_credential_required",
            userMessage: "Add an OpenRouter key in Settings.",
            severity: "warning",
            retryable: false,
            correlationId: "4b1f9c2e-7d3a-4e8b-a1c5-9e2d7f6b3a10",
          },
        }
      throw new Error(`Unexpected command: ${command}`)
    })
    render(
      <MeetingInsights finals={3} project={project} session={configured} />,
    )

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Add an OpenRouter key in Settings.",
    )
    expect(
      screen.getByRole("checkbox", { name: /automatic insights/i }),
    ).not.toBeChecked()
    expect(screen.getByText(/automatic insights stopped/i)).toBeInTheDocument()
  })

  it("explains what is missing when the meeting has no insights model", () => {
    render(
      <MeetingInsights
        finals={5}
        project={project}
        session={{ ...configured, llmModels: {} }}
      />,
    )
    expect(screen.getByText("Insights are not set up")).toBeInTheDocument()
    expect(invokeMock).not.toHaveBeenCalled()
  })
})
