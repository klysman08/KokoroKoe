import { invoke } from "@tauri-apps/api/core"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { closeTranscriptWindow, openTranscriptWindow } from "./windows"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

describe("transcript window Tauri adapter", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
  })

  it("sends no label, url, path, or dimension to Rust", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    await openTranscriptWindow()
    await closeTranscriptWindow()

    expect(vi.mocked(invoke).mock.calls).toEqual([
      ["open_transcript_window"],
      ["close_transcript_window"],
    ])
  })

  it("surfaces sanitized backend failures", async () => {
    vi.mocked(invoke).mockRejectedValue({
      error: {
        code: "window_open_failed",
        userMessage: "KokoroKoe could not open the transcript window.",
        technicalDetail: "window_open_failed",
        severity: "error",
        retryable: true,
        correlationId: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
      },
    })

    await expect(openTranscriptWindow()).rejects.toMatchObject({
      details: { code: "window_open_failed" },
    })
  })
})
