import { describe, expect, it } from "vitest"

import fixture from "../../fixtures/contracts/transcript-reading-v1.json"
import {
  transcriptReadingFixtureSchema,
  transcriptSearchRequestSchema,
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
