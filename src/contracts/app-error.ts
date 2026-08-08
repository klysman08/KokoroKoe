import { z } from "zod"

export const appErrorSchema = z
  .object({
    code: z.string().min(1).max(128),
    userMessage: z.string().min(1).max(512),
    technicalDetail: z.string().min(1).max(512).optional(),
    severity: z.enum(["info", "warning", "error", "critical"]),
    retryable: z.boolean(),
    correlationId: z.uuid(),
  })
  .strict()

export const commandErrorSchema = z
  .object({
    error: appErrorSchema,
  })
  .strict()

export type AppError = z.infer<typeof appErrorSchema>

export class ApplicationError extends Error {
  readonly details: AppError

  constructor(details: AppError) {
    super(details.userMessage)
    this.name = "ApplicationError"
    this.details = details
  }
}

export function toApplicationError(value: unknown): ApplicationError {
  if (value instanceof ApplicationError) {
    return value
  }

  const parsed = commandErrorSchema.safeParse(value)
  if (parsed.success) {
    return new ApplicationError(parsed.data.error)
  }

  return createUnexpectedApplicationError()
}

export function createContractApplicationError(): ApplicationError {
  return new ApplicationError({
    code: "invalid_backend_contract",
    userMessage:
      "KokoroKoe received an invalid response from its local service.",
    technicalDetail:
      "The response did not match the expected settings contract.",
    severity: "error",
    retryable: false,
    correlationId: createCorrelationId(),
  })
}

export function createRequestContractApplicationError(): ApplicationError {
  return new ApplicationError({
    code: "invalid_request_contract",
    userMessage: "The settings change is not valid.",
    technicalDetail:
      "The request did not match the expected local settings contract.",
    severity: "warning",
    retryable: false,
    correlationId: createCorrelationId(),
  })
}

export function createUnexpectedApplicationError(): ApplicationError {
  return new ApplicationError({
    code: "unexpected_application_error",
    userMessage: "KokoroKoe encountered an unexpected application error.",
    technicalDetail:
      "Raw error details were omitted to protect local paths and meeting content.",
    severity: "error",
    retryable: true,
    correlationId: createCorrelationId(),
  })
}

function createCorrelationId() {
  return globalThis.crypto.randomUUID()
}
