import type { InsightType } from "@/contracts/insights"

/**
 * Human labels for the insight types. Shared so the main window's panel and the
 * detached window can never disagree about what a type is called.
 */
export const INSIGHT_LABELS: Record<InsightType, string> = {
  suggested_response: "Suggested response",
  follow_up_question: "Follow-up question",
  clarification: "Clarification",
  fact_or_number: "Fact or number",
  risk: "Risk",
  objection: "Objection",
  decision: "Decision",
  action_item: "Action item",
  contradiction: "Contradiction",
  unaddressed_topic: "Unaddressed topic",
}
