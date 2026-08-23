import { invoke } from "@tauri-apps/api/core"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
  closeTranscriptWindow,
  getTranscriptWindowInteraction,
  openTranscriptWindow,
  setTranscriptWindowInteraction,
} from "./windows"

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

  it("sends only the click-through flag and parses the strict reply", async () => {
    vi.mocked(invoke).mockResolvedValue({
      schemaVersion: 1,
      clickThrough: true,
    })

    await expect(
      setTranscriptWindowInteraction({ clickThrough: true }),
    ).resolves.toEqual({ schemaVersion: 1, clickThrough: true })
    expect(vi.mocked(invoke)).toHaveBeenCalledWith(
      "set_transcript_window_interaction",
      { request: { clickThrough: true } },
    )
  })

  /// A reply that does not match the contract must not be shown as state; the
  /// user would then think the mouse is captured when it is not.
  it("rejects an interaction reply that is off contract", async () => {
    vi.mocked(invoke).mockResolvedValue({
      schemaVersion: 1,
      clickThrough: "yes",
    })

    await expect(getTranscriptWindowInteraction()).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
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
