import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import fixture from "../../../fixtures/contracts/live-transcription-v1.json"

import { liveTranscriptionFixtureSchema } from "@/contracts/transcription"
import {
  getLiveTranscriptionStatus,
  listenToTranscriptionPartials,
  startLiveTranscription,
  stopLiveTranscription,
} from "@/lib/tauri/transcription"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))

describe("live transcription Tauri adapter", () => {
  const parsedFixture = liveTranscriptionFixtureSchema.parse(fixture)
  beforeEach(() => vi.clearAllMocks())

  it("invokes the exact commands with validated arguments", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(parsedFixture.runningStatus)
      .mockResolvedValueOnce(parsedFixture.runningStatus)
      .mockResolvedValueOnce(parsedFixture.stoppedStatus)

    await startLiveTranscription(
      parsedFixture.input,
      parsedFixture.runningStatus.requestId!,
    )
    await getLiveTranscriptionStatus()
    await stopLiveTranscription(parsedFixture.runningStatus.requestId!)

    expect(invoke).toHaveBeenNthCalledWith(1, "start_live_transcription", {
      input: parsedFixture.input,
      requestId: parsedFixture.runningStatus.requestId,
    })
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "get_live_transcription_status",
      undefined,
    )
    expect(invoke).toHaveBeenNthCalledWith(3, "stop_live_transcription", {
      requestId: parsedFixture.runningStatus.requestId,
    })
  })

  it("drops malformed native events at the boundary", async () => {
    let handler: ((event: { payload: unknown }) => void) | undefined
    vi.mocked(listen).mockImplementation(async (_event, callback) => {
      handler = callback as (event: { payload: unknown }) => void
      return () => undefined
    })
    const received = vi.fn()
    await listenToTranscriptionPartials(received)

    handler?.({ payload: { ...fixture.partialEvent, extra: true } })
    handler?.({ payload: fixture.partialEvent })

    expect(received).toHaveBeenCalledOnce()
    expect(received).toHaveBeenCalledWith(fixture.partialEvent)
  })
})
