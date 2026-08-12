import { invoke } from "@tauri-apps/api/core"
import { beforeEach, describe, expect, it, vi } from "vitest"

import fixture from "../../../fixtures/contracts/transcript-reading-v1.json"
import { transcriptReadingFixtureSchema } from "@/contracts/transcripts"
import { getTranscriptPage, searchTranscript } from "./transcripts"

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
