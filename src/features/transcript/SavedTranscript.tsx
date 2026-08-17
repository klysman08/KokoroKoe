import { MessageCircleQuestion, Search, ShieldCheck, X } from "lucide-react"
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
import { transcriptSearchRequestSchema } from "@/contracts/transcripts"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import {
  useSavedTranscript,
  useTranscriptSearch,
} from "@/features/transcript/use-saved-transcript"
import { askManualQuestion } from "@/lib/tauri/manual-question"

type QuestionTarget = {
  id: string
  source: "microphone" | "system_output"
  startMs: number
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
  const transcriptQuery = useSavedTranscript(project.id, session.id)
  const search = useTranscriptSearch(project.id, session.id, searchQuery)
  const segments =
    transcriptQuery.data?.pages.flatMap((page) => page.items) ?? []
  const hits = search.data?.pages.flatMap((page) => page.items) ?? []
  const showingSearch = searchQuery.length > 0
  const activeError = showingSearch ? search.error : transcriptQuery.error

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

        {questionTarget && (
          <ManualQuestionPanel
            key={`${project.id}:${session.id}:${questionTarget.id}`}
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
                    <AskSegmentButton
                      onClick={() =>
                        setQuestionTarget({
                          id: hit.id,
                          source: hit.source,
                          startMs: hit.startMs,
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
                  <AskSegmentButton
                    onClick={() =>
                      setQuestionTarget({
                        id: segment.id,
                        source: segment.source,
                        startMs: segment.startMs,
                      })
                    }
                  />
                </div>
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

function AskSegmentButton({ onClick }: { onClick: () => void }) {
  return (
    <Button onClick={onClick} size="xs" type="button" variant="ghost">
      <MessageCircleQuestion aria-hidden="true" data-icon="inline-start" />
      Ask about this segment
    </Button>
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
  const [question, setQuestion] = useState("")
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
