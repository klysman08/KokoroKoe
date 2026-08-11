import { z } from "zod"

import { appErrorSchema } from "@/contracts/app-error"
import { requestIdSchema } from "@/contracts/models"

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
const timestampSchema = z.iso.datetime({ offset: true })

export const audioSourceSchema = z.enum(["microphone", "system_output"])
export const deviceRoleSchema = z.enum([
  "console",
  "multimedia",
  "communications",
])

export const deviceSelectionSchema = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("default"), role: deviceRoleSchema }),
  z.strictObject({ kind: z.literal("fixed"), endpointId: endpointIdSchema }),
])

export const audioDeviceSchema = z.strictObject({
  endpointId: endpointIdSchema,
  friendlyName: friendlyNameSchema,
  direction: z.enum(["input", "output"]),
  state: z.enum(["active", "disabled", "not_present", "unplugged"]),
  isDefaultConsole: z.boolean(),
  isDefaultMultimedia: z.boolean(),
  isDefaultCommunications: z.boolean(),
  sampleRate: z.number().int().positive().max(0xffff_ffff).optional(),
  channels: z.number().int().positive().max(0xffff).optional(),
})

export const audioDeviceListSchema = z
  .strictObject({
    inputs: z.array(audioDeviceSchema).max(256),
    outputs: z.array(audioDeviceSchema).max(256),
  })
  .superRefine((value, context) => {
    for (const [index, device] of value.inputs.entries()) {
      if (device.direction !== "input") {
        context.addIssue({
          code: "custom",
          path: ["inputs", index, "direction"],
          message: "Input device direction is inconsistent.",
        })
      }
    }
    for (const [index, device] of value.outputs.entries()) {
      if (device.direction !== "output") {
        context.addIssue({
          code: "custom",
          path: ["outputs", index, "direction"],
          message: "Output device direction is inconsistent.",
        })
      }
    }
  })

export const deviceTestInputSchema = z.strictObject({
  source: audioSourceSchema,
  selection: deviceSelectionSchema,
})

export const deviceTestStatusSchema = z
  .strictObject({
    requestId: requestIdSchema,
    source: audioSourceSchema,
    status: z.enum(["starting", "active", "stopped", "failed"]),
    device: audioDeviceSchema.optional(),
    error: appErrorSchema.optional(),
  })
  .superRefine((value, context) => {
    if (value.status === "failed" && value.error === undefined) {
      context.addIssue({
        code: "custom",
        message: "Failed tests require an error.",
      })
    }
    if (value.status !== "failed" && value.error !== undefined) {
      context.addIssue({
        code: "custom",
        message: "Only failed tests may carry an error.",
      })
    }
    if (
      value.device !== undefined &&
      ((value.source === "microphone" && value.device.direction !== "input") ||
        (value.source === "system_output" &&
          value.device.direction !== "output"))
    ) {
      context.addIssue({
        code: "custom",
        message: "Test device direction is inconsistent.",
      })
    }
  })

export const channelHealthSchema = z.strictObject({
  status: z.enum([
    "starting",
    "active",
    "silent",
    "reconnecting",
    "unavailable",
    "stopped",
  ]),
  endpointId: endpointIdSchema.optional(),
  detailCode: z.string().min(1).max(128).optional(),
  updatedAt: timestampSchema,
})

const eventEnvelopeFields = {
  schemaVersion: z.literal(1),
  eventId: z.uuid(),
  emittedAt: timestampSchema,
  requestId: requestIdSchema,
}

export const audioLevelUpdatedEnvelopeSchema = z
  .strictObject({
    ...eventEnvelopeFields,
    payload: z.strictObject({
      testId: requestIdSchema,
      source: audioSourceSchema,
      rmsDbfs: z.number().finite().min(-120).max(0),
      peakDbfs: z.number().finite().min(-120).max(0),
      clipping: z.boolean(),
      muted: z.boolean(),
      atMs: safeCounterSchema,
    }),
  })
  .superRefine((value, context) => {
    if (value.requestId !== value.payload.testId) {
      context.addIssue({
        code: "custom",
        message: "Audio level request identity differs.",
      })
    }
    if (value.payload.rmsDbfs > value.payload.peakDbfs) {
      context.addIssue({
        code: "custom",
        message: "RMS cannot exceed peak level.",
      })
    }
  })

export const audioDeviceStatusChangedEnvelopeSchema = z.strictObject({
  ...eventEnvelopeFields,
  payload: z.strictObject({
    source: audioSourceSchema,
    previous: channelHealthSchema,
    current: channelHealthSchema,
    isDefaultChange: z.boolean(),
  }),
})

export const productAudioFixtureSchema = z.strictObject({
  deviceList: audioDeviceListSchema,
  input: deviceTestInputSchema,
  status: deviceTestStatusSchema,
  levelEvent: audioLevelUpdatedEnvelopeSchema,
  healthEvent: audioDeviceStatusChangedEnvelopeSchema,
})

export type AudioSource = z.infer<typeof audioSourceSchema>
export type DeviceRole = z.infer<typeof deviceRoleSchema>
export type DeviceSelection = z.infer<typeof deviceSelectionSchema>
export type AudioDevice = z.infer<typeof audioDeviceSchema>
export type AudioDeviceList = z.infer<typeof audioDeviceListSchema>
export type DeviceTestInput = z.infer<typeof deviceTestInputSchema>
export type DeviceTestStatus = z.infer<typeof deviceTestStatusSchema>
export type AudioLevelUpdatedEnvelope = z.infer<
  typeof audioLevelUpdatedEnvelopeSchema
>
export type AudioDeviceStatusChangedEnvelope = z.infer<
  typeof audioDeviceStatusChangedEnvelopeSchema
>
