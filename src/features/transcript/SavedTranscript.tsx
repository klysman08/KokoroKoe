import { Search, X } from "lucide-react"
import { useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { type Project, type Session } from "@/contracts/projects"
import { transcriptSearchRequestSchema } from "@/contracts/transcripts"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import {
  useSavedTranscript,
  useTranscriptSearch,
} from "@/features/transcript/use-saved-transcript"

export function SavedTranscript({
  project,
  session,
  onClose,
}: {
  project: Project
  session: Session
  onClose: () => void
}) {
  const [searchInput, setSearchInput] = useState("")
  const [searchQuery, setSearchQuery] = useState("")
  const [validationMessage, setValidationMessage] = useState<string>()
  const transcriptQuery = useSavedTranscript(project.id, session.id)
  const search = useTranscriptSearch(project.id, session.id, searchQuery)
  const segments =
    transcriptQuery.data?.pages.flatMap((page) => page.items) ?? []
  const hits = search.data?.pages.flatMap((page) => page.items) ?? []
  const showingSearch = searchQuery.length > 0
  const activeError = showingSearch ? search.error : transcriptQuery.error

  function submitSearch(event: React.FormEvent) {
    event.preventDefault()
    const query = searchInput.trim()
    if (query.length === 0) {
      setSearchQuery("")
      setValidationMessage(undefined)
      return
    }
    const parsed = transcriptSearchRequestSchema.safeParse({
      projectId: project.id,
      sessionId: session.id,
      query,
      limit: 20,
    })
    if (!parsed.success) {
      setValidationMessage(
        "Use 1–16 terms without control characters (256 UTF-8 bytes maximum).",
      )
      return
    }
    setValidationMessage(undefined)
    setSearchQuery(parsed.data.query)
  }

  return (
    <Card>
      <CardHeader className="flex-row items-start justify-between gap-4">
        <div>
          <CardTitle>{session.title} transcript</CardTitle>
          <CardDescription>
            Finalized local segments verified against the recovery journal.
          </CardDescription>
        </div>
        <Button onClick={onClose} size="sm" variant="outline">
          <X aria-hidden="true" /> Close transcript
        </Button>
      </CardHeader>
      <CardContent className="space-y-4">
        <form
          className="flex flex-col gap-2 sm:flex-row"
          onSubmit={submitSearch}
        >
          <label className="sr-only" htmlFor="saved-transcript-search">
            Search saved transcript
          </label>
          <input
            className="border-input bg-background min-h-9 flex-1 rounded-md border px-3 text-sm shadow-xs outline-none focus-visible:ring-2"
            id="saved-transcript-search"
            maxLength={256}
            onChange={(event) => setSearchInput(event.target.value)}
            placeholder="Search this saved transcript"
            value={searchInput}
          />
          <Button type="submit" variant="outline">
            <Search aria-hidden="true" /> Search
          </Button>
          {showingSearch && (
            <Button
              onClick={() => {
                setSearchInput("")
                setSearchQuery("")
                setValidationMessage(undefined)
              }}
              type="button"
              variant="ghost"
            >
              Clear
            </Button>
          )}
        </form>
        {validationMessage && (
          <p className="text-destructive text-sm" role="alert">
            {validationMessage}
          </p>
        )}
        {activeError && <SanitizedErrorPanel error={activeError} />}

        <div
          aria-label={
            showingSearch
              ? "Saved transcript search results"
              : "Saved transcript segments"
          }
          className="max-h-[58vh] space-y-3 overflow-y-auto rounded-xl border p-4"
        >
          {showingSearch ? (
            search.isPending ? (
              <EmptyMessage>Searching the local index…</EmptyMessage>
            ) : hits.length === 0 ? (
              <EmptyMessage>No matching finalized segments.</EmptyMessage>
            ) : (
              hits.map((hit) => (
                <article className="rounded-lg border p-3" key={hit.id}>
                  <SegmentHeader source={hit.source} startMs={hit.startMs} />
                  <p className="mt-2 text-sm leading-6 break-words whitespace-pre-wrap">
                    {hit.snippet}
                  </p>
                </article>
              ))
            )
          ) : transcriptQuery.isPending ? (
            <EmptyMessage>Loading the verified transcript…</EmptyMessage>
          ) : segments.length === 0 ? (
            <EmptyMessage>
              This saved transcript has no finalized segments.
            </EmptyMessage>
          ) : (
            segments.map((segment) => (
              <article className="rounded-lg border p-3" key={segment.id}>
                <SegmentHeader
                  language={segment.language}
                  source={segment.source}
                  startMs={segment.startMs}
                />
                <p className="mt-2 text-sm leading-6 break-words whitespace-pre-wrap">
                  {segment.text}
                </p>
              </article>
            ))
          )}
        </div>

        {(showingSearch ? search.hasNextPage : transcriptQuery.hasNextPage) && (
          <div className="flex justify-center">
            <Button
              disabled={
                showingSearch
                  ? search.isFetchingNextPage
                  : transcriptQuery.isFetchingNextPage
              }
              onClick={() =>
                void (showingSearch
                  ? search.fetchNextPage()
                  : transcriptQuery.fetchNextPage())
              }
              variant="outline"
            >
              Load more
            </Button>
          </div>
        )}
        <p className="text-muted-foreground text-xs">
          Transcript text is displayed as inert local text. Links, HTML, images,
          and embedded content are never activated.
        </p>
      </CardContent>
    </Card>
  )
}

function EmptyMessage({ children }: { children: React.ReactNode }) {
  return (
    <p className="text-muted-foreground py-10 text-center text-sm">
      {children}
    </p>
  )
}

function SegmentHeader({
  source,
  startMs,
  language,
}: {
  source: "microphone" | "system_output"
  startMs: number
  language?: string
}) {
  return (
    <div className="text-muted-foreground flex items-center gap-2 text-xs">
      <span className="text-foreground font-medium">
        {source === "microphone" ? "You" : "System"}
      </span>
      <span>{formatTime(startMs)}</span>
      {language && <Badge variant="outline">{language}</Badge>}
    </div>
  )
}

function formatTime(milliseconds: number) {
  const seconds = Math.floor(milliseconds / 1_000)
  const hours = Math.floor(seconds / 3_600)
  const minutes = Math.floor((seconds % 3_600) / 60)
  const remainder = seconds % 60
  return [hours, minutes, remainder]
    .map((value) => value.toString().padStart(2, "0"))
    .join(":")
}
