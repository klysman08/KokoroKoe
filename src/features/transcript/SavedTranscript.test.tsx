import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import projectSessionFixture from "../../../fixtures/contracts/project-session-v1.json"
import manualQuestionFixture from "../../../fixtures/contracts/manual-question-v1.json"
import transcriptFixture from "../../../fixtures/contracts/transcript-reading-v1.json"
import {
  askManualQuestionRequestSchema,
  manualQuestionFixtureSchema,
} from "@/contracts/manual-question"
import { projectSessionFixtureSchema } from "@/contracts/projects"
import {
  annotateTranscriptSegmentRequestSchema,
  transcriptSegmentHistoryRequestSchema,
} from "@/contracts/transcripts"
import { SEGMENT_QUESTION_PRESETS, segmentAsText } from "./segment-actions"
import { SavedTranscript } from "./SavedTranscript"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const scope = projectSessionFixtureSchema.parse(projectSessionFixture)
const manualQuestion = manualQuestionFixtureSchema.parse(manualQuestionFixture)
const invokeMock = vi.mocked(invoke)

function scopedPage() {
  return {
    items: transcriptFixture.page.items.map((item) => ({
      ...item,
      projectId: scope.project.id,
      sessionId: scope.session.id,
    })),
  }
}

/** A segment corrected twice: the middle wording is only in the journal. */
function correctedSegment() {
  return {
    ...scopedPage().items[0]!,
    text: "KokoroKoe is local",
    originalText: "kokoro co is local",
  }
}

function correctedPage() {
  return { items: [correctedSegment()] }
}

function historyResponses(command: string, args: unknown) {
  if (command === "get_transcript_page") return correctedPage()
  if (command === "get_transcript_segment_history") {
    const request = transcriptSegmentHistoryRequestSchema.parse(
      (args as { request: unknown }).request,
    )
    return {
      ...request,
      originalText: "kokoro co is local",
      revisions: [
        { recordedAt: "2026-08-12T10:04:00Z", text: "Kokoro Koe is local" },
        { recordedAt: "2026-08-12T10:06:00Z", text: "KokoroKoe is local" },
      ],
    }
  }
  throw new Error(`Unexpected command: ${command}`)
}

function renderTranscript() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  return render(
    <QueryClientProvider client={client}>
      <SavedTranscript
        onClose={vi.fn()}
        project={scope.project}
        session={scope.session}
      />
    </QueryClientProvider>,
  )
}

describe("SavedTranscript", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "get_transcript_page") return scopedPage()
      if (command === "search_transcript")
        return {
          items: [
            {
              ...transcriptFixture.searchPage.items[0],
              projectId: scope.project.id,
              sessionId: scope.session.id,
              snippet: "<img src=x onerror=alert(1)> authoritative",
            },
          ],
        }
      if (command === "ask_manual_question") {
        const request = askManualQuestionRequestSchema.parse(
          (args as { request: unknown }).request,
        )
        return {
          ...manualQuestion.response,
          requestId: request.requestId,
          projectId: request.projectId,
          sessionId: request.sessionId,
          selectedSegmentId: request.selectedSegmentId,
          answer: "<img src=x onerror=alert(1)> Keep the transcript local.",
          limitations: ["<script>untrusted model text</script>"],
        }
      }
      throw new Error(`Unexpected command: ${command}`)
    })
  })

  it("reads the exact project/session scope and renders transcript as inert text", async () => {
    renderTranscript()
    expect(
      await screen.findByText(
        "We should keep the authoritative transcript local.",
      ),
    ).toBeInTheDocument()
    expect(invokeMock).toHaveBeenCalledWith("get_transcript_page", {
      projectId: scope.project.id,
      sessionId: scope.session.id,
      limit: 50,
    })
    expect(document.querySelector("img")).toBeNull()
  })

  it("searches only within the selected session and never activates returned markup", async () => {
    const user = userEvent.setup()
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )
    await user.type(
      screen.getByRole("textbox", { name: "Search saved transcript" }),
      "authoritative",
    )
    await user.click(screen.getByRole("button", { name: "Search" }))
    expect(
      await screen.findByText("<img src=x onerror=alert(1)> authoritative"),
    ).toBeInTheDocument()
    expect(document.querySelector("img")).toBeNull()
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("search_transcript", {
        projectId: scope.project.id,
        sessionId: scope.session.id,
        query: "authoritative",
        limit: 20,
      }),
    )
  })

  it("asks from an explicit segment and renders only validated inert answer text", async () => {
    const user = userEvent.setup()
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )

    await user.click(
      screen.getAllByRole("button", { name: "Ask about this segment" })[0]!,
    )
    expect(screen.getByText("What leaves this device")).toBeInTheDocument()
    expect(screen.getByText(/It never sends audio/)).toBeInTheDocument()
    await user.type(
      screen.getByRole("textbox", { name: "Question" }),
      "What decision was made?",
    )
    await user.click(screen.getByRole("button", { name: "Ask OpenRouter" }))

    expect(
      await screen.findByText(
        "<img src=x onerror=alert(1)> Keep the transcript local.",
      ),
    ).toBeInTheDocument()
    expect(
      screen.getByText("<script>untrusted model text</script>"),
    ).toBeInTheDocument()
    expect(document.querySelector("img")).toBeNull()
    expect(document.querySelector("script")).toBeNull()
    expect(invokeMock).toHaveBeenCalledWith("ask_manual_question", {
      request: {
        requestId: expect.any(String),
        projectId: scope.project.id,
        sessionId: scope.session.id,
        selectedSegmentId: scopedPage().items[0]!.id,
        question: "What decision was made?",
      },
    })
  })

  it("keeps the action pending and omits unexpected provider details from errors", async () => {
    const user = userEvent.setup()
    let rejectQuestion!: (reason: unknown) => void
    const pendingQuestion = new Promise((_, reject) => {
      rejectQuestion = reject
    })
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_transcript_page") return scopedPage()
      if (command === "ask_manual_question") return pendingQuestion
      throw new Error(`Unexpected command: ${command}`)
    })
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )
    await user.click(
      screen.getAllByRole("button", { name: "Ask about this segment" })[0]!,
    )
    await user.type(
      screen.getByRole("textbox", { name: "Question" }),
      "What decision was made?",
    )
    await user.click(screen.getByRole("button", { name: "Ask OpenRouter" }))

    expect(
      screen.getByRole("button", { name: "Asking OpenRouter…" }),
    ).toBeDisabled()
    expect(
      screen.getByText("Generating a bounded, validated answer."),
    ).toBeInTheDocument()
    rejectQuestion(new Error("provider-secret and C:\\private\\meeting.md"))

    expect(
      await screen.findByText(
        "KokoroKoe encountered an unexpected application error.",
      ),
    ).toBeInTheDocument()
    expect(document.body).not.toHaveTextContent("provider-secret")
    expect(document.body).not.toHaveTextContent("private\\meeting.md")
  })

  /// A preset fills the question box and stops there. Meeting text must never
  /// leave the machine because the user pressed a one-click label.
  it("prefills a preset question without sending anything", async () => {
    const user = userEvent.setup()
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )
    invokeMock.mockClear()

    await user.click(screen.getAllByRole("button", { name: "Explain" })[0]!)

    const box = await screen.findByRole("textbox", { name: "Question" })
    expect(box).toHaveValue(
      SEGMENT_QUESTION_PRESETS.find((preset) => preset.id === "explain")!
        .question,
    )
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ask_manual_question",
      expect.anything(),
    )
  })

  it("replaces the prefilled question when another preset is chosen", async () => {
    const user = userEvent.setup()
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )

    await user.click(screen.getAllByRole("button", { name: "Explain" })[0]!)
    await screen.findByRole("textbox", { name: "Question" })
    await user.click(screen.getAllByRole("button", { name: "Summarize" })[0]!)

    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Question" })).toHaveValue(
        SEGMENT_QUESTION_PRESETS.find((preset) => preset.id === "summarize")!
          .question,
      ),
    )
  })

  it("leaves the question box empty when the user writes their own", async () => {
    const user = userEvent.setup()
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )

    await user.click(
      screen.getAllByRole("button", { name: "Ask about this segment" })[0]!,
    )

    expect(
      await screen.findByRole("textbox", { name: "Question" }),
    ).toHaveValue("")
  })

  it("copies a segment as speaker, timecode, and text", async () => {
    const user = userEvent.setup()
    // `userEvent.setup()` installs its own clipboard stub, so the double must
    // be defined after it or it is immediately replaced.
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    })
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )

    await user.click(screen.getAllByRole("button", { name: "Copy" })[0]!)

    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        segmentAsText({
          source: "microphone",
          startMs: 1_200,
          text: "We should keep the authoritative transcript local.",
        }),
      ),
    )
    expect(await screen.findByText("Copied")).toBeInTheDocument()
  })

  /// The decision this feature rests on: a correction changes what the
  /// transcript reads as and never what it recorded, so the transcription has
  /// to stay on screen afterwards.
  it("corrects a segment and keeps the original readable", async () => {
    const user = userEvent.setup()
    const segment = scopedPage().items[0]!
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "get_transcript_page") return scopedPage()
      if (command === "annotate_transcript_segment") {
        const request = annotateTranscriptSegmentRequestSchema.parse(
          (args as { request: unknown }).request,
        )
        expect(request.annotation).toEqual({
          kind: "correction",
          text: "KokoroKoe stays local.",
        })
        return {
          ...segment,
          text: "KokoroKoe stays local.",
          originalText: segment.text,
        }
      }
      throw new Error(`Unexpected command: ${command}`)
    })
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )

    await user.click(screen.getAllByRole("button", { name: "Correct" })[0]!)
    const box = await screen.findByRole("textbox", { name: "Corrected text" })
    expect(box).toHaveValue(segment.text)
    await user.clear(box)
    await user.type(box, "KokoroKoe stays local.")
    await user.click(screen.getByRole("button", { name: /save correction/i }))

    expect(
      await screen.findByText("KokoroKoe stays local."),
    ).toBeInTheDocument()
    expect(
      screen.getByText(/show what this segment said before/i),
    ).toBeInTheDocument()
    expect(screen.getByText(segment.text)).toBeInTheDocument()
  })

  /// The point of the history: the document only carries the current reading
  /// and the transcription, so a wording that was corrected twice exists
  /// nowhere but the journal. Opening the disclosure is what asks for it.
  it("reads the journal on demand and shows wordings the document dropped", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command, args) =>
      historyResponses(command, args),
    )
    renderTranscript()
    await screen.findByText("KokoroKoe is local")
    // Nothing is read until the reader asks for it.
    expect(invokeMock).not.toHaveBeenCalledWith(
      "get_transcript_segment_history",
      expect.anything(),
    )

    await user.click(screen.getByText(/show what this segment said before/i))

    expect(await screen.findByText("Kokoro Koe is local")).toBeInTheDocument()
    expect(invokeMock).toHaveBeenCalledWith("get_transcript_segment_history", {
      request: {
        projectId: scope.project.id,
        sessionId: scope.session.id,
        segmentId: correctedSegment().id,
      },
    })
    expect(screen.getByText("kokoro co is local")).toBeInTheDocument()
    // The newest revision is the segment's own text, already on screen above.
    expect(screen.getAllByText("KokoroKoe is local")).toHaveLength(1)
  })

  /// Putting an old wording back is itself a correction, so it fills the box
  /// and waits to be read rather than saving itself.
  it("restores an earlier wording into the correction box without saving it", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command, args) =>
      historyResponses(command, args),
    )
    renderTranscript()
    await screen.findByText("KokoroKoe is local")
    await user.click(screen.getByText(/show what this segment said before/i))
    await screen.findByText("Kokoro Koe is local")

    await user.click(
      screen.getAllByRole("button", { name: /use this wording/i })[0]!,
    )

    expect(
      await screen.findByRole("textbox", { name: "Corrected text" }),
    ).toHaveValue("kokoro co is local")
    expect(invokeMock).not.toHaveBeenCalledWith(
      "annotate_transcript_segment",
      expect.anything(),
    )
  })

  it("surfaces a sanitized failure when the history cannot be read", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_transcript_page") return correctedPage()
      throw {
        error: {
          code: "transcript_segment_not_found",
          userMessage:
            "That transcript segment is no longer part of this session.",
          technicalDetail: "transcript_segment_not_found",
          severity: "info",
          retryable: false,
          correlationId: "ffffffff-ffff-4fff-8fff-ffffffffffff",
        },
      }
    })
    renderTranscript()
    await screen.findByText("KokoroKoe is local")

    await user.click(screen.getByText(/show what this segment said before/i))

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /no longer part of this session/i,
    )
    // The transcription the segment already carries stays readable regardless.
    expect(screen.getByText("kokoro co is local")).toBeInTheDocument()
  })

  it("marks a segment important and can take it back", async () => {
    const user = userEvent.setup()
    const segment = scopedPage().items[0]!
    let important = false
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "get_transcript_page") return scopedPage()
      if (command === "annotate_transcript_segment") {
        const request = annotateTranscriptSegmentRequestSchema.parse(
          (args as { request: unknown }).request,
        )
        if (request.annotation.kind !== "importance")
          throw new Error("expected an importance annotation")
        important = request.annotation.important
        return { ...segment, important }
      }
      throw new Error(`Unexpected command: ${command}`)
    })
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )

    await user.click(
      screen.getAllByRole("button", { name: /mark important/i })[0]!,
    )

    const marked = await screen.findByRole("button", { name: "Important" })
    expect(marked).toHaveAttribute("aria-pressed", "true")
    await user.click(marked)
    await waitFor(() => expect(important).toBe(false))
  })

  it("surfaces a sanitized failure when a correction is refused", async () => {
    const user = userEvent.setup()
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_transcript_page") return scopedPage()
      throw {
        error: {
          code: "transcript_segment_not_found",
          userMessage: "KokoroKoe could not find that transcript segment.",
          technicalDetail: "transcript_segment_not_found",
          severity: "warning",
          retryable: false,
          correlationId: "ffffffff-ffff-4fff-8fff-ffffffffffff",
        },
      }
    })
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )

    await user.click(
      screen.getAllByRole("button", { name: /mark important/i })[0]!,
    )

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /could not find that transcript segment/i,
    )
  })

  it("reports a refused clipboard instead of claiming a copy", async () => {
    const user = userEvent.setup()
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
    })
    renderTranscript()
    await screen.findByText(
      "We should keep the authoritative transcript local.",
    )

    await user.click(screen.getAllByRole("button", { name: "Copy" })[0]!)

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The clipboard is not available.",
    )
    expect(screen.queryByText("Copied")).toBeNull()
  })
})
