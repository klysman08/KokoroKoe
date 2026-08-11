import { useEffect, useRef, useState } from "react"
import { ArrowDown, Mic, Square } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  toApplicationError,
  type ApplicationError,
} from "@/contracts/app-error"
import { requestIdSchema, type RequestId } from "@/contracts/models"
import {
  type LiveTranscriptionStatus,
  type TranscriptionFinalEnvelope,
  type TranscriptionGapEnvelope,
  type TranscriptionPartialEnvelope,
} from "@/contracts/transcription"
import { SanitizedErrorPanel } from "@/features/errors/SanitizedErrorPanel"
import {
  reduceLiveRecords,
  type TranscriptRecord,
} from "@/features/transcript/live-transcript-records"
import {
  getLiveTranscriptionStatus,
  listenToTranscriptionFinals,
  listenToTranscriptionGaps,
  listenToTranscriptionPartials,
  startLiveTranscription,
  stopLiveTranscription,
} from "@/lib/tauri/transcription"

type OrderedTranscriptEvent =
  | TranscriptionPartialEnvelope
  | TranscriptionFinalEnvelope
  | TranscriptionGapEnvelope

export function LiveTranscriptPage() {
  const [consent, setConsent] = useState(false)
  const [status, setStatus] = useState<LiveTranscriptionStatus>({
    state: "idle",
  })
  const [records, setRecords] = useState<TranscriptRecord[]>([])
  const [error, setError] = useState<ApplicationError | null>(null)
  const requestId = useRef<RequestId | undefined>(undefined)
  const lastSequence = useRef(0)
  const latest = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let disposed = false
    const unlisteners: Array<() => void> = []
    const receive = (event: OrderedTranscriptEvent) => {
      if (
        event.requestId !== requestId.current ||
        event.sessionSequence <= lastSequence.current
      )
        return
      lastSequence.current = event.sessionSequence
      setRecords((current) => reduceLiveRecords(current, event))
    }
    void getLiveTranscriptionStatus()
      .then((current) => {
        if (disposed) return
        setStatus(current)
        requestId.current = current.requestId
      })
      .catch((caught: unknown) => {
        if (!disposed) setError(toApplicationError(caught))
      })
    for (const subscription of [
      listenToTranscriptionPartials(receive),
      listenToTranscriptionFinals(receive),
      listenToTranscriptionGaps(receive),
    ]) {
      void subscription
        .then((unlisten) =>
          disposed ? unlisten() : unlisteners.push(unlisten),
        )
        .catch((caught: unknown) => {
          if (!disposed) setError(toApplicationError(caught))
        })
    }
    return () => {
      disposed = true
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, [])

  const running = ["starting", "running", "stopping"].includes(status.state)

  async function start() {
    setError(null)
    const nextRequestId = requestIdSchema.parse(globalThis.crypto.randomUUID())
    requestId.current = nextRequestId
    lastSequence.current = 0
    setRecords([])
    setStatus({
      state: "starting",
      requestId: nextRequestId,
      startedAt: new Date().toISOString(),
    })
    try {
      setStatus(
        await startLiveTranscription(
          {
            acknowledgedCaptureConsent: true,
            microphoneSelection: { kind: "default", role: "communications" },
            systemOutputSelection: { kind: "default", role: "console" },
          },
          nextRequestId,
        ),
      )
    } catch (caught) {
      requestId.current = undefined
      setStatus({ state: "idle" })
      setError(toApplicationError(caught))
    }
  }

  async function stop() {
    if (!requestId.current) return
    setError(null)
    setStatus((current) => ({ ...current, state: "stopping" }))
    try {
      setStatus(await stopLiveTranscription(requestId.current))
    } catch (caught) {
      setError(toApplicationError(caught))
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <p className="text-muted-foreground text-sm font-medium">
          Local transcription
        </p>
        <h1 className="text-2xl font-semibold">Live transcript</h1>
        <p className="text-muted-foreground mt-2 max-w-3xl text-sm leading-6">
          This bounded preview captures your microphone and system output,
          transcribes locally, and keeps at most 500 transient records. It does
          not create a session, save audio, or persist transcript text.
        </p>
      </div>

      <Card>
        <CardHeader className="flex-row items-start justify-between gap-4">
          <div>
            <CardTitle>Capture controls</CardTitle>
            <CardDescription>
              Uses the default communications microphone and default console
              output.
            </CardDescription>
          </div>
          <Badge variant={running ? "secondary" : "outline"}>
            {status.state}
          </Badge>
        </CardHeader>
        <CardContent className="space-y-4">
          <label className="flex items-start gap-3 text-sm">
            <input
              checked={consent}
              className="mt-1"
              disabled={running}
              onChange={(event) => setConsent(event.target.checked)}
              type="checkbox"
            />
            <span>
              I understand that starting captures microphone and system audio
              for local, transient transcription.
            </span>
          </label>
          {running ? (
            <Button
              disabled={status.state === "stopping"}
              onClick={() => void stop()}
              type="button"
              variant="outline"
            >
              <Square aria-hidden="true" /> Stop transcription
            </Button>
          ) : (
            <Button
              disabled={!consent}
              onClick={() => void start()}
              type="button"
            >
              <Mic aria-hidden="true" /> Start transcription
            </Button>
          )}
        </CardContent>
      </Card>

      {error && <SanitizedErrorPanel error={error} />}

      <Card>
        <CardHeader className="flex-row items-center justify-between gap-4">
          <div>
            <CardTitle>Transcript preview</CardTitle>
            <CardDescription aria-live="polite">
              {records.length} transient records
            </CardDescription>
          </div>
          <Button
            onClick={() =>
              latest.current?.scrollIntoView({ behavior: "smooth" })
            }
            size="sm"
            type="button"
            variant="outline"
          >
            <ArrowDown aria-hidden="true" /> Return to latest
          </Button>
        </CardHeader>
        <CardContent>
          <div
            className="max-h-[52vh] space-y-3 overflow-y-auto rounded-lg border p-4"
            aria-label="Live transcript records"
          >
            {records.length === 0 && (
              <p className="text-muted-foreground py-10 text-center text-sm">
                No transcript received yet.
              </p>
            )}
            {records.map((record) =>
              record.kind === "segment" ? (
                <article className="space-y-1" key={record.segment.id}>
                  <div className="text-muted-foreground flex items-center gap-2 text-xs">
                    <span className="text-foreground font-medium">
                      {sourceLabel(record.segment.source)}
                    </span>
                    <span>{formatTime(record.segment.startMs)}</span>
                    {record.segment.status === "partial" && (
                      <Badge variant="outline">partial</Badge>
                    )}
                  </div>
                  <p className="text-sm leading-6 break-words whitespace-pre-wrap">
                    {record.segment.text}
                  </p>
                </article>
              ) : (
                <p
                  className="text-muted-foreground rounded-md border border-dashed px-3 py-2 text-xs"
                  key={record.id}
                >
                  {sourceLabel(record.source)} audio gap at{" "}
                  {formatTime(record.startMs)} (
                  {record.code.replaceAll("_", " ")})
                </p>
              ),
            )}
            <div ref={latest} />
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

function sourceLabel(source: "microphone" | "system_output") {
  return source === "microphone" ? "You" : "System"
}

function formatTime(milliseconds: number) {
  const seconds = Math.floor(milliseconds / 1_000)
  return `${Math.floor(seconds / 60)
    .toString()
    .padStart(2, "0")}:${(seconds % 60).toString().padStart(2, "0")}`
}
