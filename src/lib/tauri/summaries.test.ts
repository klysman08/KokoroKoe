import { invoke } from "@tauri-apps/api/core"
import { beforeEach, describe, expect, it, vi } from "vitest"

import fixture from "../../../fixtures/contracts/session-summary-v1.json"
import { sessionSummaryFixtureSchema } from "@/contracts/summaries"
import { generateSessionSummary, getSessionSummary } from "./summaries"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const contract = sessionSummaryFixtureSchema.parse(fixture)
const scope = {
  projectId: contract.status.projectId,
  sessionId: contract.status.sessionId,
}

describe("session summary Tauri adapter", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset()
  })

  it("reads a saved summary and an absent one through the same strict wrapper", async () => {
    vi.mocked(invoke).mockResolvedValue(contract.status)
    await expect(getSessionSummary(scope)).resolves.toEqual(contract.status)
    expect(invoke).toHaveBeenCalledWith("get_session_summary", {
      request: scope,
    })

    vi.mocked(invoke).mockResolvedValue(contract.absentStatus)
    const absent = await getSessionSummary(scope)
    expect(absent.document).toBeUndefined()
  })

  it("generates a summary and carries only the scoped identity", async () => {
    vi.mocked(invoke).mockResolvedValue(contract.generateResponse)

    await expect(
      generateSessionSummary(contract.generateRequest),
    ).resolves.toEqual(contract.generateResponse)
    expect(invoke).toHaveBeenCalledWith("generate_session_summary", {
      request: contract.generateRequest,
    })
    expect(Object.keys(contract.generateRequest)).toEqual([
      "requestId",
      "projectId",
      "sessionId",
    ])
  })

  it("rejects malformed and cross-scope success data", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...contract.status,
      sessionId: "99999999-9999-4999-8999-999999999999",
    })
    await expect(getSessionSummary(scope)).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })

    vi.mocked(invoke).mockResolvedValue({
      ...contract.generateResponse,
      document: {
        ...contract.generateResponse.document,
        segmentsIncluded: 9_999,
      },
    })
    await expect(
      generateSessionSummary(contract.generateRequest),
    ).rejects.toMatchObject({
      details: { code: "invalid_backend_contract" },
    })

    await expect(
      getSessionSummary({
        ...scope,
        projectId: "not-a-uuid" as unknown as typeof scope.projectId,
      }),
    ).rejects.toMatchObject({
      details: { code: "invalid_request_contract" },
    })
  })
})
