import { invoke } from "@tauri-apps/api/core"
import type { z } from "zod"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

import {
  createContractApplicationError,
  createRequestContractApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  detachedWindowAppearanceSchema,
  detachedWindowInteractionSchema,
  detachedWindowSchema,
  detachedWindowShortcutStatusSchema,
  setDetachedWindowAppearanceRequestSchema,
  setDetachedWindowInteractionRequestSchema,
  setDetachedWindowShortcutRequestSchema,
  type DetachedWindow,
  type DetachedWindowAppearance,
  type DetachedWindowInteraction,
  type DetachedWindowShortcutStatus,
  type SetDetachedWindowAppearanceRequest,
  type SetDetachedWindowInteractionRequest,
  type SetDetachedWindowShortcutRequest,
} from "@/contracts/windows"

export const DETACHED_WINDOW_APPEARANCE_EVENT = "detached-window-appearance"
export const DETACHED_WINDOW_INTERACTION_EVENT = "detached-window-interaction"

/**
 * Opens a detached window.
 *
 * Rust owns every window's label, URL, title, and size. The only thing sent is
 * which of the two windows to act on, so the frontend still cannot address or
 * create an arbitrary webview.
 */
export async function openDetachedWindow(
  window: DetachedWindow,
): Promise<void> {
  await callWindowCommand("open_detached_window", window)
}

export async function closeDetachedWindow(
  window: DetachedWindow,
): Promise<void> {
  await callWindowCommand("close_detached_window", window)
}

async function callWindowCommand(
  command: string,
  window: DetachedWindow,
): Promise<void> {
  const request = detachedWindowSchema.safeParse(window)
  if (!request.success) throw createRequestContractApplicationError()
  try {
    await invoke<void>(command, { request: { window: request.data } })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
}

export async function getDetachedWindowAppearance(
  window: DetachedWindow,
): Promise<DetachedWindowAppearance> {
  return read(
    "get_detached_window_appearance",
    window,
    detachedWindowAppearanceSchema,
  )
}

export async function setDetachedWindowAppearance(
  value: SetDetachedWindowAppearanceRequest,
): Promise<DetachedWindowAppearance> {
  return write(
    "set_detached_window_appearance",
    setDetachedWindowAppearanceRequestSchema.safeParse(value),
    detachedWindowAppearanceSchema,
  )
}

export async function getDetachedWindowShortcut(
  window: DetachedWindow,
): Promise<DetachedWindowShortcutStatus> {
  return read(
    "get_detached_window_shortcut",
    window,
    detachedWindowShortcutStatusSchema,
  )
}

export async function setDetachedWindowShortcut(
  value: SetDetachedWindowShortcutRequest,
): Promise<DetachedWindowShortcutStatus> {
  return write(
    "set_detached_window_shortcut",
    setDetachedWindowShortcutRequestSchema.safeParse(value),
    detachedWindowShortcutStatusSchema,
  )
}

export async function getDetachedWindowInteraction(
  window: DetachedWindow,
): Promise<DetachedWindowInteraction> {
  return read(
    "get_detached_window_interaction",
    window,
    detachedWindowInteractionSchema,
  )
}

export async function setDetachedWindowInteraction(
  value: SetDetachedWindowInteractionRequest,
): Promise<DetachedWindowInteraction> {
  return write(
    "set_detached_window_interaction",
    setDetachedWindowInteractionRequestSchema.safeParse(value),
    detachedWindowInteractionSchema,
  )
}

async function read<T extends { window: DetachedWindow }>(
  command: string,
  window: DetachedWindow,
  schema: z.ZodType<T>,
): Promise<T> {
  const request = detachedWindowSchema.safeParse(window)
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>(command, { request: { window: request.data } })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  return parsed(schema, raw, request.data)
}

async function write<T extends { window: DetachedWindow }>(
  command: string,
  request: z.ZodSafeParseResult<{ window: DetachedWindow }>,
  schema: z.ZodType<T>,
): Promise<T> {
  if (!request.success) throw createRequestContractApplicationError()
  let raw: unknown
  try {
    raw = await invoke<unknown>(command, { request: request.data })
  } catch (error: unknown) {
    throw toApplicationError(error)
  }
  return parsed(schema, raw, request.data.window)
}

/**
 * A reply that describes a different window than the one asked about is a
 * contract violation, not a value to render: it would silently show one
 * window's state under the other's controls.
 */
function parsed<T extends { window: DetachedWindow }>(
  schema: z.ZodType<T>,
  raw: unknown,
  window: DetachedWindow,
): T {
  const result = schema.safeParse(raw)
  if (!result.success || result.data.window !== window)
    throw createContractApplicationError()
  return result.data
}

/**
 * Subscribes a detached window to its pointer-interaction state so a
 * click-through window can show that it is not accepting input.
 *
 * Every window receives every event, so the listener filters to its own.
 */
export function listenToDetachedWindowInteraction(
  window: DetachedWindow,
  onEvent: (interaction: DetachedWindowInteraction) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(DETACHED_WINDOW_INTERACTION_EVENT, (event) => {
    const interaction = detachedWindowInteractionSchema.safeParse(event.payload)
    if (interaction.success && interaction.data.window === window)
      onEvent(interaction.data)
  })
}

/**
 * Subscribes a detached window to its Rust-owned appearance. Those windows hold
 * only event permissions, so this is the only way one learns its own opacity
 * and compact state.
 */
export function listenToDetachedWindowAppearance(
  window: DetachedWindow,
  onEvent: (appearance: DetachedWindowAppearance) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(DETACHED_WINDOW_APPEARANCE_EVENT, (event) => {
    const appearance = detachedWindowAppearanceSchema.safeParse(event.payload)
    if (appearance.success && appearance.data.window === window)
      onEvent(appearance.data)
  })
}
