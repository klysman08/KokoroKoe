import { describe, expect, it } from "vitest"

import startFixture from "../../fixtures/contracts/audio-prototype-start-v1.json"
import statusFixture from "../../fixtures/contracts/audio-prototype-status-v1.json"
import {
  audioDeviceSchema,
  audioPrototypeStartRequestSchema,
  audioPrototypeStatusSchema,
  deviceSelectionSchema,
} from "./audio"

describe("audio prototype contracts", () => {
  it("parses the shared start fixture", () => {
    expect(audioPrototypeStartRequestSchema.parse(startFixture)).toEqual(
      startFixture,
    )
  })

  it("parses the shared normalized-processing status fixture", () => {
    expect(audioPrototypeStatusSchema.parse(statusFixture)).toEqual(
      statusFixture,
    )
  })

  it.each([
    [{ ...startFixture, acknowledgedCaptureConsent: false }],
    [{ ...startFixture, queueCapacityPacketsPerSource: 3 }],
    [{ ...startFixture, queueCapacityPacketsPerSource: 257 }],
    [{ ...startFixture, extra: true }],
  ])("rejects unsafe or unknown start input", (candidate) => {
    expect(audioPrototypeStartRequestSchema.safeParse(candidate).success).toBe(
      false,
    )
  })

  it.each([
    [{ kind: "fixed", endpointId: "" }],
    [{ kind: "fixed", endpointId: "bad\nendpoint" }],
    [{ kind: "fixed", endpointId: "x".repeat(1025) }],
    [{ kind: "default", role: "Console" }],
    [{ kind: "default", role: "console", endpointId: "unexpected" }],
  ])("rejects malformed or ambiguous device selections", (selection) => {
    expect(deviceSelectionSchema.safeParse(selection).success).toBe(false)
  })

  it("rejects control characters in returned endpoint metadata", () => {
    expect(
      audioDeviceSchema.safeParse({
        endpointId: "synthetic-endpoint",
        friendlyName: "bad\ndevice name",
        direction: "input",
        isDefaultConsole: true,
        isDefaultMultimedia: false,
        isDefaultCommunications: false,
        nativeFormat: null,
      }).success,
    ).toBe(false)
  })

  it("rejects non-finite, out-of-range, and unknown level diagnostics", () => {
    expect(
      audioPrototypeStatusSchema.safeParse({
        ...statusFixture,
        microphone: {
          ...statusFixture.microphone,
          latestLevel: {
            ...statusFixture.microphone.latestLevel,
            peakDbfs: Number.NaN,
          },
        },
      }).success,
    ).toBe(false)
    expect(
      audioPrototypeStatusSchema.safeParse({
        ...statusFixture,
        systemOutput: {
          ...statusFixture.systemOutput,
          latestLevel: {
            rmsDbfs: -121,
            peakDbfs: -30,
            clipping: false,
            atMs: 100,
          },
        },
      }).success,
    ).toBe(false)
    expect(
      audioPrototypeStatusSchema.safeParse({
        ...statusFixture,
        unexpectedAudio: true,
      }).success,
    ).toBe(false)
  })
})
