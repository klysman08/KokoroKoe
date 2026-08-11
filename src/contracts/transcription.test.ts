import fixture from "../../fixtures/contracts/live-transcription-v1.json"

import {
  liveTranscriptionFixtureSchema,
  transcriptionFinalEnvelopeSchema,
  transcriptionPartialEnvelopeSchema,
} from "@/contracts/transcription"

describe("live transcription contract", () => {
  it("parses the shared fixture and stable replacement identity", () => {
    const parsed = liveTranscriptionFixtureSchema.parse(fixture)
    expect(parsed.finalEvent.payload.segment.id).toBe(
      parsed.partialEvent.payload.segment.id,
    )
    expect(parsed.finalEvent.payload.replacesPartialId).toBe(
      parsed.partialEvent.payload.segment.id,
    )
  })

  it("rejects unknown, nullable, unsafe, and inconsistent event values", () => {
    expect(
      transcriptionPartialEnvelopeSchema.safeParse({
        ...fixture.partialEvent,
        samples: [],
      }).success,
    ).toBe(false)
    expect(
      transcriptionPartialEnvelopeSchema.safeParse({
        ...fixture.partialEvent,
        requestId: null,
      }).success,
    ).toBe(false)
    expect(
      transcriptionPartialEnvelopeSchema.safeParse({
        ...fixture.partialEvent,
        sessionSequence: Number.MAX_SAFE_INTEGER + 1,
      }).success,
    ).toBe(false)
    expect(
      transcriptionFinalEnvelopeSchema.safeParse({
        ...fixture.finalEvent,
        payload: {
          ...fixture.finalEvent.payload,
          replacesPartialId: "34ad1c4d-3c82-4f4e-aa7c-e555b667f2cd",
        },
      }).success,
    ).toBe(false)
  })
})
