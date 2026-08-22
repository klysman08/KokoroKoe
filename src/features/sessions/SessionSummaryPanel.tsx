import { useEffect, useState } from "react"
import { FileCheck2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { SanitizedMarkdown } from "@/components/content/SanitizedMarkdown"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import type { Project, Session } from "@/contracts/projects"
import {
  generateSessionSummaryRequestSchema,
  type SessionSummaryDocument,
} from "@/contracts/summaries"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import {
  generateSessionSummary,
  getSessionSummary,
} from "@/lib/tauri/summaries"

/**
 * The final summary for a finished Session.
 *
 * Unlike insights, this content is persisted: generating publishes the Session's
 * `summary.md`, and the saved document is what is displayed on later visits.
 */
export function SessionSummaryPanel({
  project,
  session,
}: {
  project: Project
  session: Session
}) {
  const [document, setDocument] = useState<SessionSummaryDocument>()
  const [error, setError] = useState<ApplicationError>()
  const [loading, setLoading] = useState(true)
  const [pending, setPending] = useState(false)

  useEffect(() => {
    let disposed = false
    // The identifiers already come from validated Project/Session records, and
    // the adapter revalidates them before invoking.
    void getSessionSummary({ projectId: project.id, sessionId: session.id })
      .then((status) => {
        if (!disposed) setDocument(status.document)
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
      .finally(() => {
        if (!disposed) setLoading(false)
      })
    return () => {
      disposed = true
    }
  }, [project.id, session.id])

  async function generate() {
    const request = generateSessionSummaryRequestSchema.safeParse({
      requestId: globalThis.crypto.randomUUID(),
      projectId: project.id,
      sessionId: session.id,
    })
    if (!request.success) return

    setError(undefined)
    setPending(true)
    try {
      const response = await generateSessionSummary(request.data)
      setDocument(response.document)
    } catch (caught: unknown) {
      setError(toApplicationError(caught))
    } finally {
      setPending(false)
    }
  }

  return (
    <section aria-label={`Final summary for ${session.title}`}>
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-xs font-medium">Final summary</p>
        <Button
          disabled={pending || loading}
          onClick={() => void generate()}
          size="sm"
          type="button"
          variant="outline"
        >
          <FileCheck2 aria-hidden="true" />
          {pending
            ? "Generating…"
            : document
              ? "Regenerate summary"
              : "Generate summary"}
        </Button>
      </div>
      {error && <SanitizedErrorPanel error={error} />}
      {!loading && !document && !error && (
        <p className="text-muted-foreground text-xs">
          No summary has been saved for this Session yet. Generating one writes
          <code className="mx-1">summary.md</code>
          next to its transcript.
        </p>
      )}
      {document && (
        <div aria-live="polite" className="flex flex-col gap-3">
          <div className="max-h-96 overflow-y-auto rounded-lg border p-3">
            <SanitizedMarkdown>{document.markdown}</SanitizedMarkdown>
          </div>
          <div className="flex flex-wrap gap-2">
            <Badge variant="outline">
              {document.segmentsIncluded} of {document.segmentsConsidered}{" "}
              segments used
            </Badge>
            <Badge variant="outline">{document.modelId}</Badge>
            <Badge variant="outline">
              Generated {new Date(document.generatedAt).toLocaleString()}
            </Badge>
          </div>
          {document.segmentsIncluded < document.segmentsConsidered && (
            <p className="text-muted-foreground text-xs">
              The transcript was larger than this model&rsquo;s context, so the
              summary used a sample spanning the whole Session plus its closing
              segments.
            </p>
          )}
        </div>
      )}
    </section>
  )
}
