import { describe, expect, it } from "vitest"

import startFixture from "../../fixtures/contracts/audio-prototype-start-v1.json"
import {
  audioDeviceSchema,
  audioPrototypeStartRequestSchema,
  deviceSelectionSchema,
} from "./audio"

describe("audio prototype contracts", () => {
  it("parses the shared start fixture", () => {
    expect(audioPrototypeStartRequestSchema.parse(startFixture)).toEqual(
      startFixture,
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
})
