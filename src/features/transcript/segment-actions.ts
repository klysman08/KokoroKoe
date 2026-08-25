import type { AudioSource } from "@/contracts/audio"

/**
 * One-click questions for a transcript segment.
 *
 * The Manifest lists "explain", "suggest a response", and "summarize" as
 * separate per-segment actions. They are implemented as prefilled questions
 * through the one verified manual-question path rather than as hidden prompts
 * of their own, for two reasons: the answer stays inside the schema that path
 * already validates, and — because this application sends meeting text to a
 * third party — the user sees the exact question before anything leaves the
 * machine, and can edit or abandon it.
 */
export type SegmentQuestionPreset = {
  id: string
  label: string
  question: string
}

export const SEGMENT_QUESTION_PRESETS: readonly SegmentQuestionPreset[] = [
  {
    id: "explain",
    label: "Explain",
    question:
      "Explain what was said in this part of the conversation, in plain language, using only the transcript context.",
  },
  {
    id: "suggest-response",
    label: "Suggest a response",
    question:
      "Suggest how I could respond to this part of the conversation. Keep it short and specific to what was actually said.",
  },
  {
    id: "summarize",
    label: "Summarize",
    question:
      "Summarize this part of the conversation in a few sentences, covering only what the transcript supports.",
  },
]

const SOURCE_LABELS: Record<AudioSource, string> = {
  microphone: "You",
  system_output: "System",
}

/**
 * Plain text for the clipboard: who spoke, when, and what they said.
 *
 * Segment identifiers, language detection, and confidence are deliberately left
 * out — they are internal accounting, not part of what the user is quoting.
 */
export function segmentAsText(segment: {
  source: AudioSource
  startMs: number
  text: string
}): string {
  return `[${formatTimecode(segment.startMs)}] ${SOURCE_LABELS[segment.source]}: ${segment.text}`
}

export function formatTimecode(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor(milliseconds / 1_000))
  const minutes = Math.floor(seconds / 60)
  return `${minutes.toString().padStart(2, "0")}:${(seconds % 60)
    .toString()
    .padStart(2, "0")}`
}
