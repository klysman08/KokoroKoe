import { type Session } from "@/contracts/projects"

/** How a session state reads to someone who thinks of it as a meeting. */
export const MEETING_STATE_LABELS: Record<Session["state"], string> = {
  idle: "Not started",
  preparing: "Starting",
  capturing: "Starting",
  transcribing: "Live",
  paused: "Paused",
  stopping: "Ending",
  processing_summary: "Summarizing",
  completed: "Ended",
  failed: "Interrupted",
}
