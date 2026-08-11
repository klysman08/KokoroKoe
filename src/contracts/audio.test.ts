import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/audio-device-test-v1.json"
import {
  audioDeviceListSchema,
  audioDeviceSchema,
  audioLevelUpdatedEnvelopeSchema,
  deviceSelectionSchema,
  deviceTestStatusSchema,
  productAudioFixtureSchema,
} from "./audio"

describe("product audio contracts", () => {
  it("parses the shared device, test, and event fixture", () => {
    expect(productAudioFixtureSchema.parse(fixture)).toEqual(fixture)
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

  it("rejects control characters and inconsistent device directions", () => {
    expect(
      audioDeviceSchema.safeParse({
        ...fixture.deviceList.inputs[0],
        friendlyName: "bad\ndevice name",
      }).success,
    ).toBe(false)
    expect(
      audioDeviceListSchema.safeParse({
        inputs: [{ ...fixture.deviceList.inputs[0], direction: "output" }],
        outputs: [],
      }).success,
    ).toBe(false)
  })

  it("rejects invalid status/error combinations and source directions", () => {
    expect(
      deviceTestStatusSchema.safeParse({
        ...fixture.status,
        status: "failed",
      }).success,
    ).toBe(false)
    expect(
      deviceTestStatusSchema.safeParse({
        ...fixture.status,
        source: "system_output",
      }).success,
    ).toBe(false)
  })

  it("rejects non-finite levels, request mismatches, and unknown sample data", () => {
    expect(
      audioLevelUpdatedEnvelopeSchema.safeParse({
        ...fixture.levelEvent,
        payload: { ...fixture.levelEvent.payload, peakDbfs: Number.NaN },
      }).success,
    ).toBe(false)
    expect(
      audioLevelUpdatedEnvelopeSchema.safeParse({
        ...fixture.levelEvent,
        payload: {
          ...fixture.levelEvent.payload,
          testId: "5c188d9d-b772-48da-b4ec-b8f89d362a57",
        },
      }).success,
    ).toBe(false)
    expect(
      audioLevelUpdatedEnvelopeSchema.safeParse({
        ...fixture.levelEvent,
        payload: { ...fixture.levelEvent.payload, samples: [] },
      }).success,
    ).toBe(false)
  })
})
