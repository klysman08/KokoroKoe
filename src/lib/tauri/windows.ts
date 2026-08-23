import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  setTranscriptWindowAppearanceRequestSchema,
  setTranscriptWindowShortcutRequestSchema,
  transcriptWindowAppearanceSchema,
  transcriptWindowShortcutStatusSchema,
  type SetTranscriptWindowAppearanceRequest,
  type SetTranscriptWindowShortcutRequest,
  type TranscriptWindowAppearance,
  type TranscriptWindowShortcutStatus,
} from "@/contracts/windows"

export const TRANSCRIPT_WINDOW_APPEARANCE_EVENT = "transcript-window-appearance"

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

export async function getTranscriptWindowAppearance(): Promise<TranscriptWindowAppearance> {
  let raw: unknown
  try {
    raw = await invoke<unknown>("get_transcript_window_appearance")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const appearance = transcriptWindowAppearanceSchema.safeParse(raw)
  if (!appearance.success) throw createContractApplicationError()
  return appearance.data
}

export async function setTranscriptWindowAppearance(
  value: SetTranscriptWindowAppearanceRequest,
): Promise<TranscriptWindowAppearance> {
  const request = setTranscriptWindowAppearanceRequestSchema.safeParse(value)
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>("set_transcript_window_appearance", {
      request: request.data,
    })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const appearance = transcriptWindowAppearanceSchema.safeParse(raw)
  if (!appearance.success) throw createContractApplicationError()
  return appearance.data
}

export async function getTranscriptWindowShortcut(): Promise<TranscriptWindowShortcutStatus> {
  let raw: unknown
  try {
    raw = await invoke<unknown>("get_transcript_window_shortcut")
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const status = transcriptWindowShortcutStatusSchema.safeParse(raw)
  if (!status.success) throw createContractApplicationError()
  return status.data
}

export async function setTranscriptWindowShortcut(
  value: SetTranscriptWindowShortcutRequest,
): Promise<TranscriptWindowShortcutStatus> {
  const request = setTranscriptWindowShortcutRequestSchema.safeParse(value)
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>("set_transcript_window_shortcut", {
      request: request.data,
    })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  const status = transcriptWindowShortcutStatusSchema.safeParse(raw)
  if (!status.success) throw createContractApplicationError()
  return status.data
}

/**
 * Subscribes the transcript window to its Rust-owned appearance. That window
 * holds only event permissions, so this is the only way it learns its own
 * opacity and compact state.
 */
export function listenToTranscriptWindowAppearance(
  onEvent: (appearance: TranscriptWindowAppearance) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(TRANSCRIPT_WINDOW_APPEARANCE_EVENT, (event) => {
    const appearance = transcriptWindowAppearanceSchema.safeParse(event.payload)
    if (appearance.success) onEvent(appearance.data)
  })
}
