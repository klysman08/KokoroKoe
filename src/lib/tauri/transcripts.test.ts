import { invoke } from "@tauri-apps/api/core"
import { beforeEach, describe, expect, it, vi } from "vitest"

import fixture from "../../../fixtures/contracts/transcript-reading-v1.json"
import { transcriptReadingFixtureSchema } from "@/contracts/transcripts"
import {
  getTranscriptPage,
  getTranscriptSegmentHistory,
  searchTranscript,
} from "./transcripts"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const contracts = transcriptReadingFixtureSchema.parse(fixture)

describe("transcript Tauri adapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset())

  it("invokes only the exact bounded transcript commands", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(contracts.page)
      .mockResolvedValueOnce(contracts.searchPage)

    await expect(getTranscriptPage(contracts.pageRequest)).resolves.toEqual(
      contracts.page,
    )
    await expect(searchTranscript(contracts.searchRequest)).resolves.toEqual(
      contracts.searchPage,
    )
    expect(invoke).toHaveBeenNthCalledWith(
      1,
      "get_transcript_page",
      contracts.pageRequest,
    )
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "search_transcript",
      contracts.searchRequest,
    )
  })

  it("rejects malformed success data", async () => {
    vi.mocked(invoke).mockResolvedValue({ items: [{ text: "unsafe" }] })
    await expect(
      getTranscriptPage(contracts.pageRequest),
    ).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })
})

describe("transcript segment history adapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset())

  /// A history read carries three identifiers and nothing else: no workspace
  /// path, no cursor, and none of the transcript text it is about to read.
  it("sends only the bounded identifiers and returns the validated history", async () => {
    vi.mocked(invoke).mockResolvedValue(contracts.history)

    await expect(
      getTranscriptSegmentHistory(contracts.historyRequest),
    ).resolves.toEqual(contracts.history)

    expect(invoke).toHaveBeenCalledWith("get_transcript_segment_history", {
      request: contracts.historyRequest,
    })
    expect(JSON.stringify(vi.mocked(invoke).mock.calls[0])).not.toContain(
      contracts.history.originalText,
    )
  })

  /// A history for a different segment is a contract violation, not a result
  /// to render beneath the segment the user is looking at.
  it("refuses a history that does not answer the segment that was asked about", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...contracts.history,
      segmentId: "44444444-4444-4444-8444-444444444444",
    })

    await expect(
      getTranscriptSegmentHistory(contracts.historyRequest),
    ).rejects.toMatchObject({ details: { code: "invalid_backend_contract" } })
  })

  it("rejects a revision with an unusable timestamp or wording", async () => {
    for (const revision of [
      { recordedAt: "not a time", text: "Fine." },
      { recordedAt: "2026-08-12T10:04:00Z", text: "carriage\rreturn" },
    ]) {
      vi.mocked(invoke).mockResolvedValue({
        ...contracts.history,
        revisions: [revision],
      })
      await expect(
        getTranscriptSegmentHistory(contracts.historyRequest),
      ).rejects.toMatchObject({ details: { code: "invalid_backend_contract" } })
    }
  })
})
