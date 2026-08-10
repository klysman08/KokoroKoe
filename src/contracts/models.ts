import { z } from "zod"

import { appErrorSchema } from "@/contracts/app-error"

const safeBytes = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER)
const positiveSafeBytes = safeBytes.min(1)
const modelId = z.string().min(1).max(128)
const timestamp = z.iso.datetime({ offset: true })

export const requestIdSchema = z.uuid().brand<"RequestId">()

export const modelDescriptorSchema = z
  .object({
    id: modelId,
    engine: z.literal("whisper"),
    name: z.string().min(1).max(128),
    sourceUrl: z.url().startsWith("https://").max(2048),
    sourceRevision: z.string().min(1).max(128),
    fileName: z
      .string()
      .min(1)
      .max(128)
      .refine((value) => !/[\\/]/.test(value)),
    sha256: z.string().regex(/^[0-9a-fA-F]{64}$/),
    downloadBytes: positiveSafeBytes,
    diskBytes: positiveSafeBytes,
    languages: z.array(z.string().min(1).max(64)).min(1).max(16),
    approximateMemoryBytes: positiveSafeBytes,
    performanceClass: z.enum(["fast", "balanced", "accurate"]),
    backends: z
      .array(z.enum(["cpu", "vulkan"]))
      .min(1)
      .max(2),
    licenseSpdx: z.string().min(1).max(64),
    licenseUrl: z.url().startsWith("https://").max(2048),
  })
  .strict()
  .superRefine((value, context) => {
    if (value.downloadBytes !== value.diskBytes) {
      context.addIssue({
        code: "custom",
        message: "Model disk bytes are inconsistent.",
      })
    }
    if (
      new Set(value.languages).size !== value.languages.length ||
      new Set(value.backends).size !== value.backends.length
    ) {
      context.addIssue({
        code: "custom",
        message: "Model metadata contains duplicate values.",
      })
    }
  })

export const modelDownloadJobSchema = z
  .object({
    requestId: requestIdSchema,
    modelId,
    status: z.enum([
      "queued",
      "downloading",
      "paused",
      "verifying",
      "completed",
      "cancelled",
      "failed",
    ]),
    bytesDownloaded: safeBytes,
    totalBytes: positiveSafeBytes,
    resumable: z.boolean(),
    etag: z.string().min(1).max(512).optional(),
    lastModified: z.string().min(1).max(128).optional(),
    startedAt: timestamp,
    updatedAt: timestamp,
    error: appErrorSchema.optional(),
  })
  .strict()
  .superRefine((value, context) => {
    if (value.bytesDownloaded > value.totalBytes) {
      context.addIssue({
        code: "custom",
        message: "Downloaded bytes exceed the total.",
      })
    }
    if ((value.status === "failed") !== Boolean(value.error)) {
      context.addIssue({
        code: "custom",
        message: "Only failed jobs contain an error.",
      })
    }
    if (
      value.status === "completed" &&
      value.bytesDownloaded !== value.totalBytes
    ) {
      context.addIssue({
        code: "custom",
        message: "Completed jobs must contain all bytes.",
      })
    }
  })

const compatibilitySchema = z
  .object({
    availableDiskBytes: safeBytes,
    requiredDiskBytes: safeBytes,
    availableMemoryBytes: safeBytes,
    approximateMemoryBytes: safeBytes,
    diskCompatible: z.boolean(),
    memoryCompatible: z.boolean(),
  })
  .strict()
  .superRefine((value, context) => {
    if (
      value.diskCompatible !==
      value.availableDiskBytes >= value.requiredDiskBytes
    ) {
      context.addIssue({
        code: "custom",
        message: "Disk compatibility is inconsistent.",
      })
    }
    if (
      value.memoryCompatible !==
      value.availableMemoryBytes >= value.approximateMemoryBytes
    ) {
      context.addIssue({
        code: "custom",
        message: "Memory compatibility is inconsistent.",
      })
    }
  })

export const modelInstallationSchema = z
  .object({
    descriptor: modelDescriptorSchema,
    status: z.enum([
      "not_installed",
      "downloading",
      "installed",
      "failed",
      "incompatible",
    ]),
    installedBytes: safeBytes,
    installedAt: timestamp.optional(),
    selectedAsDefault: z.boolean(),
    availableBackends: z
      .array(z.enum(["cpu", "vulkan"]))
      .min(1)
      .max(2),
    compatibility: compatibilitySchema,
    downloadJob: modelDownloadJobSchema.optional(),
    lastError: appErrorSchema.optional(),
  })
  .strict()
  .superRefine((value, context) => {
    if (
      value.status === "installed" &&
      (!value.installedAt ||
        value.installedBytes !== value.descriptor.diskBytes)
    ) {
      context.addIssue({
        code: "custom",
        message: "Installed model metadata is inconsistent.",
      })
    }
    if (value.status !== "installed" && value.installedBytes !== 0) {
      context.addIssue({
        code: "custom",
        message: "Uninstalled models cannot report installed bytes.",
      })
    }
    if (
      value.downloadJob?.modelId !== undefined &&
      value.downloadJob.modelId !== value.descriptor.id
    ) {
      context.addIssue({
        code: "custom",
        message: "The model job belongs to another model.",
      })
    }
    if (
      value.compatibility.approximateMemoryBytes !==
      value.descriptor.approximateMemoryBytes
    ) {
      context.addIssue({
        code: "custom",
        message: "Model memory guidance is inconsistent.",
      })
    }
    if (value.status === "failed" && !value.lastError) {
      context.addIssue({
        code: "custom",
        message: "Failed models require a sanitized error.",
      })
    }
  })

export const modelInstallationListSchema = z
  .array(modelInstallationSchema)
  .max(8)

export const modelDownloadProgressEnvelopeSchema = z
  .object({
    schemaVersion: z.literal(1),
    eventId: z.uuid(),
    emittedAt: timestamp,
    requestId: requestIdSchema,
    payload: z
      .object({
        job: modelDownloadJobSchema,
        bytesPerSecond: safeBytes.optional(),
      })
      .strict(),
  })
  .strict()
  .refine((value) => value.requestId === value.payload.job.requestId)

export type RequestId = z.infer<typeof requestIdSchema>
export type ModelDownloadJob = z.infer<typeof modelDownloadJobSchema>
export type ModelInstallation = z.infer<typeof modelInstallationSchema>
export type ModelDownloadProgressEnvelope = z.infer<
  typeof modelDownloadProgressEnvelopeSchema
>
