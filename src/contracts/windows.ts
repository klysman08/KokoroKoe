import { z } from "zod"

/**
 * Mirrors the Rust opacity bounds. The window never becomes fully invisible:
 * opacity dims the background only, and a window the user cannot see is a
 * window they cannot recover.
 */
export const MINIMUM_BACKGROUND_OPACITY = 0.3
export const MAXIMUM_BACKGROUND_OPACITY = 1

const backgroundOpacitySchema = z
  .number()
  .min(MINIMUM_BACKGROUND_OPACITY)
  .max(MAXIMUM_BACKGROUND_OPACITY)

export const transcriptWindowAppearanceSchema = z.strictObject({
  schemaVersion: z.literal(1),
  backgroundOpacity: backgroundOpacitySchema,
  alwaysOnTop: z.boolean(),
  compact: z.boolean(),
})

export const setTranscriptWindowAppearanceRequestSchema = z.strictObject({
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

export const transcriptWindowShortcutStatusSchema = z.strictObject({
  schemaVersion: z.literal(1),
  binding: bindingSchema,
  enabled: z.boolean(),
  registered: z.boolean(),
})

export const setTranscriptWindowShortcutRequestSchema = z.strictObject({
  binding: bindingSchema,
  enabled: z.boolean(),
})

/**
 * Whether the window passes mouse input through. Never persisted: pointer input
 * is always restored when the application starts.
 */
export const transcriptWindowInteractionSchema = z.strictObject({
  schemaVersion: z.literal(1),
  clickThrough: z.boolean(),
})

export const setTranscriptWindowInteractionRequestSchema = z.strictObject({
  clickThrough: z.boolean(),
})

export type TranscriptWindowInteraction = z.infer<
  typeof transcriptWindowInteractionSchema
>
export type SetTranscriptWindowInteractionRequest = z.infer<
  typeof setTranscriptWindowInteractionRequestSchema
>
export type TranscriptWindowShortcutStatus = z.infer<
  typeof transcriptWindowShortcutStatusSchema
>
export type SetTranscriptWindowShortcutRequest = z.infer<
  typeof setTranscriptWindowShortcutRequestSchema
>
export type TranscriptWindowAppearance = z.infer<
  typeof transcriptWindowAppearanceSchema
>
export type SetTranscriptWindowAppearanceRequest = z.infer<
  typeof setTranscriptWindowAppearanceRequestSchema
>
