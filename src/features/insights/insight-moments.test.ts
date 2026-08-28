import { describe, expect, it } from "vitest"

import { formatInsightMoment, navigableMoments } from "./insight-moments"

const moment = (startMs: number, source: "microphone" | "system_output") => ({
  segmentId: "44444444-4444-4444-8444-444444444444" as never,
  source,
  startMs,
})

describe("insight moments", () => {
  it("names a speaker and a timecode, not the speech", () => {
    expect(formatInsightMoment(moment(74_000, "microphone"))).toBe(
      "You at 01:14",
    )
    expect(formatInsightMoment(moment(0, "system_output"))).toBe(
      "System at 00:00",
    )
  })

  it("grows to hours only when the meeting has run that long", () => {
    expect(formatInsightMoment(moment(3_599_000, "microphone"))).toBe(
      "You at 59:59",
    )
    expect(formatInsightMoment(moment(3_600_000, "microphone"))).toBe(
      "You at 01:00:00",
    )
  })

  /// A reader scanning citations reads down the meeting, so the earliest place
  /// comes first however the model happened to order them.
  it("orders moments by when they happened", () => {
    const ordered = navigableMoments([
      moment(9_000, "microphone"),
      moment(1_000, "system_output"),
    ])

    expect(ordered.map((one) => one.startMs)).toEqual([1_000, 9_000])
  })

  it("treats an insight with no resolved moments as having none", () => {
    expect(navigableMoments(undefined)).toEqual([])
  })
})
