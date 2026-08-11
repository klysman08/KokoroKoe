import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { vi } from "vitest"

import fixture from "../../../fixtures/contracts/audio-device-test-v1.json"
import commandErrorFixture from "../../../fixtures/contracts/command-error-v1.json"

import { deviceTestInputSchema } from "@/contracts/audio"
import { requestIdSchema } from "@/contracts/models"
import {
  AUDIO_DEVICE_STATUS_CHANGED_EVENT,
  AUDIO_LEVEL_UPDATED_EVENT,
  listAudioDevices,
  listenToAudioDeviceStatus,
  listenToAudioLevels,
  startAudioDeviceTest,
  stopAudioDeviceTest,
} from "@/lib/tauri/audio"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const listenMock = vi.mocked(listen)
const requestId = requestIdSchema.parse(fixture.status.requestId)
const validInput = deviceTestInputSchema.parse(fixture.input)

describe("audio device Tauri adapter", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    listenMock.mockReset()
  })

  it("lists devices through the exact command", async () => {
    invokeMock.mockResolvedValue(fixture.deviceList)
    await expect(listAudioDevices()).resolves.toEqual(fixture.deviceList)
    expect(invokeMock).toHaveBeenCalledWith("list_audio_devices", undefined)
  })

  it("starts and stops through the exact product commands", async () => {
    invokeMock.mockResolvedValueOnce({ ...fixture.status, status: "starting" })
    await expect(startAudioDeviceTest(validInput, requestId)).resolves.toEqual({
      ...fixture.status,
      status: "starting",
    })
    expect(invokeMock).toHaveBeenCalledWith("start_audio_device_test", {
      input: validInput,
      requestId,
    })

    invokeMock.mockResolvedValueOnce({ ...fixture.status, status: "stopped" })
    await expect(stopAudioDeviceTest(requestId)).resolves.toMatchObject({
      status: "stopped",
    })
    expect(invokeMock).toHaveBeenLastCalledWith("stop_audio_device_test", {
      requestId,
    })
  })

  it("rejects invalid requests and mismatched responses", async () => {
    await expect(
      startAudioDeviceTest(
        { ...validInput, selection: { kind: "fixed", endpointId: "" } },
        requestId,
      ),
    ).rejects.toMatchObject({ details: { code: "invalid_request_contract" } })
    expect(invokeMock).not.toHaveBeenCalled()

    invokeMock.mockResolvedValue({
      ...fixture.status,
      requestId: "5c188d9d-b772-48da-b4ec-b8f89d362a57",
    })
    await expect(
      startAudioDeviceTest(validInput, requestId),
    ).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })

  it("normalizes command rejection and malformed success data", async () => {
    invokeMock.mockRejectedValueOnce(commandErrorFixture)
    await expect(listAudioDevices()).rejects.toMatchObject({
      details: { code: commandErrorFixture.error.code },
    })
    invokeMock.mockResolvedValueOnce({ rawAudio: "secret-canary" })
    await expect(listAudioDevices()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })

  it("delivers only strictly validated aggregate audio events", async () => {
    const callbacks = new Map<string, (event: { payload: unknown }) => void>()
    listenMock.mockImplementation(async (name, callback) => {
      callbacks.set(name, callback as (event: { payload: unknown }) => void)
      return () => undefined
    })
    const levels = vi.fn()
    const health = vi.fn()
    await listenToAudioLevels(levels)
    await listenToAudioDeviceStatus(health)

    callbacks.get(AUDIO_LEVEL_UPDATED_EVENT)?.({ payload: fixture.levelEvent })
    callbacks.get(AUDIO_DEVICE_STATUS_CHANGED_EVENT)?.({
      payload: fixture.healthEvent,
    })
    callbacks.get(AUDIO_LEVEL_UPDATED_EVENT)?.({ payload: { samples: [1] } })

    expect(levels).toHaveBeenCalledOnce()
    expect(health).toHaveBeenCalledOnce()
  })
})
