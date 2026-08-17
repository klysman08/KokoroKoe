import { invoke } from "@tauri-apps/api/core"
import { beforeEach, describe, expect, it, vi } from "vitest"

import fixture from "../../../fixtures/contracts/manual-question-v1.json"
import { manualQuestionFixtureSchema } from "@/contracts/manual-question"
import { askManualQuestion } from "./manual-question"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const contract = manualQuestionFixtureSchema.parse(fixture)

describe("manual question Tauri adapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset())

  it("invokes only the strict request wrapper and parses the response", async () => {
    vi.mocked(invoke).mockResolvedValue(contract.response)

    await expect(askManualQuestion(contract.request)).resolves.toEqual(
      contract.response,
    )
    expect(invoke).toHaveBeenCalledWith("ask_manual_question", {
      request: contract.request,
    })
  })

  it("rejects malformed and cross-scope success data", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...contract.response,
      selectedSegmentId: "55555555-5555-4555-8555-555555555555",
    })
    await expect(askManualQuestion(contract.request)).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })

    await expect(
      askManualQuestion({ ...contract.request, question: " unsafe" }),
    ).rejects.toMatchObject({
      details: { code: "invalid_request_contract" },
    })
  })
})
