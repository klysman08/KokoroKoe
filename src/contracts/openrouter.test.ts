import modelFixture from "../../fixtures/contracts/openrouter-model-v1.json"
import validationFixture from "../../fixtures/contracts/openrouter-validation-v1.json"

import {
  credentialValidationSchema,
  openRouterModelListSchema,
  openRouterModelSchema,
} from "@/contracts/openrouter"

describe("OpenRouter product contracts", () => {
  it("matches the Rust validation and model fixtures", () => {
    expect(credentialValidationSchema.parse(validationFixture)).toEqual(
      validationFixture,
    )
    expect(openRouterModelSchema.parse(modelFixture)).toEqual(modelFixture)
    expect(openRouterModelListSchema.parse([modelFixture])).toEqual([
      modelFixture,
    ])
  })

  it("rejects secrets, non-private models, duplicates, and malformed prices", () => {
    expect(
      credentialValidationSchema.safeParse({
        ...validationFixture,
        apiKey: "secret-canary-contract",
      }).success,
    ).toBe(false)
    expect(
      openRouterModelSchema.safeParse({
        ...modelFixture,
        zeroDataRetentionAvailable: false,
      }).success,
    ).toBe(false)
    expect(
      openRouterModelSchema.safeParse({
        ...modelFixture,
        promptPricePerToken: "NaN",
      }).success,
    ).toBe(false)
    expect(
      openRouterModelListSchema.safeParse([modelFixture, modelFixture]).success,
    ).toBe(false)
  })
})
