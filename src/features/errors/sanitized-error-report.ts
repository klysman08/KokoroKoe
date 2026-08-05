import { type ApplicationError } from "@/contracts/app-error"

export function createSanitizedErrorReport(error: ApplicationError) {
  const {
    code,
    correlationId,
    retryable,
    severity,
    technicalDetail,
    userMessage,
  } = error.details

  return [
    "KokoroKoe sanitized error report",
    `code: ${code}`,
    `severity: ${severity}`,
    `retryable: ${String(retryable)}`,
    `correlationId: ${correlationId}`,
    `message: ${userMessage}`,
    technicalDetail ? `detail: ${technicalDetail}` : undefined,
  ]
    .filter((line): line is string => line !== undefined)
    .join("\n")
}
