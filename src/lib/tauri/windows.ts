import { invoke } from "@tauri-apps/api/core"

import { toApplicationError } from "@/contracts/app-error"

/**
 * Opens the detached transcript window.
 *
 * Rust owns the window's label, URL, title, and size: no argument is sent, so
 * the frontend cannot address or create any other webview.
 */
export async function openTranscriptWindow(): Promise<void> {
  try {
    await invoke<void>("open_transcript_window")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
}

export async function closeTranscriptWindow(): Promise<void> {
  try {
    await invoke<void>("close_transcript_window")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
}
