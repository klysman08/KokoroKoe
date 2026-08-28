import { type InsightMoment } from "@/contracts/insights"

/**
 * Where in the meeting an insight came from, as a reader would say it.
 *
 * Rust resolves a citation against the segments the model was shown, so this
 * names a place that really exists in the transcript. It is a speaker and a
 * timecode rather than the speech itself: enough to find the moment, without
 * repeating the transcript inside the insight.
 */
export function formatInsightMoment(moment: InsightMoment): string {
  const speaker = moment.source === "microphone" ? "You" : "System"
  return `${speaker} at ${formatMomentTime(moment.startMs)}`
}

/**
 * The moments an insight can actually take a reader to, earliest first.
 *
 * An insight may cite a segment without a resolved place; those are absent
 * rather than rendered, because a reference the reader cannot follow is not a
 * way back into the transcript.
 */
export function navigableMoments(
  moments: readonly InsightMoment[] | undefined,
): InsightMoment[] {
  return [...(moments ?? [])].sort(
    (left, right) => left.startMs - right.startMs,
  )
}

function formatMomentTime(milliseconds: number) {
  const seconds = Math.floor(milliseconds / 1_000)
  const hours = Math.floor(seconds / 3_600)
  const minutes = Math.floor((seconds % 3_600) / 60)
  const remainder = seconds % 60
  const parts = hours > 0 ? [hours, minutes, remainder] : [minutes, remainder]
  return parts.map((value) => value.toString().padStart(2, "0")).join(":")
}
