import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import fixture from "../../../fixtures/contracts/audio-device-test-v1.json"

import { productAudioFixtureSchema } from "@/contracts/audio"
import { AudioDeviceSettings } from "@/features/settings/AudioDeviceSettings"
import {
  listAudioDevices,
  listenToAudioDeviceStatus,
  listenToAudioLevels,
  startAudioDeviceTest,
  stopAudioDeviceTest,
} from "@/lib/tauri/audio"

vi.mock("@/lib/tauri/audio", () => ({
  listAudioDevices: vi.fn(),
  listenToAudioDeviceStatus: vi.fn(),
  listenToAudioLevels: vi.fn(),
  startAudioDeviceTest: vi.fn(),
  stopAudioDeviceTest: vi.fn(),
}))

const listMock = vi.mocked(listAudioDevices)
const levelsMock = vi.mocked(listenToAudioLevels)
const healthMock = vi.mocked(listenToAudioDeviceStatus)
const startMock = vi.mocked(startAudioDeviceTest)
const stopMock = vi.mocked(stopAudioDeviceTest)
const typedFixture = productAudioFixtureSchema.parse(fixture)

describe("AudioDeviceSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    listMock.mockResolvedValue(typedFixture.deviceList)
    levelsMock.mockResolvedValue(() => undefined)
    healthMock.mockResolvedValue(() => undefined)
    startMock.mockImplementation(async (input, requestId) => ({
      ...typedFixture.status,
      requestId,
      source: input.source,
      status: "starting",
      device:
        input.source === "microphone"
          ? typedFixture.deviceList.inputs[0]
          : typedFixture.deviceList.outputs[0],
    }))
    stopMock.mockImplementation(async (requestId) => ({
      ...typedFixture.status,
      requestId,
      status: "stopped",
    }))
  })

  it("lists devices and starts independent source-local tests", async () => {
    const user = userEvent.setup()
    renderAudioSettings()

    expect(await screen.findByText("Synthetic microphone")).toBeInTheDocument()
    expect(screen.getByText("Synthetic output")).toBeInTheDocument()
    expect(
      screen.getByText(/choices are temporary until a meeting session/i),
    ).toBeInTheDocument()

    await user.selectOptions(
      screen.getByLabelText("Microphone input device"),
      "fixed:synthetic-microphone",
    )
    await user.click(screen.getByRole("button", { name: "Test input" }))
    await user.click(screen.getByRole("button", { name: "Test output" }))

    await waitFor(() => expect(startMock).toHaveBeenCalledTimes(2))
    expect(startMock.mock.calls[0]?.[0]).toEqual({
      source: "microphone",
      selection: { kind: "fixed", endpointId: "synthetic-microphone" },
    })
    expect(startMock.mock.calls[1]?.[0]).toEqual({
      source: "system_output",
      selection: { kind: "default", role: "console" },
    })
    expect(
      screen.getByRole("button", { name: "Stop input test" }),
    ).toBeInTheDocument()
    expect(
      screen.getByRole("button", { name: "Stop output test" }),
    ).toBeInTheDocument()
  })

  it("reconciles matching aggregate events and stops by request identity", async () => {
    const user = userEvent.setup()
    let onLevel: Parameters<typeof listenToAudioLevels>[0] | undefined
    let onHealth: Parameters<typeof listenToAudioDeviceStatus>[0] | undefined
    levelsMock.mockImplementation(async (callback) => {
      onLevel = callback
      return () => undefined
    })
    healthMock.mockImplementation(async (callback) => {
      onHealth = callback
      return () => undefined
    })
    renderAudioSettings()
    await screen.findByText("Synthetic microphone")
    await user.click(screen.getByRole("button", { name: "Test input" }))
    await waitFor(() => expect(startMock).toHaveBeenCalledOnce())
    const requestId = startMock.mock.calls[0]?.[1]
    expect(requestId).toBeDefined()

    onLevel?.({
      ...fixture.levelEvent,
      requestId: requestId!,
      payload: {
        ...fixture.levelEvent.payload,
        testId: requestId!,
        peakDbfs: -6.5,
      },
    } as never)
    onHealth?.({
      ...fixture.healthEvent,
      requestId: requestId!,
      payload: {
        ...fixture.healthEvent.payload,
        current: { ...fixture.healthEvent.payload.current, status: "active" },
      },
    } as never)
    expect(await screen.findByText("Peak -6.5 dBFS")).toBeInTheDocument()
    expect(screen.getByText("active")).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: "Stop input test" }))
    await waitFor(() => expect(stopMock).toHaveBeenCalledWith(requestId))
  })
})

function renderAudioSettings() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={client}>
      <AudioDeviceSettings />
    </QueryClientProvider>,
  )
}
