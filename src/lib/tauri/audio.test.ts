import { invoke } from "@tauri-apps/api/core"
import { vi } from "vitest"

import commandErrorFixture from "../../../fixtures/contracts/command-error-v1.json"
import startFixture from "../../../fixtures/contracts/audio-prototype-start-v1.json"

import { audioPrototypeStartRequestSchema } from "@/contracts/audio"
import {
  getAudioCapturePrototypeStatus,
  listAudioDevices,
  startAudioCapturePrototype,
  stopAudioCapturePrototype,
} from "@/lib/tauri/audio"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const validStart = audioPrototypeStartRequestSchema.parse(startFixture)
const nativeFormat = {
  sampleRate: 48_000,
  channels: 2,
  bitsPerSample: 32,
  validBitsPerSample: 32,
  blockAlign: 8,
  channelMask: 3,
  sampleType: "float" as const,
}
const channel = {
  status: "active" as const,
  endpointId: "synthetic-endpoint",
  nativeFormat,
  captureAttempts: 1,
  packetsCaptured: 2,
  framesCaptured: 960,
  packetsConsumed: 2,
  framesConsumed: 960,
  bytesConsumed: 7680,
  queueDrops: 0,
  dataDiscontinuities: 0,
  timestampErrors: 0,
  timestampRegressions: 0,
  firstPacketMs: 10,
  lastPacketMs: 20,
  lastErrorCode: null,
}
const validStatus = {
  state: "capturing" as const,
  elapsedMs: 25,
  queueCapacityPacketsPerSource: 64,
  microphone: { ...channel, source: "microphone" as const },
  systemOutput: { ...channel, source: "system_output" as const },
}

describe("audio prototype Tauri adapter", () => {
  beforeEach(() => invokeMock.mockReset())

  it("lists devices through the exact command and validates diagnostics", async () => {
    const devices = {
      inputs: [
        {
          endpointId: "synthetic-input",
          friendlyName: "Synthetic microphone",
          direction: "input",
          isDefaultConsole: true,
          isDefaultMultimedia: true,
          isDefaultCommunications: false,
          nativeFormat,
        },
      ],
      outputs: [],
    }
    invokeMock.mockResolvedValue(devices)

    await expect(listAudioDevices()).resolves.toEqual(devices)
    expect(invokeMock).toHaveBeenCalledWith("list_audio_devices", undefined)
  })

  it("requires a valid explicit-consent request before invoking Rust", async () => {
    await expect(
      startAudioCapturePrototype({
        ...validStart,
        acknowledgedCaptureConsent: false,
      } as never),
    ).rejects.toMatchObject({ details: { code: "invalid_request_contract" } })
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it("starts, reads, and stops only through the prototype commands", async () => {
    invokeMock.mockResolvedValue(validStatus)

    await expect(startAudioCapturePrototype(validStart)).resolves.toEqual(
      validStatus,
    )
    expect(invokeMock).toHaveBeenCalledWith("start_audio_capture_prototype", {
      request: validStart,
    })
    await expect(getAudioCapturePrototypeStatus()).resolves.toEqual(validStatus)
    await expect(stopAudioCapturePrototype()).resolves.toEqual(validStatus)
  })

  it("normalizes command rejection and malformed success data", async () => {
    invokeMock.mockRejectedValueOnce(commandErrorFixture)
    await expect(listAudioDevices()).rejects.toMatchObject({
      details: { code: commandErrorFixture.error.code },
    })

    invokeMock.mockResolvedValueOnce({ rawAudio: "secret-canary" })
    await expect(getAudioCapturePrototypeStatus()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })
})
