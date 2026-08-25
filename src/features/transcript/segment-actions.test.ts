import { describe, expect, it } from "vitest"

import { askManualQuestionRequestSchema } from "@/contracts/manual-question"
import {
  formatTimecode,
  SEGMENT_QUESTION_PRESETS,
  segmentAsText,
} from "./segment-actions"

const identity = {
  requestId: "11111111-1111-4111-8111-111111111111",
  projectId: "22222222-2222-4222-8222-222222222222",
  sessionId: "33333333-3333-4333-8333-333333333333",
  selectedSegmentId: "44444444-4444-4444-8444-444444444444",
}

describe("segment actions", () => {
  /// A preset is only useful if the command would accept it unchanged; a preset
  /// the contract rejects would fail the moment the user pressed Ask.
  it("offers presets the manual-question contract already accepts", () => {
    expect(SEGMENT_QUESTION_PRESETS.length).toBeGreaterThan(0)

    for (const preset of SEGMENT_QUESTION_PRESETS) {
      expect(
        askManualQuestionRequestSchema.safeParse({
          ...identity,
          question: preset.question,
        }).success,
      ).toBe(true)
    }
  })

  it("gives every preset a distinct id, label, and question", () => {
    const ids = SEGMENT_QUESTION_PRESETS.map((preset) => preset.id)
    const labels = SEGMENT_QUESTION_PRESETS.map((preset) => preset.label)
    const questions = SEGMENT_QUESTION_PRESETS.map((preset) => preset.question)

    expect(new Set(ids).size).toBe(ids.length)
    expect(new Set(labels).size).toBe(labels.length)
    expect(new Set(questions).size).toBe(questions.length)
  })

  /// Copying quotes the meeting, so it carries who spoke and when and nothing
  /// internal: no segment id, language, or confidence.
  it("copies the speaker, the timecode, and the text", () => {
    const text = segmentAsText({
      source: "microphone",
      startMs: 125_000,
      text: "We agreed to ship on Friday.",
    })

    expect(text).toBe("[02:05] You: We agreed to ship on Friday.")
    expect(
      segmentAsText({
        source: "system_output",
        startMs: 0,
        text: "Understood.",
      }),
    ).toBe("[00:00] System: Understood.")
  })

  it("formats a timecode and never renders a negative one", () => {
    expect(formatTimecode(0)).toBe("00:00")
    expect(formatTimecode(59_999)).toBe("00:59")
    expect(formatTimecode(3_600_000)).toBe("60:00")
    expect(formatTimecode(-5_000)).toBe("00:00")
  })
})
