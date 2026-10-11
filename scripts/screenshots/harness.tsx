/**
 * Renders the real application with a mocked IPC layer and fictional demo
 * data, so README screenshots and the social preview show the current
 * interface without a real meeting, a real transcript, or an API key.
 *
 * Development only: Vite serves this page, `scripts/capture-screenshots.mjs`
 * photographs it, and the production build never includes it.
 */
import { emit } from "@tauri-apps/api/event"
import { mockIPC } from "@tauri-apps/api/mocks"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import App from "@/App"
import "@/index.css"
import { useMeetingStore } from "@/stores/meeting-store"
import { useNavigationStore } from "@/stores/navigation-store"
import { useShellStore } from "@/stores/shell-store"

import {
  demoCommands,
  demoFinalEvents,
  DEMO_PROJECT_ID,
  DEMO_SESSION_ID,
} from "./demo-data"

const parameters = new URLSearchParams(window.location.search)
const view = parameters.get("view") ?? "meeting"
const theme = parameters.get("theme") === "light" ? "light" : "dark"
const collapsed = parameters.get("collapsed") === "1"

mockIPC(
  (command, payload) => {
    const handler = demoCommands[command]
    if (handler) return handler(payload)
    console.warn(`[harness] unmocked command: ${command}`, payload)
    return null
  },
  { shouldMockEvents: true },
)

useShellStore.setState({ theme, navigationCollapsed: collapsed })
if (view === "meeting") {
  useMeetingStore
    .getState()
    .open({ projectId: DEMO_PROJECT_ID, sessionId: DEMO_SESSION_ID })
  useNavigationStore.getState().navigate("meeting")
} else if (view === "dashboard") {
  useNavigationStore.getState().navigate("dashboard")
} else {
  useNavigationStore.getState().navigate("home")
}

const root = document.getElementById("root")
if (!root) throw new Error("The harness could not find its root")
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

const wait = (ms: number) =>
  new Promise((resolve) => window.setTimeout(resolve, ms))

/**
 * Plays the demo, then raises the flag `scripts/capture-screenshots.mjs`
 * waits for before it takes the picture.
 */
async function playDemo() {
  await wait(800)
  if (view === "meeting") {
    // The meeting view has subscribed by now; the transcript "arrives", and
    // automatic insights answer from the mocked command.
    for (const event of demoFinalEvents()) {
      await emit("session-transcription-final", event)
    }
  }
  await document.fonts.ready
  await wait(1_500)
  document.documentElement.dataset.screenshotReady = "true"
}

void playDemo()
