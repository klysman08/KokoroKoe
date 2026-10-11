/**
 * Fictional data for the screenshot harness. Every name, sentence, and number
 * here is invented; the shapes come from the frozen contract fixtures so the
 * real Zod schemas accept them.
 */
import appSettings from "../../fixtures/contracts/app-settings-v1.json"
import audioDevices from "../../fixtures/contracts/audio-device-test-v1.json"
import credentialStatus from "../../fixtures/contracts/credential-status-v1.json"
import dashboard from "../../fixtures/contracts/dashboard-metrics-v1.json"
import modelManagement from "../../fixtures/contracts/model-management-v1.json"
import openRouterModel from "../../fixtures/contracts/openrouter-model-v1.json"
import presetCatalog from "../../fixtures/contracts/preset-catalog-v1.json"
import projectSession from "../../fixtures/contracts/project-session-v1.json"
import recentInsights from "../../fixtures/contracts/recent-insights-v1.json"

export const DEMO_PROJECT_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
export const DEMO_SESSION_ID = "cccccccc-cccc-4ccc-8ccc-cccccccccccc"
const DEMO_MODEL = "google/gemini-2.5-flash"

const minutesAgo = (minutes: number) =>
  new Date(Date.now() - minutes * 60_000).toISOString().replace(/\.\d+Z$/, "Z")

export const demoProject = {
  ...projectSession.project,
  name: "Product team",
  description: "Weekly planning, reviews, and customer calls.",
  participants: ["Ana", "Bruno", "Carla"],
  tags: ["product", "weekly"],
}

const sessionCreatedAt = minutesAgo(24)

export const demoSession = {
  ...projectSession.session,
  folderName: `${sessionCreatedAt.slice(0, 10)}-q4-roadmap-review--cccccccc`,
  title: "Q4 roadmap review",
  objective: "Agree what ships in the November release.",
  language: "en-US",
  preset: {
    ...projectSession.session.preset,
    name: presetCatalog.name,
    insightTypes: [
      "decision",
      "action_item",
      "risk",
      "unaddressed_topic",
      "follow_up_question",
    ],
  },
  llmModels: { insights: DEMO_MODEL, summaries: DEMO_MODEL },
  spendingLimitUsd: "0.50",
  createdAt: sessionCreatedAt,
  startedAt: minutesAgo(23),
  updatedAt: minutesAgo(0),
}

/** Earlier meetings, so Home's recent list reads like a real week. */
const pastMeetings = [
  ["dddddddd-dddd-4ddd-8ddd-dddddddddddd", "Customer call: Northwind", 1],
  ["eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee", "Design review: onboarding", 2],
  ["ffffffff-ffff-4fff-8fff-ffffffffffff", "Weekly sync", 4],
].map(([id, title, daysAgo]) => {
  const createdAt = minutesAgo(Number(daysAgo) * 24 * 60)
  const slug = String(title)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
  return {
    ...demoSession,
    id: String(id),
    folderName: `${createdAt.slice(0, 10)}-${slug}--${String(id).slice(0, 8)}`,
    title: String(title),
    state: "completed",
    summaryStatus: "completed",
    createdAt,
    startedAt: createdAt,
    endedAt: minutesAgo(Number(daysAgo) * 24 * 60 - 42),
    updatedAt: minutesAgo(Number(daysAgo) * 24 * 60 - 43),
  }
})

const lines: Array<["microphone" | "system_output", string]> = [
  [
    "system_output",
    "Okay, let's start with the November release. Offline export is the last big item still open.",
  ],
  [
    "microphone",
    "Export is done on our side. What's left is the PDF layout, and Bruno said that's about three days.",
  ],
  [
    "system_output",
    "Then let's commit to it. Offline export ships on November 18th, and Bruno owns the PDF layout.",
  ],
  [
    "microphone",
    "Agreed. One worry: the sync service still times out with large workspaces, and we have no load test for it yet.",
  ],
  [
    "system_output",
    "Carla can set up a load test this week. If it fails we move sync to the December release.",
  ],
  [
    "microphone",
    "Fine by me. We should also tell the support team about the new export before customers ask.",
  ],
  [
    "system_output",
    "Good point. And the pricing page still mentions the old plan names, someone has to update it.",
  ],
]

const segmentId = (index: number) =>
  `5e9a1c00-0000-4000-8000-${String(index + 1).padStart(12, "0")}`

export function demoFinalEvents() {
  let startMs = 61_000
  return lines.map(([source, text], index) => {
    const endMs = startMs + 4_000 + text.length * 45
    const event = {
      schemaVersion: 1,
      projectId: DEMO_PROJECT_ID,
      sessionId: DEMO_SESSION_ID,
      event: {
        schemaVersion: 1,
        eventId: `6f0b2d00-0000-4000-8000-${String(index + 1).padStart(12, "0")}`,
        emittedAt: minutesAgo(lines.length - index),
        sessionSequence: index + 1,
        requestId: "34ad1c4d-3c82-4f4e-aa7c-e555b667f2cd",
        payload: {
          segment: {
            id: segmentId(index),
            source,
            startMs,
            endMs,
            text,
            status: "final",
            language: "en",
          },
        },
      },
    }
    startMs = endMs + 1_500
    return event
  })
}

const moment = (index: number) => {
  const event = demoFinalEvents()[index]?.event.payload.segment
  return {
    segmentId: segmentId(index),
    source: event?.source ?? "microphone",
    startMs: event?.startMs ?? 0,
  }
}

const demoInsights = [
  {
    type: "decision",
    title: "Offline export ships on November 18",
    content:
      "The team committed to the date once the remaining PDF layout work was sized at about three days.",
    rationale: "Stated explicitly and agreed without objection.",
    relatedSegmentIds: [segmentId(1), segmentId(2)],
    relatedMoments: [moment(1), moment(2)],
    confidence: 0.92,
  },
  {
    type: "action_item",
    title: "Bruno: finish the PDF layout",
    content: "Needed before November 18 for the offline export release.",
    relatedSegmentIds: [segmentId(2)],
    relatedMoments: [moment(2)],
    confidence: 0.88,
  },
  {
    type: "risk",
    title: "Sync times out on large workspaces",
    content:
      "There is no load test yet. If Carla's test this week fails, sync moves to the December release.",
    relatedSegmentIds: [segmentId(3), segmentId(4)],
    relatedMoments: [moment(3), moment(4)],
    confidence: 0.84,
  },
  {
    type: "unaddressed_topic",
    title: "Pricing page has no owner",
    content:
      "The old plan names are still on the pricing page, but nobody took the update.",
    relatedSegmentIds: [segmentId(6)],
    relatedMoments: [moment(6)],
    confidence: 0.77,
  },
]

const windowAppearance = (window: string) => ({
  window,
  schemaVersion: 1,
  backgroundOpacity: 0.92,
  alwaysOnTop: true,
  compact: false,
})

const defaultShortcuts: Record<string, string> = {
  transcript: "Ctrl+Shift+T",
  insights: "Ctrl+Shift+I",
}

type Handler = (payload: unknown) => unknown
const request = (payload: unknown) =>
  ((payload as { request?: Record<string, unknown> } | undefined)?.request ??
    payload ??
    {}) as Record<string, unknown>

export const demoCommands: Record<string, Handler> = {
  get_settings: () => ({
    ...appSettings,
    revision: 4,
    workspacePath: "C:\\Users\\Demo\\Documents\\KokoroKoe",
    llmEnabled: true,
    defaultLlmModels: {
      insights: DEMO_MODEL,
      summaries: DEMO_MODEL,
      manualQuestions: DEMO_MODEL,
    },
    defaultSessionBudgetUsd: "0.50",
    defaultSessionLanguage: "en-US",
    defaultTranscriptionModelId: modelManagement.installation.descriptor.id,
  }),
  get_openrouter_credential_status: () => credentialStatus,
  list_openrouter_models: () => [
    { ...openRouterModel, id: DEMO_MODEL, name: "Gemini 2.5 Flash" },
  ],
  list_projects: () => ({ items: [demoProject] }),
  get_project: () => demoProject,
  get_session: () => demoSession,
  list_sessions: () => ({ items: [demoSession, ...pastMeetings] }),
  list_presets: () => [presetCatalog],
  list_audio_devices: () => audioDevices.deviceList,
  list_transcription_models: () => {
    // The fixture is mid-download; the demo has it installed.
    const installation: Partial<typeof modelManagement.installation> = {
      ...modelManagement.installation,
    }
    delete installation.downloadJob
    return [
      {
        ...installation,
        status: "installed",
        installedBytes: modelManagement.installation.descriptor.diskBytes,
        installedAt: minutesAgo(7 * 24 * 60),
        selectedAsDefault: true,
      },
    ]
  },
  get_dashboard_metrics: () => ({
    ...dashboard.metrics,
    sessions: 14,
    transcribedMs: 9 * 3_600_000 + 25 * 60_000,
    projects: [{ projectId: DEMO_PROJECT_ID, sessions: 14 }],
    transcriptionModels: [
      { modelId: modelManagement.installation.descriptor.id, sessions: 14 },
    ],
    llmModels: [
      {
        modelId: DEMO_MODEL,
        requests: 96,
        inputTokens: 412_000,
        outputTokens: 38_500,
        actualCostUsd: "0.219700000000",
      },
    ],
    llmRequests: 96,
    inputTokens: 412_000,
    outputTokens: 38_500,
    actualCostUsd: "0.219700000000",
    insightsGenerated: 187,
    usageRecordedSince: minutesAgo(30 * 24 * 60),
    disk: {
      workspaceBytes: 6_815_744,
      trashBytes: 40_960,
      modelBytes: modelManagement.installation.descriptor.diskBytes,
      complete: true,
    },
    recentErrors: [],
  }),
  get_detached_window_appearance: (payload) =>
    windowAppearance(String(request(payload).window ?? "transcript")),
  get_detached_window_shortcut: (payload) => {
    const window = String(request(payload).window ?? "transcript")
    return {
      window,
      schemaVersion: 1,
      binding: defaultShortcuts[window] ?? "Ctrl+Shift+T",
      enabled: true,
      registered: true,
    }
  },
  get_detached_window_interaction: (payload) => ({
    window: String(request(payload).window ?? "transcript"),
    schemaVersion: 1,
    clickThrough: false,
  }),
  generate_recent_insights: (payload) => ({
    ...recentInsights.response,
    requestId: request(payload).requestId,
    projectId: DEMO_PROJECT_ID,
    sessionId: DEMO_SESSION_ID,
    insights: demoInsights,
    sessionActualCostUsd: "0.003120000000",
    availableBudgetUsd: "0.496880000000",
  }),
}
