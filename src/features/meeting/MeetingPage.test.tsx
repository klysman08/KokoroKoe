import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { vi } from "vitest"

import recordsFixture from "../../../fixtures/contracts/project-session-v1.json"

import { MeetingPage } from "@/features/meeting/MeetingPage"
import { useActiveSessionStore } from "@/stores/active-session-store"
import { useMeetingStore } from "@/stores/meeting-store"
import { useNavigationStore } from "@/stores/navigation-store"

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }))
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }))

const invokeMock = vi.mocked(invoke)
const live = { ...recordsFixture.session, state: "transcribing" }
const paused = { ...live, state: "paused", revision: live.revision + 1 }

function renderMeeting() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <MeetingPage />
    </QueryClientProvider>,
  )
}

describe("MeetingPage", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    vi.mocked(listen).mockResolvedValue(() => undefined)
    useActiveSessionStore.setState({ activeSession: undefined })
    useMeetingStore.setState({ focus: undefined })
    useNavigationStore.setState({ activeRoute: "meeting" })
  })

  it("points back to Home when no meeting is running", async () => {
    const user = userEvent.setup()
    renderMeeting()

    expect(
      screen.getByRole("heading", { name: "No meeting is running" }),
    ).toBeInTheDocument()
    await user.click(screen.getByRole("button", { name: "Start a meeting" }))
    expect(useNavigationStore.getState().activeRoute).toBe("home")
  })

  it("shows the live meeting with its controls, and pauses it", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_project") return recordsFixture.project
      if (command === "get_session") return live
      if (command === "pause_session") return paused
      throw new Error(`Unexpected command: ${command}`)
    })
    useMeetingStore.setState({
      focus: {
        projectId: recordsFixture.project.id as never,
        sessionId: live.id as never,
      },
    })
    const user = userEvent.setup()
    renderMeeting()

    expect(
      await screen.findByRole("heading", { name: live.title }),
    ).toBeInTheDocument()
    expect(screen.getByText("Live")).toBeInTheDocument()
    expect(
      screen.getByRole("button", { name: /end meeting/i }),
    ).toBeInTheDocument()
    // The fixture session has no insights model, so it says what to set up
    // rather than offering a switch that could never work.
    expect(screen.getByText("Insights are not set up")).toBeInTheDocument()

    await user.click(screen.getByRole("button", { name: /pause/i }))
    expect(
      await screen.findByRole("button", { name: /resume/i }),
    ).toBeInTheDocument()
    await waitFor(() =>
      expect(useActiveSessionStore.getState().activeSession?.state).toBe(
        "paused",
      ),
    )
  })
})
