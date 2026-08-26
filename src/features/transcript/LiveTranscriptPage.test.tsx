import { act, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import fixture from "../../../fixtures/contracts/live-transcription-v1.json"

import { liveTranscriptionFixtureSchema } from "@/contracts/transcription"
import { requestIdSchema } from "@/contracts/models"
import { LiveTranscriptPage } from "@/features/transcript/LiveTranscriptPage"
import {
  markableSegmentId,
  reduceLiveRecords,
  type TranscriptRecord,
} from "@/features/transcript/live-transcript-records"
import {
  getLiveTranscriptionStatus,
  listenToTranscriptionFinals,
  listenToTranscriptionGaps,
  listenToTranscriptionPartials,
  startLiveTranscription,
} from "@/lib/tauri/transcription"

vi.mock("@/lib/tauri/transcription", () => ({
  getLiveTranscriptionStatus: vi.fn(),
  listenToTranscriptionFinals: vi.fn(),
  listenToTranscriptionGaps: vi.fn(),
  listenToTranscriptionPartials: vi.fn(),
  startLiveTranscription: vi.fn(),
  stopLiveTranscription: vi.fn(),
}))

describe("LiveTranscriptPage", () => {
  const parsedFixture = liveTranscriptionFixtureSchema.parse(fixture)

  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(getLiveTranscriptionStatus).mockResolvedValue({ state: "idle" })
    vi.mocked(startLiveTranscription).mockResolvedValue(
      parsedFixture.runningStatus,
    )
    for (const listener of [
      listenToTranscriptionPartials,
      listenToTranscriptionFinals,
      listenToTranscriptionGaps,
    ])
      vi.mocked(listener).mockResolvedValue(() => undefined)
  })

  it("requires explicit capture consent before starting", async () => {
    const user = userEvent.setup()
    render(<LiveTranscriptPage />)

    const start = screen.getByRole("button", { name: "Start transcription" })
    expect(start).toBeDisabled()
    await user.click(screen.getByRole("checkbox"))
    await user.click(start)

    await waitFor(() => expect(startLiveTranscription).toHaveBeenCalledOnce())
    expect(startLiveTranscription).toHaveBeenCalledWith(
      expect.objectContaining({ acknowledgedCaptureConsent: true }),
      expect.any(String),
    )
  })

  it("renders validated partials, replaces them with finals, and labels gaps", async () => {
    let partial:
      ((event: typeof parsedFixture.partialEvent) => void) | undefined
    let final: ((event: typeof parsedFixture.finalEvent) => void) | undefined
    let gap: ((event: typeof parsedFixture.gapEvent) => void) | undefined
    vi.mocked(getLiveTranscriptionStatus).mockResolvedValue(
      parsedFixture.runningStatus,
    )
    vi.mocked(listenToTranscriptionPartials).mockImplementation(
      async (callback) => {
        partial = callback
        return () => undefined
      },
    )
    vi.mocked(listenToTranscriptionFinals).mockImplementation(
      async (callback) => {
        final = callback
        return () => undefined
      },
    )
    vi.mocked(listenToTranscriptionGaps).mockImplementation(
      async (callback) => {
        gap = callback
        return () => undefined
      },
    )
    render(<LiveTranscriptPage />)
    await waitFor(() => expect(partial).toBeDefined())
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument())

    act(() => partial?.(parsedFixture.partialEvent))
    expect(screen.getByText("provisional words")).toBeInTheDocument()
    expect(screen.getByText("You")).toBeInTheDocument()

    act(() => final?.(parsedFixture.finalEvent))
    expect(screen.queryByText("provisional words")).not.toBeInTheDocument()
    expect(screen.getByText("final words")).toBeInTheDocument()

    act(() => partial?.(parsedFixture.partialEvent))
    expect(screen.queryByText("provisional words")).not.toBeInTheDocument()

    act(() => gap?.(parsedFixture.gapEvent))
    expect(screen.getByText(/System audio gap/)).toHaveTextContent(
      "transcription inference failed",
    )
    expect(screen.getByText("2 transient records")).toBeInTheDocument()

    act(() =>
      gap?.({
        ...parsedFixture.gapEvent,
        requestId: requestIdSchema.parse(crypto.randomUUID()),
        sessionSequence: 4,
      }),
    )
    expect(screen.getByText("2 transient records")).toBeInTheDocument()
  })

  it("keeps the newest 500 transient records", () => {
    let records: TranscriptRecord[] = []
    for (let sequence = 1; sequence <= 501; sequence += 1) {
      records = reduceLiveRecords(records, {
        ...parsedFixture.gapEvent,
        eventId: crypto.randomUUID(),
        sessionSequence: sequence,
      })
    }
    expect(records).toHaveLength(500)
  })
})

describe("live segment addressability", () => {
  const parsedFixture = liveTranscriptionFixtureSchema.parse(fixture)

  /// A finalized live segment carries the same id the journal recorded it
  /// under, so it can be marked the moment it appears. A partial is a guess
  /// that will be replaced and names no journal record; a gap is not speech.
  it("addresses finalized segments and nothing else", () => {
    // A partial is superseded by the final that carries its id, so the order
    // here is the order the run produces.
    const records = [
      parsedFixture.partialEvent,
      parsedFixture.gapEvent,
      parsedFixture.finalEvent,
    ].reduce<TranscriptRecord[]>(
      (current, event, index) =>
        reduceLiveRecords(current, {
          ...event,
          eventId: crypto.randomUUID(),
          sessionSequence: index + 1,
        }),
      [],
    )

    const addressable = records
      .map(markableSegmentId)
      .filter((id) => id !== undefined)

    expect(addressable).toEqual([parsedFixture.finalEvent.payload.segment.id])
  })

  it("addresses nothing while a segment is still a partial", () => {
    const records = reduceLiveRecords([], parsedFixture.partialEvent)

    expect(records.map(markableSegmentId)).toEqual([undefined])
  })
})
