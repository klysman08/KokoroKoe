import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import settingsFixture from "../../../fixtures/contracts/app-settings-v1.json"
import audioFixture from "../../../fixtures/contracts/audio-device-test-v1.json"
import modelFixture from "../../../fixtures/contracts/model-management-v1.json"
import presetFixture from "../../../fixtures/contracts/preset-catalog-v1.json"
import recordsFixture from "../../../fixtures/contracts/project-session-v1.json"

import { HomePage } from "@/features/home/HomePage"
import { useActiveSessionStore } from "@/stores/active-session-store"
import { useMeetingStore } from "@/stores/meeting-store"
import { useNavigationStore } from "@/stores/navigation-store"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const settings = {
  ...settingsFixture,
  defaultTranscriptionModelId: modelFixture.installation.descriptor.id,
}
const installed = {
  ...modelFixture.installation,
  status: "installed",
  installedBytes: modelFixture.installation.descriptor.diskBytes,
  installedAt: "2026-08-10T09:05:00Z",
  selectedAsDefault: true,
  downloadJob: undefined,
}
const idleSession = { ...recordsFixture.session, state: "idle" }
const liveSession = { ...recordsFixture.session, state: "transcribing" }

function respond(models: unknown[]) {
  invokeMock.mockImplementation(async (command) => {
    switch (command) {
      case "get_settings":
        return settings
      case "list_transcription_models":
        return models
      case "get_openrouter_credential_status":
        return { configured: false }
      case "list_projects":
        return { items: [] }
      case "list_presets":
        return [presetFixture]
      case "list_audio_devices":
        return audioFixture.deviceList
      case "create_project":
        return recordsFixture.project
      case "create_session":
        return idleSession
      case "start_session":
        return liveSession
      default:
        throw new Error(`Unexpected command: ${command}`)
    }
  })
}

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

function argsOf(command: string) {
  return invokeMock.mock.calls.find(([name]) => name === command)?.[1] as
    Record<string, unknown> | undefined
}

describe("HomePage", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    useActiveSessionStore.setState({ activeSession: undefined })
    useMeetingStore.setState({ focus: undefined })
    useNavigationStore.setState({ activeRoute: "home" })
  })

  it("starts a meeting in one press, creating the General project first", async () => {
    respond([installed])
    const user = userEvent.setup()
    renderHome()

    const start = await screen.findByRole("button", { name: "Start meeting" })
    await waitFor(() => expect(start).toBeEnabled())
    await user.click(start)

    await waitFor(() =>
      expect(useNavigationStore.getState().activeRoute).toBe("meeting"),
    )
    expect(argsOf("create_project")).toMatchObject({ name: "General" })
    expect(argsOf("create_session")).toMatchObject({
      projectId: recordsFixture.project.id,
    })
    expect(argsOf("start_session")).toMatchObject({
      sessionId: idleSession.id,
      acknowledgedCaptureConsent: true,
    })
    expect(useMeetingStore.getState().focus).toEqual({
      projectId: recordsFixture.project.id,
      sessionId: idleSession.id,
    })
    expect(useActiveSessionStore.getState().activeSession?.id).toBe(
      liveSession.id,
    )
  })

  it("remembers the language and meeting type for the next meeting", async () => {
    respond([installed])
    const fallback = invokeMock.getMockImplementation()!
    invokeMock.mockImplementation(async (command, args) =>
      command === "update_settings"
        ? { ...settings, revision: 1, defaultSessionLanguage: "pt-BR" }
        : fallback(command, args),
    )
    const user = userEvent.setup()
    renderHome()

    await user.selectOptions(
      await screen.findByRole("combobox", { name: "Spoken language" }),
      "pt-BR",
    )
    const start = screen.getByRole("button", { name: "Start meeting" })
    await waitFor(() => expect(start).toBeEnabled())
    await user.click(start)

    await waitFor(() => expect(argsOf("update_settings")).toBeDefined())
    expect(JSON.stringify(argsOf("update_settings"))).toContain(
      '"defaultSessionLanguage":"pt-BR"',
    )
    expect(JSON.stringify(argsOf("update_settings"))).toContain(
      `"defaultPresetId":"${presetFixture.id}"`,
    )
  })

  it("offers the language chosen last time", async () => {
    respond([installed])
    const fallback = invokeMock.getMockImplementation()!
    invokeMock.mockImplementation(async (command, args) =>
      command === "get_settings"
        ? { ...settings, defaultSessionLanguage: "pt-BR" }
        : fallback(command, args),
    )
    renderHome()

    await waitFor(() =>
      expect(
        screen.getByRole("combobox", { name: "Spoken language" }),
      ).toHaveValue("pt-BR"),
    )
  })

  it("will not start before a transcription model is installed, and says why", async () => {
    respond([modelFixture.installation])
    renderHome()

    expect(
      await screen.findByText(/download a transcription model in settings/i),
    ).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Start meeting" })).toBeDisabled()
    expect(screen.getByRole("region", { name: "Setup" })).toHaveTextContent(
      "Transcription model",
    )
  })

  it("offers to return to a meeting that is already running", async () => {
    respond([installed])
    useActiveSessionStore.setState({
      activeSession: {
        id: liveSession.id as never,
        projectId: liveSession.projectId as never,
        title: liveSession.title,
        state: "transcribing",
        revision: liveSession.revision,
      },
    })
    const user = userEvent.setup()
    renderHome()

    expect(
      screen.queryByRole("button", { name: "Start meeting" }),
    ).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: /return to meeting/i }))
    expect(useNavigationStore.getState().activeRoute).toBe("meeting")
  })
})
