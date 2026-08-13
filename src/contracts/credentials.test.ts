import fixture from "../../fixtures/contracts/credential-status-v1.json"

import {
  credentialStatusSchema,
  openRouterApiKeySchema,
} from "@/contracts/credentials"

describe("credential contracts", () => {
  it("accepts the shared status fixture without a secret field", () => {
    expect(credentialStatusSchema.parse(fixture)).toEqual(fixture)
    expect(JSON.stringify(fixture)).not.toContain("apiKey")
    expect(
      credentialStatusSchema.safeParse({ ...fixture, apiKey: "secret" })
        .success,
    ).toBe(false)
  })

  it("bounds transient keys without freezing a provider prefix", () => {
    expect(
      openRouterApiKeySchema.safeParse("provider-format-can-change-1234")
        .success,
    ).toBe(true)
    for (const invalid of [
      "short",
      " leading-key-material",
      "trailing-key-material ",
      "key-material-with\ncontrol",
    ]) {
      expect(openRouterApiKeySchema.safeParse(invalid).success).toBe(false)
    }
  })
})
