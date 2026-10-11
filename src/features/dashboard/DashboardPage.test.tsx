import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor, within } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import dashboardFixture from "../../../fixtures/contracts/dashboard-metrics-v1.json"
import projectFixture from "../../../fixtures/contracts/project-management-v1.json"

import { projectManagementFixtureSchema } from "@/contracts/projects"
import { DashboardPage } from "@/features/dashboard/DashboardPage"
import {
  formatBytes,
  formatDuration,
  formatUsd,
} from "@/features/dashboard/format"
import { dashboardQueryFor } from "@/features/dashboard/use-dashboard-query"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const project =
  projectManagementFixtureSchema.parse(projectFixture).page.items[0]!
const metrics = {
  ...dashboardFixture.metrics,
  projects: [{ projectId: project.id, sessions: 4 }],
}

function renderDashboard() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <DashboardPage />
    </QueryClientProvider>,
  )
}

function dashboardQueries() {
  return invokeMock.mock.calls
    .filter(([command]) => command === "get_dashboard_metrics")
    .map(([, args]) => (args as { query: Record<string, string> }).query)
}

describe("DashboardPage", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_dashboard_metrics") return metrics
      if (command === "list_projects") return { items: [project] }
      throw new Error(`Unexpected command: ${command}`)
    })
  })

  it("shows the totals, rankings, disk, and errors it was given", async () => {
    renderDashboard()

    const totals = await screen.findByRole("region", { name: "Totals" })
    expect(within(totals).getByText("4")).toBeInTheDocument()
    expect(within(totals).getByText("1 h 30 min")).toBeInTheDocument()
    expect(within(totals).getByText("$0.0288")).toBeInTheDocument()
    expect(within(totals).getByText("21")).toBeInTheDocument()
    // A project is named from the project list, not from the metrics.
    const ranking = screen
      .getByText("Most used projects")
      .closest<HTMLElement>('[data-slot="card"]')!
    expect(await within(ranking).findByText(project.name)).toBeInTheDocument()
    expect(screen.getByText("whisper-base")).toBeInTheDocument()
    expect(screen.getByText("openai/gpt-4.1-mini")).toBeInTheDocument()
    expect(screen.getByText("1.0 MB")).toBeInTheDocument()
    expect(
      screen.getByText(
        "OpenRouter needs credit before it can generate insights.",
      ),
    ).toBeInTheDocument()
  })

  it("asks for the chosen period and project", async () => {
    const user = userEvent.setup()
    renderDashboard()
    await screen.findByRole("region", { name: "Totals" })
    expect(dashboardQueries()[0]?.from).toBeDefined()

    await user.click(screen.getByRole("button", { name: "All time" }))
    await waitFor(() => expect(dashboardQueries().at(-1)).toEqual({}))

    await screen.findByRole("option", { name: project.name })
    await user.selectOptions(screen.getByLabelText("Project"), project.id)
    await waitFor(() =>
      expect(dashboardQueries().at(-1)).toEqual({ projectId: project.id }),
    )
  })

  it("says plainly when a period holds nothing", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_dashboard_metrics")
        return {
          ...metrics,
          sessions: 0,
          transcribedMs: 0,
          projects: [],
          transcriptionModels: [],
          llmModels: [],
          llmRequests: 0,
          inputTokens: 0,
          outputTokens: 0,
          actualCostUsd: "0.000000000000",
          insightsGenerated: 0,
          recentErrors: [],
        }
      if (command === "list_projects") return { items: [] }
      throw new Error(`Unexpected command: ${command}`)
    })
    renderDashboard()

    expect(
      await screen.findByText("No errors in this period."),
    ).toBeInTheDocument()
    expect(screen.getByText("$0.00")).toBeInTheDocument()
    expect(screen.getAllByText("No sessions in this period.")).toHaveLength(2)
  })

  it("shows a sanitized error instead of numbers it could not read", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_dashboard_metrics")
        throw {
          error: {
            code: "metrics_unavailable",
            userMessage: "KokoroKoe could not read its local metrics.",
            severity: "error",
            retryable: false,
            correlationId: "4b1f9c2e-7d3a-4e8b-a1c5-9e2d7f6b3a10",
          },
        }
      if (command === "list_projects") return { items: [] }
      throw new Error(`Unexpected command: ${command}`)
    })
    renderDashboard()

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "KokoroKoe could not read its local metrics.",
    )
  })
})

describe("dashboard formatting", () => {
  it("resolves a period against the moment of the read", () => {
    const now = Date.parse("2026-10-11T12:00:00Z")
    expect(dashboardQueryFor("7d", undefined, now)).toEqual({
      from: "2026-10-04T12:00:00.000Z",
    })
    expect(dashboardQueryFor("all", undefined, now)).toEqual({})
  })

  it("reads durations, sizes, and amounts the way a person does", () => {
    expect(formatDuration(0)).toBe("0 min")
    expect(formatDuration(30_000)).toBe("< 1 min")
    expect(formatDuration(3_600_000)).toBe("1 h")
    expect(formatBytes(512)).toBe("512 B")
    expect(formatBytes(147_951_465)).toBe("141 MB")
    expect(formatUsd("12.500000000000")).toBe("$12.50")
    expect(formatUsd("0.000000000001")).toBe("< $0.0001")
  })
})
