import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { act, renderHook, waitFor } from "@testing-library/react"
import { type PropsWithChildren } from "react"
import { vi } from "vitest"

import appSettingsFixture from "../../../fixtures/contracts/app-settings-v1.json"
import workspaceStatusFixture from "../../../fixtures/contracts/workspace-status-v1.json"

import { appSettingsSchema } from "@/contracts/settings"
import {
  useChooseWorkspaceMutation,
  useUpdateSettingsMutation,
} from "@/features/settings/use-settings-mutations"
import { settingsQueryKey } from "@/features/settings/use-settings-query"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const initialSettings = appSettingsSchema.parse(appSettingsFixture)

function setup() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  client.setQueryData(settingsQueryKey, initialSettings)
  const wrapper = ({ children }: PropsWithChildren) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
  return { client, wrapper }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((resolver) => {
    resolve = resolver
  })
  return { promise, resolve }
}

describe("settings mutations", () => {
  beforeEach(() => invokeMock.mockReset())

  it("shows an optimistic patch without inventing a revision, then accepts Rust authority", async () => {
    const response = deferred<unknown>()
    const authoritative = {
      ...appSettingsFixture,
      revision: 1,
      retainAudioByDefault: true,
    }
    invokeMock.mockImplementation((command) =>
      command === "update_settings"
        ? response.promise
        : Promise.resolve(authoritative),
    )
    const { client, wrapper } = setup()
    const { result } = renderHook(() => useUpdateSettingsMutation(), {
      wrapper,
    })

    act(() =>
      result.current.mutate({
        expectedRevision: 0,
        value: { retainAudioByDefault: true },
      }),
    )
    await waitFor(() =>
      expect(client.getQueryData(settingsQueryKey)).toMatchObject({
        revision: 0,
        retainAudioByDefault: true,
      }),
    )
    response.resolve(authoritative)
    await waitFor(() =>
      expect(client.getQueryData(settingsQueryKey)).toMatchObject({
        revision: 1,
        retainAudioByDefault: true,
      }),
    )
  })

  it("does not optimistically change a workspace path", async () => {
    const response = deferred<unknown>()
    const authoritative = {
      ...appSettingsFixture,
      revision: 1,
      workspacePath: workspaceStatusFixture.path,
    }
    invokeMock.mockImplementation((command) =>
      command === "choose_workspace"
        ? response.promise
        : Promise.resolve(authoritative),
    )
    const { client, wrapper } = setup()
    const { result } = renderHook(() => useChooseWorkspaceMutation(), {
      wrapper,
    })

    act(() => result.current.mutate())
    expect(client.getQueryData(settingsQueryKey)).toMatchObject({
      workspacePath: appSettingsFixture.workspacePath,
    })
    response.resolve(workspaceStatusFixture)
    await waitFor(() =>
      expect(client.getQueryData(settingsQueryKey)).toMatchObject({
        workspacePath: workspaceStatusFixture.path,
      }),
    )
    expect(invokeMock).toHaveBeenCalledWith("choose_workspace")
  })
})
