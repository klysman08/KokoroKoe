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

export type TranscriptWindowAppearance = z.infer<
  typeof transcriptWindowAppearanceSchema
>
export type SetTranscriptWindowAppearanceRequest = z.infer<
  typeof setTranscriptWindowAppearanceRequestSchema
>
