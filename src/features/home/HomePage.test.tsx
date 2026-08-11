import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import appSettingsFixture from "../../../fixtures/contracts/app-settings-v1.json"
import audioFixture from "../../../fixtures/contracts/audio-device-test-v1.json"
import projectFixture from "../../../fixtures/contracts/project-management-v1.json"
import projectSessionFixture from "../../../fixtures/contracts/project-session-v1.json"

import { projectManagementFixtureSchema } from "@/contracts/projects"
import { HomePage } from "@/features/home/HomePage"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const projects = projectManagementFixtureSchema.parse(projectFixture)

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
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_settings") return appSettingsFixture
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
          retainAudio: false,
        }),
      ),
    )
  })
})
