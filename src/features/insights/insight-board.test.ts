import { describe, expect, it } from "vitest"

import type { InsightType, RecentInsight } from "@/contracts/insights"
import {
  dismissInsight,
  EMPTY_INSIGHT_BOARD,
  insightAsText,
  focusInsight,
  insightKey,
  MAXIMUM_PINNED,
  pinnedCount,
  receiveInsightBatch,
  toggleInsightPin,
  type InsightBoard,
} from "./insight-board"

function insight(
  title: string,
  overrides: Partial<RecentInsight> = {},
): RecentInsight {
  return {
    type: "decision" as InsightType,
    title,
    content: `${title} content`,
    relatedSegmentIds: [],
    ...overrides,
  }
}

function board(...titles: string[]): InsightBoard {
  return receiveInsightBatch(
    EMPTY_INSIGHT_BOARD,
    titles.map((title) => insight(title)),
  )
}

describe("insight board", () => {
  it("shows an arriving batch in order", () => {
    const next = board("first", "second")

    expect(next.cards.map((card) => card.insight.title)).toEqual([
      "first",
      "second",
    ])
    expect(next.cards.every((card) => !card.pinned)).toBe(true)
  })

  /// Insights describe the last few minutes, so an unpinned card must not
  /// outlive the batch that produced it.
  it("replaces unpinned cards with the newer batch", () => {
    const next = receiveInsightBatch(board("first", "second"), [
      insight("third"),
    ])

    expect(next.cards.map((card) => card.insight.title)).toEqual(["third"])
  })

  /// Surviving the next batch is exactly what pinning means.
  it("keeps pinned cards across batches and puts them first", () => {
    const pinned = toggleInsightPin(
      board("first", "second"),
      insightKey(insight("second")),
    )

    const next = receiveInsightBatch(pinned, [insight("third")])

    expect(next.cards.map((card) => card.insight.title)).toEqual([
      "second",
      "third",
    ])
    expect(next.cards[0]!.pinned).toBe(true)
  })

  /// A new batch is new reading. Landing on a pin the user set several batches
  /// ago would hide the thing they just asked for.
  it("lands on the first newly arrived card, not on a pin", () => {
    const pinned = toggleInsightPin(
      board("first"),
      insightKey(insight("first")),
    )

    const next = receiveInsightBatch(pinned, [insight("second")])

    expect(next.focus).toBe(1)
    expect(next.cards[next.focus]!.insight.title).toBe("second")
  })

  /// Every insight in the batch was already dismissed, so there is nothing new
  /// to read and the position must not jump past the end.
  it("stays put when a batch brings nothing new", () => {
    const pinned = toggleInsightPin(
      board("first"),
      insightKey(insight("first")),
    )

    const next = receiveInsightBatch(pinned, [insight("first")])

    expect(next.cards).toHaveLength(1)
    expect(next.focus).toBe(0)
  })

  it("moves the reading position within the cards that exist", () => {
    const next = focusInsight(board("first", "second"), 1)
    expect(next.focus).toBe(1)

    expect(focusInsight(next, 99).focus).toBe(1)
    expect(focusInsight(next, -5).focus).toBe(0)
    expect(focusInsight(EMPTY_INSIGHT_BOARD, 3).focus).toBe(0)
  })

  /// Dismissing must not leave the position past the end, and reading should
  /// continue where the user was rather than jumping to the top.
  it("keeps the reading position valid after a dismissal", () => {
    const reading = focusInsight(board("first", "second", "third"), 2)

    const removed = dismissInsight(reading, insightKey(insight("third")))
    expect(removed.focus).toBe(1)
    expect(removed.cards[removed.focus]!.insight.title).toBe("second")

    const earlier = dismissInsight(removed, insightKey(insight("first")))
    expect(earlier.cards[earlier.focus]!.insight.title).toBe("second")

    const emptied = dismissInsight(earlier, insightKey(insight("second")))
    expect(emptied.cards).toHaveLength(0)
    expect(emptied.focus).toBe(0)
  })

  it("does not duplicate an insight the board already holds", () => {
    const pinned = toggleInsightPin(
      board("first"),
      insightKey(insight("first")),
    )

    const next = receiveInsightBatch(pinned, [insight("first")])

    expect(next.cards).toHaveLength(1)
  })

  /// The model has no memory of what it already said, so the same observation
  /// can arrive again. Dismissing it has to stick or the control is useless.
  it("keeps a dismissed insight from returning in a later batch", () => {
    const dismissed = dismissInsight(
      board("first"),
      insightKey(insight("first")),
    )

    const next = receiveInsightBatch(dismissed, [
      insight("first"),
      insight("second"),
    ])

    expect(next.cards.map((card) => card.insight.title)).toEqual(["second"])
  })

  /// A repeat differs only if the model changed what it said; identical text is
  /// the same observation and stays dismissed.
  it("treats a changed title or content as a different insight", () => {
    const dismissed = dismissInsight(
      board("first"),
      insightKey(insight("first")),
    )

    const next = receiveInsightBatch(dismissed, [
      insight("first", { content: "revised content" }),
    ])

    expect(next.cards).toHaveLength(1)
  })

  it("bounds the dismissed set and forgets the oldest first", () => {
    let current = EMPTY_INSIGHT_BOARD
    for (let index = 0; index < 205; index += 1) {
      current = receiveInsightBatch(current, [insight(`insight ${index}`)])
      current = dismissInsight(current, insightKey(insight(`insight ${index}`)))
    }

    expect(current.dismissed).toHaveLength(200)
    // The oldest dismissal was forgotten, so that insight can appear again.
    expect(
      receiveInsightBatch(current, [insight("insight 0")]).cards,
    ).toHaveLength(1)
    expect(
      receiveInsightBatch(current, [insight("insight 204")]).cards,
    ).toHaveLength(0)
  })

  it("toggles a pin and ignores an unknown card", () => {
    const pinnedOnce = toggleInsightPin(
      board("first"),
      insightKey(insight("first")),
    )
    expect(pinnedCount(pinnedOnce)).toBe(1)

    const unpinned = toggleInsightPin(pinnedOnce, insightKey(insight("first")))
    expect(pinnedCount(unpinned)).toBe(0)

    expect(toggleInsightPin(unpinned, "missing")).toBe(unpinned)
    expect(dismissInsight(unpinned, "missing")).toBe(unpinned)
  })

  /// A pin the user set is a deliberate choice, so the ceiling refuses a new
  /// pin rather than silently discarding an old one.
  it("refuses to pin past the ceiling instead of dropping an existing pin", () => {
    const titles = Array.from(
      { length: MAXIMUM_PINNED + 1 },
      (_value, index) => `insight ${index}`,
    )
    let current = board(...titles)
    for (const title of titles) {
      current = toggleInsightPin(current, insightKey(insight(title)))
    }

    expect(pinnedCount(current)).toBe(MAXIMUM_PINNED)
    expect(
      current.cards.find(
        (card) => card.insight.title === `insight ${MAXIMUM_PINNED}`,
      )?.pinned,
    ).toBe(false)
  })

  it("copies only the text the user can already read", () => {
    const text = insightAsText(
      insight("first", { rationale: "because the transcript says so" }),
    )

    expect(text).toBe(
      "first\n\nfirst content\n\nbecause the transcript says so",
    )
    expect(insightAsText(insight("first"))).toBe("first\n\nfirst content")
  })
})
