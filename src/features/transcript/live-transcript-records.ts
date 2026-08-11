import {
  type LiveTranscriptSegment,
  type TranscriptionFinalEnvelope,
  type TranscriptionGapEnvelope,
  type TranscriptionPartialEnvelope,
} from "@/contracts/transcription"

const MAX_TRANSCRIPT_RECORDS = 500

export type TranscriptRecord =
  | { kind: "segment"; segment: LiveTranscriptSegment }
  | {
      kind: "gap"
      id: string
      source: "microphone" | "system_output"
      startMs: number
      endMs: number
      code: string
    }

type OrderedTranscriptEvent =
  | TranscriptionPartialEnvelope
  | TranscriptionFinalEnvelope
  | TranscriptionGapEnvelope

export function reduceLiveRecords(
  records: TranscriptRecord[],
  event: OrderedTranscriptEvent,
): TranscriptRecord[] {
  const payload = event.payload
  let next: TranscriptRecord[]
  if ("segment" in payload) {
    const index = records.findIndex(
      (record) =>
        record.kind === "segment" && record.segment.id === payload.segment.id,
    )
    const record: TranscriptRecord = {
      kind: "segment",
      segment: payload.segment,
    }
    if (index === -1) next = [...records, record]
    else
      next = records.map((current, currentIndex) =>
        currentIndex === index ? record : current,
      )
  } else {
    next = [
      ...records,
      { kind: "gap", id: `gap-${event.sessionSequence}`, ...payload },
    ]
  }
  return next.slice(-MAX_TRANSCRIPT_RECORDS)
}
