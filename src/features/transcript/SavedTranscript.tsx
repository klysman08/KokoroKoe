import {
  Copy,
  MessageCircleQuestion,
  PencilLine,
  Search,
  ShieldCheck,
  Star,
  X,
} from "lucide-react"
import { type FormEvent, useState } from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import { Textarea } from "@/components/ui/textarea"
import {
  type ApplicationError,
  toApplicationError,
} from "@/contracts/app-error"
import {
  askManualQuestionRequestSchema,
  type ManualQuestionResponse,
} from "@/contracts/manual-question"
import { type Project, type Session } from "@/contracts/projects"
import {
  annotateTranscriptSegmentRequestSchema,
  transcriptSearchRequestSchema,
  type SegmentAnnotation,
  type TranscriptSegment,
} from "@/contracts/transcripts"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import { cn } from "@/lib/utils"
import {
  useSavedTranscript,
  useTranscriptSearch,
} from "@/features/transcript/use-saved-transcript"
import {
  SEGMENT_QUESTION_PRESETS,
  segmentAsText,
} from "@/features/transcript/segment-actions"
import { askManualQuestion } from "@/lib/tauri/manual-question"
import { annotateTranscriptSegment } from "@/lib/tauri/transcripts"

type QuestionTarget = {
  id: string
  source: "microphone" | "system_output"
  startMs: number
  /** Prefilled question text. Empty when the user asked to write their own. */
  question: string
}

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
  const [questionTarget, setQuestionTarget] = useState<QuestionTarget>()
  const [copiedSegmentId, setCopiedSegmentId] = useState<string>()
  const [correctingId, setCorrectingId] = useState<string>()
  const [annotatingId, setAnnotatingId] = useState<string>()
  const [annotationError, setAnnotationError] = useState<ApplicationError>()
  const [annotations, setAnnotations] = useState<
    Record<string, TranscriptSegment>
  >({})
  const transcriptQuery = useSavedTranscript(project.id, session.id)
  const search = useTranscriptSearch(project.id, session.id, searchQuery)
  const segments =
    transcriptQuery.data?.pages.flatMap((page) => page.items) ?? []
  const hits = search.data?.pages.flatMap((page) => page.items) ?? []
  const showingSearch = searchQuery.length > 0
  const activeError = showingSearch ? search.error : transcriptQuery.error

  /**
   * The segment as it now stands.
   *
   * An annotated segment is held locally until the next refetch so the change
   * is visible immediately; Rust remains the authority, and this only ever
   * holds what Rust returned.
   */
  function annotated(segment: TranscriptSegment): TranscriptSegment {
    return annotations[segment.id] ?? segment
  }

  async function annotate(segmentId: string, annotation: SegmentAnnotation) {
    const request = annotateTranscriptSegmentRequestSchema.safeParse({
      projectId: project.id,
      sessionId: session.id,
      segmentId,
      annotation,
    })
    if (!request.success) {
      setValidationMessage("That correction cannot be saved as written.")
      return
    }
    setValidationMessage(undefined)
    setAnnotatingId(segmentId)
    try {
      const updated = await annotateTranscriptSegment(request.data)
      setAnnotations((current) => ({ ...current, [segmentId]: updated }))
      setCorrectingId(undefined)
    } catch (caught: unknown) {
      setAnnotationError(toApplicationError(caught))
    } finally {
      setAnnotatingId(undefined)
    }
  }

  async function copySegment(segment: {
    id: string
    source: "microphone" | "system_output"
    startMs: number
    text: string
  }) {
    try {
      await navigator.clipboard.writeText(segmentAsText(segment))
      setCopiedSegmentId(segment.id)
    } catch {
      setCopiedSegmentId(undefined)
      setValidationMessage("The clipboard is not available.")
    }
  }

  function submitSearch(event: FormEvent) {
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
      <CardHeader>
        <div>
          <CardTitle>{session.title} transcript</CardTitle>
          <CardDescription>
            Finalized local segments verified against the recovery journal.
          </CardDescription>
        </div>
        <CardAction>
          <Button onClick={onClose} size="sm" variant="outline">
            <X aria-hidden="true" data-icon="inline-start" /> Close transcript
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <form
          className="flex flex-col gap-2 sm:flex-row"
          onSubmit={submitSearch}
        >
          <label className="sr-only" htmlFor="saved-transcript-search">
            Search saved transcript
          </label>
          <Input
            className="min-h-9 flex-1"
            id="saved-transcript-search"
            maxLength={256}
            onChange={(event) => setSearchInput(event.target.value)}
            placeholder="Search this saved transcript"
            value={searchInput}
          />
          <Button type="submit" variant="outline">
            <Search aria-hidden="true" data-icon="inline-start" /> Search
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
        {annotationError && <SanitizedErrorPanel error={annotationError} />}

        {questionTarget && (
          <ManualQuestionPanel
            // Remounting on a new target or preset is what puts the prefilled
            // question in the box; an open panel must not keep stale text.
            key={`${project.id}:${session.id}:${questionTarget.id}:${questionTarget.question}`}
            onClose={() => setQuestionTarget(undefined)}
            project={project}
            session={session}
            target={questionTarget}
          />
        )}

        <div
          aria-label={
            showingSearch
              ? "Saved transcript search results"
              : "Saved transcript segments"
          }
          className="flex max-h-[58vh] flex-col gap-3 overflow-y-auto rounded-xl border p-4"
        >
          {showingSearch ? (
            search.isPending ? (
              <EmptyMessage>Searching the local index…</EmptyMessage>
            ) : hits.length === 0 ? (
              <EmptyMessage>No matching finalized segments.</EmptyMessage>
            ) : (
              hits.map((hit) => (
                <article
                  className="flex flex-col gap-2 rounded-lg border p-3"
                  key={hit.id}
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <SegmentHeader source={hit.source} startMs={hit.startMs} />
                    <SegmentActions
                      // A search hit shows a snippet with match markers, not the
                      // segment's own text, so copying it would quote something
                      // the transcript does not actually say.
                      copied={false}
                      onAsk={(question) =>
                        setQuestionTarget({
                          id: hit.id,
                          source: hit.source,
                          startMs: hit.startMs,
                          question,
                        })
                      }
                    />
                  </div>
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
              <article
                className="flex flex-col gap-2 rounded-lg border p-3"
                key={segment.id}
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <SegmentHeader
                    language={segment.language}
                    source={segment.source}
                    startMs={segment.startMs}
                  />
                  <SegmentActions
                    copied={copiedSegmentId === segment.id}
                    onAsk={(question) =>
                      setQuestionTarget({
                        id: segment.id,
                        source: segment.source,
                        startMs: segment.startMs,
                        question,
                      })
                    }
                    important={annotated(segment).important === true}
                    onCopy={() => void copySegment(segment)}
                    onCorrect={() => setCorrectingId(segment.id)}
                    onMark={() =>
                      void annotate(segment.id, {
                        kind: "importance",
                        important: annotated(segment).important !== true,
                      })
                    }
                  />
                </div>
                {correctingId === segment.id ? (
                  <SegmentCorrectionForm
                    initialText={annotated(segment).text}
                    onCancel={() => setCorrectingId(undefined)}
                    onSubmit={(text) =>
                      void annotate(segment.id, { kind: "correction", text })
                    }
                    pending={annotatingId === segment.id}
                  />
                ) : (
                  <p className="mt-2 text-sm leading-6 break-words whitespace-pre-wrap">
                    {annotated(segment).text}
                  </p>
                )}
                {annotated(segment).originalText && (
                  // A correction changes what the transcript reads as, never
                  // what it recorded, so the transcription stays on screen.
                  <details className="text-muted-foreground text-xs">
                    <summary className="text-foreground cursor-pointer">
                      Corrected — show the original transcription
                    </summary>
                    <p className="mt-1 leading-6 break-words whitespace-pre-wrap">
                      {annotated(segment).originalText}
                    </p>
                  </details>
                )}
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

/**
 * Per-segment actions.
 *
 * Copying is view-local. Every question action only opens the question box with
 * text already filled in: nothing reaches OpenRouter until the user reads it
 * and presses Ask, which is the point of prefilling rather than sending.
 */
function SegmentActions({
  copied,
  important,
  onAsk,
  onCopy,
  onCorrect,
  onMark,
}: {
  copied: boolean
  important?: boolean
  onAsk: (question: string) => void
  onCopy?: () => void
  onCorrect?: () => void
  onMark?: () => void
}) {
  return (
    <div className="flex flex-wrap items-center gap-1">
      {onMark && (
        <Button
          aria-pressed={important === true}
          className={cn(important && "border-primary")}
          onClick={onMark}
          size="xs"
          type="button"
          variant="ghost"
        >
          <Star aria-hidden="true" data-icon="inline-start" />
          {important ? "Important" : "Mark important"}
        </Button>
      )}
      {onCorrect && (
        <Button onClick={onCorrect} size="xs" type="button" variant="ghost">
          <PencilLine aria-hidden="true" data-icon="inline-start" /> Correct
        </Button>
      )}
      {onCopy && (
        <Button onClick={onCopy} size="xs" type="button" variant="ghost">
          <Copy aria-hidden="true" data-icon="inline-start" />
          {copied ? "Copied" : "Copy"}
        </Button>
      )}
      {SEGMENT_QUESTION_PRESETS.map((preset) => (
        <Button
          key={preset.id}
          onClick={() => onAsk(preset.question)}
          size="xs"
          type="button"
          variant="ghost"
        >
          {preset.label}
        </Button>
      ))}
      <Button onClick={() => onAsk("")} size="xs" type="button" variant="ghost">
        <MessageCircleQuestion aria-hidden="true" data-icon="inline-start" />
        Ask about this segment
      </Button>
    </div>
  )
}

function ManualQuestionPanel({
  project,
  session,
  target,
  onClose,
}: {
  project: Project
  session: Session
  target: QuestionTarget
  onClose: () => void
}) {
  const [question, setQuestion] = useState(target.question)
  const [response, setResponse] = useState<ManualQuestionResponse>()
  const [error, setError] = useState<ApplicationError>()
  const [validationMessage, setValidationMessage] = useState<string>()
  const [pending, setPending] = useState(false)

  async function submitQuestion(event: FormEvent) {
    event.preventDefault()
    const request = askManualQuestionRequestSchema.safeParse({
      requestId: globalThis.crypto.randomUUID(),
      projectId: project.id,
      sessionId: session.id,
      selectedSegmentId: target.id,
      question: question.trim(),
    })
    if (!request.success) {
      setValidationMessage(
        "Enter a question without unsupported control characters (4,096 UTF-8 bytes maximum).",
      )
      return
    }

    setValidationMessage(undefined)
    setError(undefined)
    setResponse(undefined)
    setPending(true)
    try {
      setResponse(await askManualQuestion(request.data))
    } catch (caught: unknown) {
      setError(toApplicationError(caught))
    } finally {
      setPending(false)
    }
  }

  function changeQuestion(value: string) {
    setQuestion(value)
    setResponse(undefined)
    setError(undefined)
    setValidationMessage(undefined)
  }

  return (
    <Card aria-label="Ask about a transcript segment" size="sm">
      <CardHeader>
        <CardTitle>Ask about this moment</CardTitle>
        <CardDescription>
          {target.source === "microphone" ? "You" : "System"} at{" "}
          {formatTime(target.startMs)}
        </CardDescription>
        <CardAction>
          <Button
            disabled={pending}
            onClick={onClose}
            size="sm"
            type="button"
            variant="ghost"
          >
            Close question
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <Alert>
          <ShieldCheck aria-hidden="true" />
          <AlertTitle>What leaves this device</AlertTitle>
          <AlertDescription>
            KokoroKoe sends the selected finalized segment, up to four nearby
            segments on each side, your question, and the session&apos;s saved
            model instructions to OpenRouter. It never sends audio.
          </AlertDescription>
        </Alert>

        <form className="flex flex-col gap-4" onSubmit={submitQuestion}>
          <FieldGroup>
            <Field
              data-disabled={pending || undefined}
              data-invalid={!!validationMessage}
            >
              <FieldLabel htmlFor="manual-transcript-question">
                Question
              </FieldLabel>
              <Textarea
                aria-describedby="manual-question-description"
                aria-invalid={!!validationMessage}
                data-disabled={pending || undefined}
                disabled={pending}
                id="manual-transcript-question"
                maxLength={4096}
                onChange={(event) => changeQuestion(event.target.value)}
                placeholder="What decision or action relates to this moment?"
                rows={3}
                value={question}
              />
              <FieldDescription id="manual-question-description">
                The backend rebuilds the context from the verified local
                transcript; search snippets and visible HTML are never trusted.
              </FieldDescription>
              {validationMessage && (
                <FieldError>{validationMessage}</FieldError>
              )}
            </Field>
          </FieldGroup>
          <div className="flex flex-wrap items-center gap-3">
            <Button
              data-disabled={pending || undefined}
              disabled={pending}
              type="submit"
            >
              {pending ? (
                <>
                  <Spinner aria-hidden="true" data-icon="inline-start" /> Asking
                  OpenRouter…
                </>
              ) : (
                <>
                  <MessageCircleQuestion
                    aria-hidden="true"
                    data-icon="inline-start"
                  />
                  Ask OpenRouter
                </>
              )}
            </Button>
            <span aria-live="polite" className="text-muted-foreground text-xs">
              {pending && "Generating a bounded, validated answer."}
            </span>
          </div>
        </form>

        {error && <SanitizedErrorPanel error={error} />}
        {response && <ManualQuestionAnswer response={response} />}
      </CardContent>
    </Card>
  )
}

function ManualQuestionAnswer({
  response,
}: {
  response: ManualQuestionResponse
}) {
  return (
    <Card aria-live="polite" size="sm">
      <CardHeader>
        <CardTitle>Answer</CardTitle>
        <CardDescription>
          Validated structured text from the selected local context.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <p className="text-sm leading-6 break-words whitespace-pre-wrap">
          {response.answer}
        </p>
        {response.limitations.length > 0 && (
          <div className="flex flex-col gap-2">
            <p className="text-sm font-medium">Limitations</p>
            <ul className="text-muted-foreground flex list-disc flex-col gap-1 pl-5 text-sm">
              {response.limitations.map((limitation) => (
                <li key={limitation}>{limitation}</li>
              ))}
            </ul>
          </div>
        )}
      </CardContent>
      <CardFooter className="flex flex-wrap gap-2">
        <Badge variant="outline">
          {response.primaryUsage.inputTokens +
            response.primaryUsage.outputTokens}{" "}
          tokens
        </Badge>
        <Badge variant="outline">
          {response.primaryAttempts} primary attempt
          {response.primaryAttempts === 1 ? "" : "s"}
        </Badge>
        {response.repaired && <Badge variant="secondary">JSON repaired</Badge>}
        <Badge variant="outline">
          ${response.sessionActualCostUsd} session cost
        </Badge>
      </CardFooter>
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

/**
 * Rewrites one finalized segment.
 *
 * The box opens with the current text so a correction is an edit of what is
 * there, not a retype. Saving records an event; it never overwrites what was
 * transcribed, which stays visible beneath the segment afterwards.
 */
function SegmentCorrectionForm({
  initialText,
  onCancel,
  onSubmit,
  pending,
}: {
  initialText: string
  onCancel: () => void
  onSubmit: (text: string) => void
  pending: boolean
}) {
  const [text, setText] = useState(initialText)
  const unchanged = text.trim() === initialText.trim()

  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={(event) => {
        event.preventDefault()
        onSubmit(text)
      }}
    >
      <label className="sr-only" htmlFor="segment-correction">
        Corrected text
      </label>
      <Textarea
        autoFocus
        disabled={pending}
        id="segment-correction"
        maxLength={32768}
        onChange={(event) => setText(event.target.value)}
        rows={3}
        value={text}
      />
      <p className="text-muted-foreground text-xs">
        The transcription is kept. It stays in the recovery journal and remains
        readable beneath this segment and in <code>transcript.md</code>.
      </p>
      <div className="flex flex-wrap gap-2">
        <Button
          disabled={pending || unchanged || text.trim().length === 0}
          size="sm"
          type="submit"
        >
          {pending ? "Saving…" : "Save correction"}
        </Button>
        <Button
          disabled={pending}
          onClick={onCancel}
          size="sm"
          type="button"
          variant="ghost"
        >
          Cancel
        </Button>
      </div>
    </form>
  )
}
