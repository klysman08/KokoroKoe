import { invoke } from "@tauri-apps/api/core"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { DETACHED_WINDOWS, type DetachedWindow } from "@/contracts/windows"
import {
  closeDetachedWindow,
  getDetachedWindowAppearance,
  getDetachedWindowInteraction,
  openDetachedWindow,
  setDetachedWindowInteraction,
} from "./windows"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

describe("detached window Tauri adapter", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
  })

  /// Naming a window is the only window identity the frontend sends: no label,
  /// URL, path, or dimension ever crosses.
  it("sends only the window name to Rust", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    for (const window of DETACHED_WINDOWS) {
      await openDetachedWindow(window)
      await closeDetachedWindow(window)
    }

    expect(vi.mocked(invoke).mock.calls).toEqual([
      ["open_detached_window", { request: { window: "transcript" } }],
      ["close_detached_window", { request: { window: "transcript" } }],
      ["open_detached_window", { request: { window: "insights" } }],
      ["close_detached_window", { request: { window: "insights" } }],
    ])
  })

  it("rejects a window the application does not own", async () => {
    await expect(
      openDetachedWindow("main" as unknown as DetachedWindow),
    ).rejects.toMatchObject({ details: { code: "invalid_request_contract" } })
    expect(vi.mocked(invoke)).not.toHaveBeenCalled()
  })

  it("sends only the click-through flag and parses the strict reply", async () => {
    vi.mocked(invoke).mockResolvedValue({
      window: "insights",
      schemaVersion: 1,
      clickThrough: true,
    })

    await expect(
      setDetachedWindowInteraction({ window: "insights", clickThrough: true }),
    ).resolves.toEqual({
      window: "insights",
      schemaVersion: 1,
      clickThrough: true,
    })
    expect(vi.mocked(invoke)).toHaveBeenCalledWith(
      "set_detached_window_interaction",
      { request: { window: "insights", clickThrough: true } },
    )
  })

  /// A reply that does not match the contract must not be shown as state; the
  /// user would then think the mouse is captured when it is not.
  it("rejects an interaction reply that is off contract", async () => {
    vi.mocked(invoke).mockResolvedValue({
      window: "transcript",
      schemaVersion: 1,
      clickThrough: "yes",
    })

    await expect(
      getDetachedWindowInteraction("transcript"),
    ).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })
  })

  /// Two windows share one command surface, so a reply about the wrong window
  /// would silently render one window's state under the other's controls.
  it("rejects a reply that describes a different window", async () => {
    vi.mocked(invoke).mockResolvedValue({
      window: "insights",
      schemaVersion: 1,
      backgroundOpacity: 0.5,
      alwaysOnTop: false,
      compact: false,
    })

    await expect(
      getDetachedWindowAppearance("transcript"),
    ).rejects.toMatchObject({
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

    await expect(openDetachedWindow("transcript")).rejects.toMatchObject({
      details: { code: "window_open_failed" },
    })
  })
})
