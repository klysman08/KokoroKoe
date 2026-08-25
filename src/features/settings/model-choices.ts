import type { OpenRouterModel } from "@/contracts/openrouter"

export type ModelChoice = {
  value: string
  label: string
  /** Secondary line: the provider and the context window. */
  detail?: string
  disabled?: boolean
}

/**
 * Builds the options for one role selector.
 *
 * The model id is part of the label rather than only the secondary line,
 * because the label is what the search box matches and what the field shows
 * once a model is chosen — and people know models as `vendor/model-name`, not
 * by display name.
 */
export function buildModelChoices(
  models: readonly OpenRouterModel[],
  value: string | undefined,
): ModelChoice[] {
  const missing = value !== undefined && !models.some((m) => m.id === value)
  return [
    { value: "", label: "Not selected", detail: "Leave this role unset" },
    ...(missing && value
      ? [
          {
            value,
            label: `Unavailable · ${value}`,
            detail: "Not in the loaded catalog",
            disabled: true,
          },
        ]
      : []),
    ...models.map((model) => ({
      value: model.id,
      label: `${model.name} · ${model.id}`,
      detail: `${model.provider} · ${formatContextLength(model.contextLength)} context`,
    })),
  ]
}

/**
 * Substring matching over the label and the secondary line.
 *
 * A leading-match filter would make a catalog of hundreds unusable: nobody
 * recalls whether a model's display name starts with the vendor. Any fragment
 * of the id, the display name, or the provider finds it.
 */
export function matchesModelQuery(choice: ModelChoice, query: string): boolean {
  const needle = query.trim().toLowerCase()
  if (needle.length === 0) return true
  return `${choice.label} ${choice.detail ?? ""}`.toLowerCase().includes(needle)
}

/** Compact token counts: a raw 1048576 tells the reader less than 1.0M. */
export function formatContextLength(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M tokens`
  if (tokens >= 1_000) return `${Math.round(tokens / 1_000)}K tokens`
  return `${tokens} tokens`
}
