import { describe, expect, it } from "vitest"

import openRouterModelFixture from "../../../fixtures/contracts/openrouter-model-v1.json"
import { openRouterModelSchema } from "@/contracts/openrouter"
import {
  buildModelChoices,
  formatContextLength,
  matchesModelQuery,
} from "./model-choices"

const model = openRouterModelSchema.parse(openRouterModelFixture)

describe("model choices", () => {
  it("always offers a way to leave the role unset", () => {
    const choices = buildModelChoices([], undefined)

    expect(choices).toHaveLength(1)
    expect(choices[0]!.value).toBe("")
    expect(choices[0]!.label).toBe("Not selected")
  })

  /// The label is what the field shows once chosen and what the search box
  /// matches, so the id belongs there rather than only in the detail line.
  it("puts the id in the label and the provider and context in the detail", () => {
    const [, option] = buildModelChoices([model], undefined)

    expect(option!.value).toBe(model.id)
    expect(option!.label).toContain(model.name)
    expect(option!.label).toContain(model.id)
    expect(option!.detail).toContain(model.provider)
    expect(option!.detail).toContain("131K tokens")
  })

  /// A stored model that has left the catalog must stay visible and unusable,
  /// not silently read as "Not selected".
  it("keeps a selection that is no longer in the catalog, disabled", () => {
    const choices = buildModelChoices([model], "vendor/withdrawn")

    const missing = choices.find((c) => c.value === "vendor/withdrawn")
    expect(missing?.disabled).toBe(true)
    expect(missing?.label).toContain("Unavailable")
    expect(buildModelChoices([model], model.id).some((c) => c.disabled)).toBe(
      false,
    )
  })

  /// Nobody recalls whether a display name starts with the vendor, so a
  /// leading-match filter would make a catalog of hundreds unusable.
  it("matches any fragment of the id, name, or provider", () => {
    const [, option] = buildModelChoices([model], undefined)

    for (const query of [
      "text-model",
      "example/",
      "Example Text",
      "EXAMPLE",
      " text ",
      model.provider,
    ]) {
      expect(matchesModelQuery(option!, query)).toBe(true)
    }
    expect(matchesModelQuery(option!, "anthropic")).toBe(false)
  })

  it("shows everything for an empty query", () => {
    const [notSelected, option] = buildModelChoices([model], undefined)

    expect(matchesModelQuery(option!, "")).toBe(true)
    expect(matchesModelQuery(notSelected!, "   ")).toBe(true)
  })

  it("formats context windows at a glance", () => {
    expect(formatContextLength(900)).toBe("900 tokens")
    expect(formatContextLength(131_072)).toBe("131K tokens")
    expect(formatContextLength(1_048_576)).toBe("1.0M tokens")
  })
})
