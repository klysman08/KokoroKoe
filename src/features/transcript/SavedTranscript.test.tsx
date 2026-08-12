import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"

import projectSessionFixture from "../../../fixtures/contracts/project-session-v1.json"
import transcriptFixture from "../../../fixtures/contracts/transcript-reading-v1.json"
import { projectSessionFixtureSchema } from "@/contracts/projects"
import { SavedTranscript } from "./SavedTranscript"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))

const scope = projectSessionFixtureSchema.parse(projectSessionFixture)
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
    invokeMock.mockImplementation(async (command) => {
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
})
