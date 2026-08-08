import { z } from "zod"

const safeCounterSchema = z
  .number()
  .int()
  .nonnegative()
  .max(Number.MAX_SAFE_INTEGER)
const hasNoControlCharacters = (value: string) =>
  [...value].every((character) => {
    const codePoint = character.codePointAt(0) ?? 0
    return codePoint > 31 && codePoint !== 127
  })
const endpointIdSchema = z
  .string()
  .min(1)
  .max(1024)
  .refine(hasNoControlCharacters)
const friendlyNameSchema = z
  .string()
  .min(1)
  .max(512)
  .refine(hasNoControlCharacters)

export const audioSourceSchema = z.enum(["microphone", "system_output"])
export const deviceRoleSchema = z.enum([
  "console",
  "multimedia",
  "communications",
])

export const deviceSelectionSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("default"), role: deviceRoleSchema }),
  z.strictObject({
    kind: z.literal("fixed"),
    endpointId: endpointIdSchema,
  }),
])

export const audioPrototypeStartRequestSchema = z.strictObject({
  acknowledgedCaptureConsent: z.literal(true),
  microphone: deviceSelectionSchema,
  systemOutput: deviceSelectionSchema,
  queueCapacityPacketsPerSource: z.number().int().min(4).max(256),
})

export const nativeAudioFormatSchema = z.strictObject({
  sampleRate: z.number().int().positive().max(0xffff_ffff),
  channels: z.number().int().positive().max(0xffff),
  bitsPerSample: z.number().int().positive().max(0xffff),
  validBitsPerSample: z.number().int().nonnegative().max(0xffff),
  blockAlign: z.number().int().positive().max(0xffff_ffff),
  channelMask: z.number().int().nonnegative().max(0xffff_ffff),
  sampleType: z.enum(["float", "integer", "unknown"]),
})

export const audioDeviceSchema = z.strictObject({
  endpointId: endpointIdSchema,
  friendlyName: friendlyNameSchema,
  direction: z.enum(["input", "output"]),
  isDefaultConsole: z.boolean(),
  isDefaultMultimedia: z.boolean(),
  isDefaultCommunications: z.boolean(),
  nativeFormat: nativeAudioFormatSchema.nullable(),
})

export const audioDeviceListSchema = z.strictObject({
  inputs: z.array(audioDeviceSchema).max(256),
  outputs: z.array(audioDeviceSchema).max(256),
})

const channelDiagnosticsSchema = z.strictObject({
  source: audioSourceSchema,
  status: z.enum([
    "starting",
    "active",
    "reconnecting",
    "unavailable",
    "stopped",
  ]),
  endpointId: endpointIdSchema.nullable(),
  nativeFormat: nativeAudioFormatSchema.nullable(),
  captureAttempts: safeCounterSchema,
  packetsCaptured: safeCounterSchema,
  framesCaptured: safeCounterSchema,
  packetsConsumed: safeCounterSchema,
  framesConsumed: safeCounterSchema,
  bytesConsumed: safeCounterSchema,
  queueDrops: safeCounterSchema,
  dataDiscontinuities: safeCounterSchema,
  timestampErrors: safeCounterSchema,
  timestampRegressions: safeCounterSchema,
  firstPacketMs: safeCounterSchema.nullable(),
  lastPacketMs: safeCounterSchema.nullable(),
  lastErrorCode: z.string().min(1).max(128).nullable(),
})

export const audioPrototypeStatusSchema = z.strictObject({
  state: z.enum(["starting", "capturing", "stopping", "stopped"]),
  elapsedMs: safeCounterSchema,
  queueCapacityPacketsPerSource: z.number().int().min(4).max(256),
  microphone: channelDiagnosticsSchema.extend({
    source: z.literal("microphone"),
  }),
  systemOutput: channelDiagnosticsSchema.extend({
    source: z.literal("system_output"),
  }),
})

export type AudioPrototypeStartRequest = z.infer<
  typeof audioPrototypeStartRequestSchema
>
export type AudioDeviceList = z.infer<typeof audioDeviceListSchema>
export type AudioPrototypeStatus = z.infer<typeof audioPrototypeStatusSchema>
