import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/transcript-reading-v1.json"
import {
  transcriptReadingFixtureSchema,
  transcriptSearchRequestSchema,
  transcriptSegmentHistorySchema,
  segmentAnnotationSchema,
  transcriptBookmarkListSchema,
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

describe("bookmark contract", () => {
  /// A bookmark says "come back here". Removing one carries no name, because
  /// there is nothing left to name.
  it("refuses a name on a bookmark being removed, and an unusable name", () => {
    for (const annotation of [
      { kind: "bookmark", bookmarked: false, label: "orphaned name" },
      { kind: "bookmark", bookmarked: true, label: "" },
      { kind: "bookmark", bookmarked: true, label: "   " },
      { kind: "bookmark", bookmarked: true, label: "two\nlines" },
      { kind: "bookmark", bookmarked: true, label: "x".repeat(121) },
    ]) {
      expect(segmentAnnotationSchema.safeParse(annotation).success).toBe(false)
    }
  })

  it("accepts a named and an unnamed bookmark, and its removal", () => {
    for (const annotation of [
      { kind: "bookmark", bookmarked: true, label: "pricing decision" },
      { kind: "bookmark", bookmarked: true },
      { kind: "bookmark", bookmarked: false },
    ]) {
      expect(segmentAnnotationSchema.safeParse(annotation).success).toBe(true)
    }
  })

  /// The list is a way back into the transcript, so every item in it has to
  /// actually be a bookmark.
  it("refuses a list carrying a segment that is not bookmarked", () => {
    const bookmarked = {
      ...fixture.page.items[0],
      bookmarked: true,
      bookmarkLabel: "pricing decision",
    }
    expect(
      transcriptBookmarkListSchema.safeParse({ items: [bookmarked] }).success,
    ).toBe(true)
    expect(
      transcriptBookmarkListSchema.safeParse({
        items: [fixture.page.items[0]],
      }).success,
    ).toBe(false)
  })
})
