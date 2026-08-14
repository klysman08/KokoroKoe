import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import appSettingsFixture from "../../../fixtures/contracts/app-settings-v1.json"
import commandErrorFixture from "../../../fixtures/contracts/command-error-v1.json"
import workspaceStatusFixture from "../../../fixtures/contracts/workspace-status-v1.json"
import modelFixture from "../../../fixtures/contracts/model-management-v1.json"
import openRouterModelFixture from "../../../fixtures/contracts/openrouter-model-v1.json"

import { SettingsPage } from "@/features/settings/SettingsPage"

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}))
vi.mock("@/features/settings/AudioDeviceSettings", () => ({
  AudioDeviceSettings: () => null,
}))
vi.mock("@/features/settings/OpenRouterCredentialCard", () => ({
  OpenRouterCredentialCard: () => null,
}))

const invokeMock = vi.mocked(invoke)
const conflictError = {
  error: {
    code: "settings_revision_conflict",
    userMessage:
      "Settings changed since this screen was loaded. Reload and try again.",
    technicalDetail:
      "The expected settings revision did not match the stored revision.",
    severity: "warning",
    retryable: false,
    correlationId: "00000000-0000-4000-8000-000000000099",
  },
} as const
const cancellationError = {
  error: {
    code: "workspace_selection_cancelled",
    userMessage: "Workspace selection was cancelled.",
    technicalDetail: "The native folder picker closed without a selection.",
    severity: "info",
    retryable: false,
    correlationId: "00000000-0000-4000-8000-000000000098",
  },
} as const

describe("SettingsPage", () => {
  beforeEach(() => {
    invokeMock.mockReset()
  })

  it("shows a sanitized error and retries one retryable local failure", async () => {
    invokeMock.mockRejectedValue(commandErrorFixture)
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retryDelay: 0 } },
    })

    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /could not load the foundation settings/i,
    )
    await waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(2))
    expect(screen.getByText("Local SQLite settings")).toBeInTheDocument()
    expect(
      screen.getByRole("button", { name: /choose workspace folder/i }),
    ).toBeEnabled()
  })

  it("selects a workspace without sending a path from React", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_transcription_models") return []
      if (command === "choose_workspace") return workspaceStatusFixture
      return appSettingsFixture
    })
    const queryClient = new QueryClient()

    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )

    await screen.findByText(appSettingsFixture.workspacePath)
    await user.click(
      screen.getByRole("button", { name: /choose workspace folder/i }),
    )
    expect(
      await screen.findByText(/workspace is writable and saved/i),
    ).toBeInTheDocument()
    expect(invokeMock).toHaveBeenCalledWith("choose_workspace")
    expect(
      screen.getByText(/responsible for obtaining any consent/i),
    ).toBeInTheDocument()
  })

  it("requires confirmation before enabling retained audio and saves by revision", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_transcription_models") return []
      if (command === "update_settings") {
        return {
          ...appSettingsFixture,
          revision: 1,
          retainAudioByDefault: true,
        }
      }
      return appSettingsFixture
    })
    const queryClient = new QueryClient()

    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )

    const retention = await screen.findByRole("checkbox", {
      name: /retain original session audio/i,
    })
    await user.click(retention)
    const save = screen.getByRole("button", { name: /save settings/i })
    expect(save).toBeDisabled()
    await user.click(
      screen.getByRole("checkbox", {
        name: /understand retained audio increases/i,
      }),
    )
    expect(save).toBeEnabled()
    await user.click(save)

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_settings", {
        expectedRevision: 0,
        value: { retainAudioByDefault: true },
      }),
    )
  })

  it("selects a cached privacy-filtered role model and saves it by revision", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_transcription_models") return []
      if (command === "update_settings") {
        return {
          ...appSettingsFixture,
          revision: 1,
          defaultLlmModels: { insights: openRouterModelFixture.id },
        }
      }
      return appSettingsFixture
    })
    const queryClient = new QueryClient()
    queryClient.setQueryData(["openrouter-models"], [openRouterModelFixture])

    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )

    const selector = await screen.findByRole("combobox", {
      name: "Fast insights model",
    })
    await user.click(selector)
    await user.click(
      screen.getByRole("option", { name: /Example Text Model · example/i }),
    )
    await user.click(screen.getByRole("button", { name: /save settings/i }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_settings", {
        expectedRevision: 0,
        value: {
          defaultLlmModels: { insights: openRouterModelFixture.id },
        },
      }),
    )
  })

  it("keeps a conflicting draft visible after the authoritative refetch", async () => {
    const user = userEvent.setup()
    let getCount = 0
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_transcription_models") return []
      if (command === "update_settings") throw conflictError
      if (command === "get_settings") {
        getCount += 1
        return getCount === 1
          ? appSettingsFixture
          : { ...appSettingsFixture, revision: 1 }
      }
      return workspaceStatusFixture
    })
    const queryClient = new QueryClient()

    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )

    const retention = await screen.findByRole("checkbox", {
      name: /retain original session audio/i,
    })
    await user.click(retention)
    await user.click(
      screen.getByRole("checkbox", {
        name: /understand retained audio increases/i,
      }),
    )
    await user.click(screen.getByRole("button", { name: /save settings/i }))

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /settings changed since this screen was loaded/i,
    )
    await waitFor(() => expect(getCount).toBe(2))
    expect(retention).toBeChecked()
    expect(screen.getByText("Revision 1")).toBeInTheDocument()
    expect(
      invokeMock.mock.calls.filter(
        ([command]) => command === "update_settings",
      ),
    ).toHaveLength(1)
  })

  it("treats native picker cancellation as an unchanged workspace", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_transcription_models") return []
      if (command === "choose_workspace") throw cancellationError
      return appSettingsFixture
    })
    const queryClient = new QueryClient()

    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )
    await screen.findByText(appSettingsFixture.workspacePath)
    await user.click(
      screen.getByRole("button", { name: /choose workspace folder/i }),
    )

    expect(
      await screen.findByText(/folder selection was cancelled/i),
    ).toBeInTheDocument()
    expect(screen.queryByRole("alert")).not.toBeInTheDocument()
    expect(
      screen.getByText(appSettingsFixture.workspacePath),
    ).toBeInTheDocument()
  })

  it("shows bounded model progress and never renders Rust-owned download metadata", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_transcription_models")
        return [modelFixture.installation]
      if (command === "cancel_model_download") {
        return {
          ...modelFixture.installation.downloadJob,
          status: "cancelled",
          resumable: true,
        }
      }
      return appSettingsFixture
    })
    const queryClient = new QueryClient()
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )

    expect(
      await screen.findByText(modelFixture.installation.descriptor.name),
    ).toBeInTheDocument()
    expect(
      screen.getByRole("progressbar", { name: /whisper tiny/i }),
    ).toHaveAttribute("aria-valuenow", "5")
    expect(
      screen.queryByText(modelFixture.installation.descriptor.sourceUrl),
    ).not.toBeInTheDocument()
    expect(
      screen.queryByText(modelFixture.installation.descriptor.sha256),
    ).not.toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: /cancel download/i }))
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("cancel_model_download", {
        requestId: modelFixture.installation.downloadJob.requestId,
      }),
    )
  })
})
