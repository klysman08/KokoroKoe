import { z } from "zod"

const boundedText = (maximumBytes: number) =>
  z
    .string()
    .min(1)
    .refine(
      (value) =>
        new TextEncoder().encode(value).length <= maximumBytes &&
        value.trim() === value &&
        !Array.from(value).some((character) => {
          const codePoint = character.codePointAt(0) ?? 0
          return codePoint <= 31 || codePoint === 127
        }),
      { message: "OpenRouter text metadata is invalid." },
    )

const price = z
  .string()
  .min(1)
  .max(64)
  .refine((value) => {
    const parsed = Number(value)
    return value.trim() === value && Number.isFinite(parsed) && parsed >= 0
  })

export const credentialValidationSchema = z
  .object({
    valid: z.literal(true),
    validatedAt: z.iso.datetime({ offset: false }),
    usageUsd: price.optional(),
    remainingLimitUsd: price.optional(),
    expiresAt: z.iso.datetime({ offset: false }).optional(),
  })
  .strict()

export const openRouterModelSchema = z
  .object({
    id: boundedText(256),
    name: boundedText(256),
    provider: boundedText(128),
    contextLength: z.number().int().min(1).max(10_000_000),
    promptPricePerToken: price,
    completionPricePerToken: price,
    supportsStructuredOutputs: z.boolean(),
    supportsStreaming: z.boolean(),
    zeroDataRetentionAvailable: z.literal(true),
    dataCollection: z.literal("deny"),
  })
  .strict()

export const openRouterModelListSchema = z
  .array(openRouterModelSchema)
  .max(500)
  .superRefine((models, context) => {
    const ids = new Set<string>()
    for (const [index, model] of models.entries()) {
      if (ids.has(model.id)) {
        context.addIssue({
          code: "custom",
          path: [index, "id"],
          message: "OpenRouter model IDs must be unique.",
        })
      }
      ids.add(model.id)
      if (index > 0) {
        const previous = models[index - 1]
        if (!previous) continue
        if (
          previous.name > model.name ||
          (previous.name === model.name && previous.id > model.id)
        ) {
          context.addIssue({
            code: "custom",
            path: [index],
            message: "OpenRouter models must use canonical ordering.",
          })
        }
      }
    }
  })

export type CredentialValidation = z.infer<typeof credentialValidationSchema>
export type OpenRouterModel = z.infer<typeof openRouterModelSchema>
