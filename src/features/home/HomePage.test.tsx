import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import appSettingsFixture from "../../../fixtures/contracts/app-settings-v1.json"
import audioFixture from "../../../fixtures/contracts/audio-device-test-v1.json"
import presetFixture from "../../../fixtures/contracts/preset-catalog-v1.json"
import projectFixture from "../../../fixtures/contracts/project-management-v1.json"
import projectSessionFixture from "../../../fixtures/contracts/project-session-v1.json"
import sessionLifecycleFixture from "../../../fixtures/contracts/session-lifecycle-v1.json"

import { projectManagementFixtureSchema } from "@/contracts/projects"
import { HomePage } from "@/features/home/HomePage"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const projects = projectManagementFixtureSchema.parse(projectFixture)
// A two-entry stand-in for the Rust-owned library: the preset the fixtures
// already default to, and one other to switch to.
const presets = [
  {
    ...presetFixture,
    id: "00000000-0000-4000-8000-000000000001",
    name: "Technical interview",
  },
  presetFixture,
]

function renderHome() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <HomePage />
    </QueryClientProvider>,
  )
}

describe("HomePage project management", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    vi.mocked(listen).mockResolvedValue(() => undefined)
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_settings") return appSettingsFixture
      if (command === "list_presets") return presets
      if (command === "list_projects") return projects.page
      if (command === "create_project") return projects.page.items[0]
      if (command === "update_project") return projects.page.items[0]
      if (command === "list_sessions") return { items: [] }
      if (command === "list_audio_devices") return audioFixture.deviceList
      if (command === "create_session") return projectSessionFixture.session
      throw new Error(`Unexpected command: ${command}`)
    })
  })

  it("lists projects and creates one with the current local defaults", async () => {
    const user = userEvent.setup()
    renderHome()

    expect(
      await screen.findByText("Weekly product meetings"),
    ).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Create project" }))
    await user.type(
      screen.getByRole("textbox", { name: "Project name" }),
      "Customer discovery",
    )
    await user.type(
      screen.getByRole("textbox", { name: /^Participants/ }),
      "Research lead",
    )
    await user.type(
      screen.getByRole("textbox", { name: /^Tags/ }),
      "research, customer",
    )
    await user.click(screen.getByRole("button", { name: "Save project" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_project", {
        name: "Customer discovery",
        description: "",
        globalContext: "",
        participants: ["Research lead"],
        tags: ["research", "customer"],
        defaultPresetId: appSettingsFixture.defaultPresetId,
        defaultTranscriptionModelId:
          appSettingsFixture.defaultTranscriptionModelId,
        preferredLlmModels: {},
      }),
    )
  })

  it("creates a project on a chosen default preset", async () => {
    const user = userEvent.setup()
    renderHome()

    await user.click(
      await screen.findByRole("button", { name: "Create project" }),
    )
    const picker = screen.getByRole("combobox", { name: /^Default preset/ })
    expect(picker).toHaveValue(appSettingsFixture.defaultPresetId)

    await user.selectOptions(picker, presetFixture.id)
    await user.type(
      screen.getByRole("textbox", { name: "Project name" }),
      "Customer discovery",
    )
    await user.click(screen.getByRole("button", { name: "Save project" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "create_project",
        expect.objectContaining({ defaultPresetId: presetFixture.id }),
      ),
    )
  })

  it("sends an edit with the displayed revision", async () => {
    const user = userEvent.setup()
    renderHome()

    await user.click(
      await screen.findByRole("button", {
        name: "Edit Weekly product meetings",
      }),
    )
    const name = screen.getByRole("textbox", { name: "Project name" })
    await user.clear(name)
    await user.type(name, "Weekly product review")
    await user.click(screen.getByRole("button", { name: "Save project" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_project", {
        projectId: projects.page.items[0]!.id,
        expectedRevision: projects.page.items[0]!.revision,
        value: {
          name: "Weekly product review",
          description: projects.page.items[0]!.description,
          globalContext: projects.page.items[0]!.globalContext,
          participants: projects.page.items[0]!.participants,
          tags: projects.page.items[0]!.tags,
          defaultPresetId: projects.page.items[0]!.defaultPresetId,
        },
      }),
    )
  })

  it("opens a project-scoped session list and creates bounded session metadata", async () => {
    const user = userEvent.setup()
    renderHome()

    await user.click(
      await screen.findByRole("button", { name: "Manage sessions" }),
    )
    expect(
      await screen.findByText("No sessions in this project"),
    ).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "New session" }))
    await user.type(
      screen.getByRole("textbox", { name: "Session title" }),
      "Sprint planning",
    )
    await user.type(
      screen.getByRole("textbox", { name: "Objective" }),
      "Agree the sprint scope",
    )
    await user.click(screen.getByRole("button", { name: "Save session" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "create_session",
        expect.objectContaining({
          projectId: projects.page.items[0]!.id,
          title: "Sprint planning",
          objective: "Agree the sprint scope",
          language: "en-GB",
          transcriptionModelId:
            projects.page.items[0]!.defaultTranscriptionModelId,
          llmModels: appSettingsFixture.defaultLlmModels,
          spendingLimitUsd: appSettingsFixture.defaultSessionBudgetUsd,
          maxTokensPerRequest: appSettingsFixture.maxTokensPerRequest,
          retainAudio: false,
        }),
      ),
    )
  })

  it("freezes the chosen preset into the session it creates", async () => {
    const user = userEvent.setup()
    renderHome()

    await user.click(
      await screen.findByRole("button", { name: "Manage sessions" }),
    )
    await user.click(await screen.findByRole("button", { name: "New session" }))
    const picker = screen.getByRole("combobox", { name: "Preset" })
    expect(picker).toHaveValue(projects.page.items[0]!.defaultPresetId)

    await user.selectOptions(picker, presetFixture.id)
    expect(screen.getByText(/Insights: Decision, Action item/)).toBeVisible()
    await user.type(
      screen.getByRole("textbox", { name: "Session title" }),
      "Sprint planning",
    )
    await user.click(screen.getByRole("button", { name: "Save session" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "create_session",
        expect.objectContaining({
          preset: {
            id: presetFixture.id,
            version: presetFixture.version,
            name: presetFixture.name,
            assistantRole: presetFixture.assistantRole,
            analysisObjectives: presetFixture.analysisObjectives,
            insightTypes: presetFixture.insightTypes,
            responseTone: presetFixture.responseTone,
            finalSummarySections: presetFixture.finalSummarySections,
            highlightInstructions: presetFixture.highlightInstructions,
            prohibitedBehaviors: presetFixture.prohibitedBehaviors,
          },
        }),
      ),
    )
  })

  it("keeps a session on the preset it froze when the library moved on", async () => {
    const user = userEvent.setup()
    const editable = { ...projectSessionFixture.session, state: "idle" }
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_settings") return appSettingsFixture
      // The library no longer offers the preset this session was created with.
      if (command === "list_presets") return [presetFixture]
      if (command === "list_projects") return projects.page
      if (command === "list_sessions") return { items: [editable] }
      if (command === "list_audio_devices") return audioFixture.deviceList
      throw new Error(`Unexpected command: ${command}`)
    })
    renderHome()

    await user.click(
      await screen.findByRole("button", { name: "Manage sessions" }),
    )
    await user.click(
      await screen.findByRole("button", { name: `Edit ${editable.title}` }),
    )
    const picker = screen.getByRole("combobox", { name: "Preset" })

    expect(picker).toHaveValue(editable.preset.id)
    expect(
      screen.getByRole("option", { name: editable.preset.name }),
    ).toBeInTheDocument()
    expect(
      screen.getByRole("option", { name: presetFixture.name }),
    ).toBeInTheDocument()
  })

  it("requires capture consent and starts the displayed persisted session revision", async () => {
    const user = userEvent.setup()
    const { startedAt: _startedAt, ...withoutStartedAt } =
      projectSessionFixture.session
    expect(_startedAt).toBeDefined()
    const idleSession = {
      ...withoutStartedAt,
      state: "idle",
      revision: 1,
      updatedAt: projectSessionFixture.session.createdAt,
      channelHealth: {
        microphone: {
          ...projectSessionFixture.session.channelHealth.microphone,
          status: "stopped",
          updatedAt: projectSessionFixture.session.createdAt,
        },
        systemOutput: {
          ...projectSessionFixture.session.channelHealth.systemOutput,
          status: "stopped",
          updatedAt: projectSessionFixture.session.createdAt,
        },
      },
    }
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_settings") return appSettingsFixture
      if (command === "list_presets") return presets
      if (command === "list_projects") return projects.page
      if (command === "list_sessions") return { items: [idleSession] }
      if (command === "list_audio_devices") return audioFixture.deviceList
      if (command === "start_session") return projectSessionFixture.session
      throw new Error(`Unexpected command: ${command}`)
    })
    renderHome()

    await user.click(
      await screen.findByRole("button", { name: "Manage sessions" }),
    )
    const start = await screen.findByRole("button", { name: "Start" })
    expect(start).toBeDisabled()
    await user.click(
      screen.getByRole("checkbox", {
        name: /I consent to local microphone and system-audio capture/i,
      }),
    )
    await user.click(start)

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("start_session", {
        projectId: idleSession.projectId,
        sessionId: idleSession.id,
        expectedRevision: 1,
        requestId: expect.any(String),
        acknowledgedCaptureConsent: true,
      }),
    )
  })

  it("shows scoped live session text and replaces its partial at the live edge", async () => {
    const user = userEvent.setup()
    const handlers = new Map<string, (event: { payload: unknown }) => void>()
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      handlers.set(event, handler as (event: { payload: unknown }) => void)
      return () => undefined
    })
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_settings") return appSettingsFixture
      if (command === "list_presets") return presets
      if (command === "list_projects") return projects.page
      if (command === "list_sessions") {
        return { items: [projectSessionFixture.session] }
      }
      if (command === "list_audio_devices") return audioFixture.deviceList
      throw new Error(`Unexpected command: ${command}`)
    })
    renderHome()

    await user.click(
      await screen.findByRole("button", { name: "Manage sessions" }),
    )
    expect(
      await screen.findByRole("region", {
        name: /live transcript for sprint planning/i,
      }),
    ).toHaveTextContent("Listening for speech")

    handlers.get("session-transcription-partial")?.({
      payload: sessionLifecycleFixture.partialEvent,
    })
    expect(await screen.findByText("provisional words")).toBeInTheDocument()

    handlers.get("session-transcription-final")?.({
      payload: sessionLifecycleFixture.finalEvent,
    })
    expect(await screen.findByText("final words")).toBeInTheDocument()
    expect(screen.queryByText("provisional words")).not.toBeInTheDocument()
  })
})
