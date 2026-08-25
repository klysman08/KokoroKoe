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
