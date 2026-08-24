import { z } from "zod"

/**
 * The detached windows Rust owns.
 *
 * A closed set, not a label: naming a window is the only window identity the
 * frontend ever sends, so it can never supply a webview label, URL, or size.
 */
export const detachedWindowSchema = z.enum(["transcript", "insights"])

export const DETACHED_WINDOWS = detachedWindowSchema.options

/**
 * Mirrors the Rust opacity bounds. A window never becomes fully invisible:
 * opacity dims the background only, and a window the user cannot see is a
 * window they cannot recover.
 */
export const MINIMUM_BACKGROUND_OPACITY = 0.3
export const MAXIMUM_BACKGROUND_OPACITY = 1

const backgroundOpacitySchema = z
  .number()
  .min(MINIMUM_BACKGROUND_OPACITY)
  .max(MAXIMUM_BACKGROUND_OPACITY)

/**
 * Every value Rust publishes names the window it describes, because every
 * window receives every event and must ignore the ones that are not its own.
 */
export const detachedWindowAppearanceSchema = z.strictObject({
  window: detachedWindowSchema,
  schemaVersion: z.literal(1),
  backgroundOpacity: backgroundOpacitySchema,
  alwaysOnTop: z.boolean(),
  compact: z.boolean(),
})

export const setDetachedWindowAppearanceRequestSchema = z.strictObject({
  window: detachedWindowSchema,
  backgroundOpacity: backgroundOpacitySchema,
  alwaysOnTop: z.boolean(),
  compact: z.boolean(),
})

/**
 * A show/hide binding. Rust owns parsing and canonicalization; the frontend
 * only checks the shape so a malformed value never reaches the command.
 */
const bindingSchema = z
  .string()
  .min(1)
  .max(64)
  .refine((value) => !/\p{Cc}/u.test(value))

export const detachedWindowShortcutStatusSchema = z.strictObject({
  window: detachedWindowSchema,
  schemaVersion: z.literal(1),
  binding: bindingSchema,
  enabled: z.boolean(),
  registered: z.boolean(),
})

export const setDetachedWindowShortcutRequestSchema = z.strictObject({
  window: detachedWindowSchema,
  binding: bindingSchema,
  enabled: z.boolean(),
})

/**
 * Whether a window passes mouse input through. Never persisted: pointer input
 * is always restored when the application starts.
 */
export const detachedWindowInteractionSchema = z.strictObject({
  window: detachedWindowSchema,
  schemaVersion: z.literal(1),
  clickThrough: z.boolean(),
})

export const setDetachedWindowInteractionRequestSchema = z.strictObject({
  window: detachedWindowSchema,
  clickThrough: z.boolean(),
})

export type DetachedWindow = z.infer<typeof detachedWindowSchema>
export type DetachedWindowInteraction = z.infer<
  typeof detachedWindowInteractionSchema
>
export type SetDetachedWindowInteractionRequest = z.infer<
  typeof setDetachedWindowInteractionRequestSchema
>
export type DetachedWindowShortcutStatus = z.infer<
  typeof detachedWindowShortcutStatusSchema
>
export type SetDetachedWindowShortcutRequest = z.infer<
  typeof setDetachedWindowShortcutRequestSchema
>
export type DetachedWindowAppearance = z.infer<
  typeof detachedWindowAppearanceSchema
>
export type SetDetachedWindowAppearanceRequest = z.infer<
  typeof setDetachedWindowAppearanceRequestSchema
>
