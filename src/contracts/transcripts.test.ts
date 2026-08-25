import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/transcript-reading-v1.json"
import {
  transcriptReadingFixtureSchema,
  transcriptSearchRequestSchema,
  transcriptSegmentHistorySchema,
} from "./transcripts"

describe("transcript contracts", () => {
  it("parses the shared strict Rust fixture", () => {
    expect(transcriptReadingFixtureSchema.parse(fixture)).toEqual(fixture)
  })

  it("rejects unsafe, oversized, and over-broad search input", () => {
    for (const query of [
      "line\nbreak",
      "x".repeat(257),
      Array.from({ length: 17 }, (_, index) => `term${index}`).join(" "),
    ]) {
      expect(
        transcriptSearchRequestSchema.safeParse({
          projectId: fixture.searchRequest.projectId,
          sessionId: fixture.searchRequest.sessionId,
          query,
          limit: 20,
        }).success,
      ).toBe(false)
    }
  })
})

describe("transcript segment history contract", () => {
  /// The history is rendered as inert text beside the transcript, so it obeys
  /// the same bounds a transcription does and carries nothing else.
  it("rejects an unbounded, unusable, or over-described history", () => {
    for (const invalid of [
      { revisions: [{ recordedAt: "2026-08-12", text: "No offset." }] },
      { revisions: [{ recordedAt: "2026-08-12T10:04:00Z", text: "" }] },
      {
        revisions: Array.from({ length: 65 }, () => ({
          recordedAt: "2026-08-12T10:04:00Z",
          text: "Too many.",
        })),
      },
      { originalText: "" },
      { unknownField: true },
    ]) {
      expect(
        transcriptSegmentHistorySchema.safeParse({
          ...fixture.history,
          ...invalid,
        }).success,
      ).toBe(false)
    }
  })

  it("accepts a segment that has never been corrected", () => {
    expect(
      transcriptSegmentHistorySchema.safeParse({
        ...fixture.history,
        revisions: [],
      }).success,
    ).toBe(true)
  })
})
