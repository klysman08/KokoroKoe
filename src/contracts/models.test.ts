import fixture from "../../fixtures/contracts/model-management-v1.json"

import {
  modelDownloadProgressEnvelopeSchema,
  modelInstallationSchema,
} from "@/contracts/models"

describe("model management contracts", () => {
  it("matches the shared strict Rust fixture", () => {
    expect(modelInstallationSchema.parse(fixture.installation)).toEqual(
      fixture.installation,
    )
    expect(modelDownloadProgressEnvelopeSchema.parse(fixture.event)).toEqual(
      fixture.event,
    )
  })

  it("rejects unknown, nullable, and inconsistent values", () => {
    expect(
      modelInstallationSchema.safeParse({
        ...fixture.installation,
        unexpected: true,
      }).success,
    ).toBe(false)
    expect(
      modelInstallationSchema.safeParse({
        ...fixture.installation,
        installedAt: null,
      }).success,
    ).toBe(false)
    expect(
      modelInstallationSchema.safeParse({
        ...fixture.installation,
        installedBytes: 1,
      }).success,
    ).toBe(false)
    expect(
      modelDownloadProgressEnvelopeSchema.safeParse({
        ...fixture.event,
        requestId: crypto.randomUUID(),
      }).success,
    ).toBe(false)
  })
})
