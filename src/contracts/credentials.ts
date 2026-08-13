import { z } from "zod"

export const credentialStatusSchema = z.strictObject({
  configured: z.boolean(),
  validatedAt: z.iso.datetime({ offset: false }).optional(),
})

export const openRouterApiKeySchema = z
  .string()
  .min(16)
  .max(2048)
  .refine(
    (value) =>
      value.trim() === value &&
      !Array.from(value).some((character) => {
        const codePoint = character.codePointAt(0) ?? 0
        return codePoint <= 31 || codePoint === 127
      }),
    {
      message: "The API key format is invalid.",
    },
  )

export type CredentialStatus = z.infer<typeof credentialStatusSchema>
