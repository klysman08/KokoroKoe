# KokoroKoe Architecture

Status: Accepted for the Windows MVP on 2026-08-05.

## Product boundary

KokoroKoe is a Windows 10 22H2/Windows 11 x64 meeting assistant. It captures microphone and selected system-output audio as independent logical sources, transcribes both locally, stores portable Markdown, and optionally sends only necessary transcript text to OpenRouter.

The MVP excludes accounts, cloud collaboration, synchronization, mobile clients, advanced diarization, meeting-platform bots/integrations, telemetry, training, and a proprietary backend.

Defaults are privacy-preserving: LLM features and audio retention are disabled. The default workspace is `%USERPROFILE%\Documents\KokoroKoe`, and onboarding may select another writable folder.

## Trust boundary and components

```text
React windows
  main: projects, settings, models, dashboard
  transcript: chronological source-labelled transcript
  insights: suggestions, questions, risks, decisions, actions
        |
        | typed Tauri commands, semantic events, ordered channels
        v
Rust application coordinator and session state machine
  |-- audio supervisor
  |     |-- WASAPI microphone capture
  |     |-- WASAPI render-endpoint loopback capture
  |     `-- normalize -> meter -> VAD -> bounded ASR scheduler
  |-- transcription engine -> Whisper CPU/Vulkan
  |-- project/session services -> session writer
  |     |-- Markdown snapshots + recovery journal
  |     `-- rebuildable SQLite/FTS projection
  |-- context builder -> versioned prompts -> OpenRouter text API
  |-- Windows Credential Manager
  `-- windows, path security, sanitized logging, local metrics
```

React is a presentation client. It has no direct credential, arbitrary filesystem, arbitrary HTTP, shell, process, audio, or model access. Separate Tauri capabilities and Rust-side authorization constrain the main, transcript, and insights windows.

## Backend modules

- `audio`: enumeration, WASAPI capture, QPC clock mapping, processing, levels, VAD, retention, and device recovery.
- `transcription`: engine trait, Whisper adapter, fair bounded scheduling, partial/final reconciliation.
- `models`: curated catalog, resumable verified downloads, installation state, selection, and deletion.
- `domain`: identifiers, projects, sessions, presets, transcript segments, insights, summaries, usage, and errors.
- `persistence`: secure workspace paths, Markdown rendering, journal, atomic replacement, SQLite/FTS, and recovery.
- `llm`: OpenRouter provider, streaming, cancellation, retries, typed failures, and cost accounting.
- `prompts`: versioned specifications, context budgets, untrusted transcript delimiters, and output schemas.
- `insights`: generation policy, deduplication, confirmation, accumulated and final summaries.
- `security`: Credential Manager, redaction, path validation, and destructive-action confirmation.
- `windows`: native windows, shortcuts, opacity, position, size, monitor preference, and quick-hide.
- `commands` and `events`: transport adapters without business logic.
- `metrics`: local operational, model, cost, disk, and sanitized error aggregates.

## Repository and source structure

```text
KokoroKoe/
  AGENTS.md
  Manifest.md
  package.json
  pnpm-lock.yaml
  components.json
  vite.config.ts
  tsconfig*.json
  .agents/skills/kokorokoe-development/
    SKILL.md
    agents/openai.yaml
    references/
  docs/
    architecture.md
    dependency-licenses.md
    privacy-and-limitations.md
    project-memory.md
    adr/
  src/
    app/                  # providers, routing, and window entry selection
    components/ui/        # generated shadcn/ui components
    contracts/            # TypeScript types, Zod schemas, and event names
    features/
      home/
      projects/
      sessions/
      transcript/
      insights/
      models/
      presets/
      settings/
    lib/tauri/            # narrow typed invoke/listen/channel adapters
    stores/               # Zustand stores for ephemeral UI/session state
  src-tauri/
    capabilities/
      main.json
      transcript.json
      insights.json
    migrations/           # ordered SQLite projection migrations
    src/
      audio/
      transcription/
      models/
      domain/
      persistence/
      llm/
      prompts/
      insights/
      security/
      windows/
      metrics/
      commands/
      events/
      lib.rs
      main.rs
    tests/
  fixtures/
    contracts/            # golden Rust/TypeScript JSON contracts
    audio/                # licensed/generated deterministic audio fixtures
    persistence/
    security/
  tests/e2e/
```

Create only the directories needed by the active task. Do not commit model weights, retained audio, user workspaces, SQLite databases, logs, or build output.

## Audio and transcription flow

1. Resolve each device selection as `default(role)` or `fixed(endpointId)`.
2. Start independent shared-mode event-driven WASAPI threads for the microphone and render-endpoint loopback.
3. Map packet QPC timestamps to one session-relative millisecond timeline; do not align by callback arrival.
4. Copy packets into bounded queues, release WASAPI buffers, convert finite samples to `f32`, downmix, and resample each source to 16 kHz mono.
5. Emit throttled RMS, peak, and clipping levels.
6. Run a source-local VAD and utterance segmenter with pre-roll, trailing silence, and forced splitting.
7. Submit jobs to one model-owning inference worker. Final jobs are chronological and higher priority; keep at most one replaceable partial job per source.
8. Use stable utterance IDs so final results replace partial results.
9. Send durable mutations to one per-session writer.

Capture threads never wait on inference. Disable partial work at 20 seconds of queued speech, restore it below 10 seconds, and cap final backlog at 10 minutes. At the hard cap, record an explicit transcript gap and error instead of allowing unbounded memory growth.

Whisper Tiny and Base multilingual models are the initial catalog entries. Base is the intended default only if it passes the Phase 3 throughput gate on the minimum supported hardware; otherwise Tiny becomes the default and Base remains an accuracy-oriented option. CPU is mandatory. Vulkan is the first acceleration path, with a runtime CPU retry and a supervised worker fallback if the in-process Vulkan prototype cannot isolate driver failures.

## Domain contracts

Rust types are the semantic source. JSON uses `camelCase` fields and `snake_case` enum values. Strict TypeScript types and Zod schemas mirror the boundary. Shared golden JSON fixtures must parse in both Rust and Vitest.

Identifiers use UUID newtypes. Wall-clock values use RFC 3339 UTC. Audio ordering uses unsigned milliseconds from the session epoch. Mutations carry `expectedRevision`; long operations also use `requestId`.

Stable enums include:

```text
AudioSource: microphone | system_output
SegmentStatus: partial | final
SessionState: idle | preparing | capturing | transcribing | paused |
              stopping | processing_summary | completed | failed
```

The following TypeScript-shaped definitions are the transport form of equivalent Rust structs/enums. Optional fields are explicitly marked; all other fields are required.

```ts
type BrandedId<Brand extends string> = string & { readonly __brand: Brand }
type ProjectId = BrandedId<"ProjectId">
type SessionId = BrandedId<"SessionId">
type SegmentId = BrandedId<"SegmentId">
type InsightId = BrandedId<"InsightId">
type PresetId = BrandedId<"PresetId">
type RequestId = BrandedId<"RequestId">
type Rfc3339Utc = string
type AudioSource = "microphone" | "system_output"
type SegmentStatus = "partial" | "final"
type SessionState =
  | "idle"
  | "preparing"
  | "capturing"
  | "transcribing"
  | "paused"
  | "stopping"
  | "processing_summary"
  | "completed"
  | "failed"

type LlmRoleModels = {
  insights?: string
  summaries?: string
  manualQuestions?: string
}

type Project = {
  schemaVersion: 1
  id: ProjectId
  name: string
  folderName: string
  description: string
  globalContext: string
  participants: string[]
  tags: string[]
  defaultPresetId: PresetId
  defaultTranscriptionModelId: string
  preferredLlmModels: LlmRoleModels
  createdAt: Rfc3339Utc
  updatedAt: Rfc3339Utc
  revision: number
}

type DeviceSelection =
  | { kind: "default"; role: "console" | "multimedia" | "communications" }
  | { kind: "fixed"; endpointId: string }

type AudioDeviceSnapshot = {
  endpointId: string
  friendlyName: string
  selection: DeviceSelection
  nativeSampleRate?: number
  nativeChannels?: number
}

type PresetSnapshot = {
  id: PresetId
  version: number
  name: string
  assistantRole: string
  analysisObjectives: string[]
  insightTypes: InsightType[]
  responseTone: string
  finalSummarySections: string[]
  highlightInstructions: string[]
  prohibitedBehaviors: string[]
}

type Session = {
  schemaVersion: 1
  id: SessionId
  projectId: ProjectId
  folderName: string
  title: string
  objective: string
  sessionContext: string
  preset: PresetSnapshot
  language: string
  microphone: AudioDeviceSnapshot
  systemOutput: AudioDeviceSnapshot
  transcriptionEngine: "whisper"
  transcriptionModelId: string
  llmModels: LlmRoleModels
  retainAudio: boolean
  state: SessionState
  channelHealth: Record<AudioSource, ChannelHealth>
  summaryStatus: "not_requested" | "pending" | "completed" | "deferred" | "failed"
  usage: UsageAggregate
  createdAt: Rfc3339Utc
  startedAt?: Rfc3339Utc
  endedAt?: Rfc3339Utc
  updatedAt: Rfc3339Utc
  revision: number
}

type ChannelHealth = {
  status: "starting" | "active" | "silent" | "reconnecting" | "unavailable" | "stopped"
  endpointId?: string
  detailCode?: string
  updatedAt: Rfc3339Utc
}

type TranscriptSegment = {
  id: SegmentId
  sessionId: SessionId
  source: AudioSource
  startMs: number
  endMs: number
  text: string
  status: SegmentStatus
  language: string
  confidence?: number
  bookmark: boolean
  important: boolean
  audioReference?: string
  createdAt: Rfc3339Utc
  lastEditedAt?: Rfc3339Utc
  revision: number
}

type Preset = PresetSnapshot & {
  schemaVersion: 1
  description: string
  builtIn: boolean
  createdAt: Rfc3339Utc
  updatedAt: Rfc3339Utc
  revision: number
}

type InsightType =
  | "suggested_response"
  | "follow_up_question"
  | "clarification"
  | "fact_or_number"
  | "risk"
  | "objection"
  | "decision"
  | "action_item"
  | "contradiction"
  | "unaddressed_topic"

type Insight = {
  id: InsightId
  sessionId: SessionId
  type: InsightType
  title: string
  content: string
  rationale?: string
  relatedSegmentIds: SegmentId[]
  confidence?: number
  status: "provisional" | "confirmed" | "dismissed"
  pinned: boolean
  promptId: string
  promptVersion: number
  modelId: string
  createdAt: Rfc3339Utc
  revision: number
}

type ActionItem = {
  text: string
  owner?: string
  deadline?: Rfc3339Utc
  relatedSegmentIds: SegmentId[]
}

type SessionSummary = {
  sessionId: SessionId
  executiveSummary: string
  mainTopics: string[]
  decisions: string[]
  actionItems: ActionItem[]
  risks: string[]
  openQuestions: string[]
  nextSteps: string[]
  promptId: string
  promptVersion: number
  modelId: string
  updatedAt: Rfc3339Utc
  revision: number
}

type ModelDescriptor = {
  id: string
  engine: "whisper"
  name: string
  sourceUrl: string
  sourceRevision: string
  fileName: string
  sha256: string
  downloadBytes: number
  diskBytes: number
  languages: string[]
  approximateMemoryBytes: number
  performanceClass: "fast" | "balanced" | "accurate"
  backends: ("cpu" | "vulkan")[]
  licenseSpdx: string
  licenseUrl: string
}

type UsageEntry = {
  requestId: RequestId
  sessionId: SessionId
  purpose: "insight" | "summary" | "manual_question"
  modelId: string
  inputTokens: number
  outputTokens: number
  reasoningTokens: number
  cachedTokens: number
  reservedCostUsd: string
  actualCostUsd: string
  createdAt: Rfc3339Utc
}

type UsageAggregate = {
  inputTokens: number
  outputTokens: number
  estimatedCostUsd: string
  actualCostUsd: string
}

type AppError = {
  code: string
  userMessage: string
  technicalDetail?: string
  severity: "info" | "warning" | "error" | "critical"
  retryable: boolean
  correlationId: string
}

type CommandError = {
  error: AppError
}
```

Validate UUIDs, BCP-47 language tags, finite confidence values in `0..1`, nonnegative millisecond ranges, bounded strings/collections, fixed decimal cost strings, and session-relative audio paths at the Rust boundary.

Persistence representation is explicit: `Project` maps to `project.md`; `Session` to `session.md`; segments form `transcript.md`; custom `Preset` records form preset Markdown; `SessionSummary` forms `summary.md`; insights form `insights.md`; and actions/questions are materialized Markdown views. The journal stores versioned mutations for these records. SQLite stores projections of the same IDs/revisions plus non-content operational state.

Session transitions are explicit:

```text
idle -> preparing -> capturing -> transcribing
transcribing -> paused -> preparing
capturing|transcribing|paused -> stopping
stopping -> processing_summary -> completed
stopping -> completed
active state -> failed
failed -> preparing
```

LLM failures affect LLM health and summary status, not local capture/transcription. A session may complete with a deferred or failed summary.

## Tauri interface

Common command DTOs:

```ts
type Versioned<T> = { expectedRevision: number; value: T }
type RequestContext = { requestId: RequestId }
type PageRequest = { cursor?: string; limit: number }
type Page<T> = { items: T[]; nextCursor?: string }
type DeletePlan = { token: string; expiresAt: Rfc3339Utc; files: string[]; bytes: number }
type CredentialStatus = { configured: boolean; validatedAt?: Rfc3339Utc }
```

Command-specific DTOs and state records are fixed as follows:

```ts
type AppSettings = {
  revision: number
  workspacePath: string
  defaultPresetId: PresetId
  defaultTranscriptionModelId: string
  llmEnabled: boolean
  retainAudioByDefault: boolean
  requireZeroDataRetention: boolean
  denyProviderDataCollection: boolean
  maxTokensPerRequest: number
  defaultSessionBudgetUsd: string
}

type AppSettingsUpdate = Partial<Omit<AppSettings, "revision" | "workspacePath">>
type WorkspaceStatus = { path: string; writable: boolean; freeBytes: number; warning?: string }

type BootstrapData = {
  settings: AppSettings
  workspace: WorkspaceStatus
  projects: Project[]
  recentSessions: Session[]
  recoverableSessions: Session[]
  credentialStatus: CredentialStatus
}

type MetricsQuery = { from?: Rfc3339Utc; to?: Rfc3339Utc; projectId?: ProjectId }
type DashboardMetrics = {
  sessions: number
  transcribedMs: number
  mostUsedProjectIds: ProjectId[]
  mostUsedTranscriptionModelIds: string[]
  mostUsedLlmModelIds: string[]
  inputTokens: number
  outputTokens: number
  actualCostUsd: string
  insightsGenerated: number
  diskBytes: number
  recentErrors: AppError[]
}

type CreateProjectInput = {
  name: string
  description: string
  globalContext: string
  participants: string[]
  tags: string[]
  defaultPresetId: PresetId
  defaultTranscriptionModelId: string
  preferredLlmModels: LlmRoleModels
}
type UpdateProjectInput = Partial<CreateProjectInput>

type CreateSessionInput = {
  projectId: ProjectId
  title: string
  objective: string
  sessionContext: string
  presetId: PresetId
  language: string
  microphoneSelection: DeviceSelection
  systemOutputSelection: DeviceSelection
  transcriptionModelId: string
  llmModels: LlmRoleModels
  retainAudio: boolean
  spendingLimitUsd: string
  maxTokensPerRequest: number
}
type UpdateSessionInput = Partial<Omit<CreateSessionInput, "projectId">>

type DeleteResult = { deleted: boolean; deletedFiles: number; reclaimedBytes: number }
type ExportResult = { path: string; files: number; bytes: number }

type AudioDevice = {
  endpointId: string
  friendlyName: string
  direction: "input" | "output"
  state: "active" | "disabled" | "not_present" | "unplugged"
  isDefaultConsole: boolean
  isDefaultMultimedia: boolean
  isDefaultCommunications: boolean
  sampleRate?: number
  channels?: number
}
type AudioDeviceList = { inputs: AudioDevice[]; outputs: AudioDevice[] }
type DeviceTestInput = { source: AudioSource; selection: DeviceSelection }
type DeviceTestStatus = {
  requestId: RequestId
  source: AudioSource
  status: "starting" | "active" | "stopped" | "failed"
  device?: AudioDevice
  error?: AppError
}

type TranscriptSearchQuery = {
  sessionId: SessionId
  query: string
  sources?: AudioSource[]
  fromMs?: number
  toMs?: number
  cursor?: string
  limit: number
}
type TranscriptSearchHit = { segment: TranscriptSegment; matchedRanges: { start: number; end: number }[] }

type ModelInstallation = {
  descriptor: ModelDescriptor
  status: "not_installed" | "downloading" | "installed" | "failed" | "incompatible"
  installedPath?: string
  installedBytes: number
  installedAt?: Rfc3339Utc
  selectedAsDefault: boolean
  availableBackends: ("cpu" | "vulkan")[]
  lastError?: AppError
}

type ModelDownloadJob = {
  requestId: RequestId
  modelId: string
  status: "queued" | "downloading" | "paused" | "verifying" | "completed" | "cancelled" | "failed"
  bytesDownloaded: number
  totalBytes: number
  resumable: boolean
  etag?: string
  lastModified?: string
  startedAt: Rfc3339Utc
  updatedAt: Rfc3339Utc
  error?: AppError
}

type CreatePresetInput = {
  name: string
  description: string
  assistantRole: string
  analysisObjectives: string[]
  insightTypes: InsightType[]
  responseTone: string
  finalSummarySections: string[]
  highlightInstructions: string[]
  prohibitedBehaviors: string[]
}
type UpdatePresetInput = Partial<CreatePresetInput>

type CredentialValidation = {
  valid: boolean
  validatedAt: Rfc3339Utc
  usageUsd?: string
  remainingLimitUsd?: string
  expiresAt?: Rfc3339Utc
  error?: AppError
}

type OpenRouterModel = {
  id: string
  name: string
  provider: string
  contextLength: number
  promptPricePerToken: string
  completionPricePerToken: string
  supportsStructuredOutputs: boolean
  supportsStreaming: boolean
  zeroDataRetentionAvailable: boolean
  dataCollection: "allow" | "deny" | "unknown"
}

type SegmentQuestionInput = {
  sessionId: SessionId
  segmentId: SegmentId
  question: string
  neighboringSegmentsBefore: number
  neighboringSegmentsAfter: number
  modelId: string
}

type InsightRequestInput = {
  sessionId: SessionId
  relatedSegmentIds: SegmentId[]
  requestedTypes: InsightType[]
  modelId: string
}

type LlmRequestAccepted = { requestId: RequestId; acceptedAt: Rfc3339Utc; reservedCostUsd: string }
type LlmRequestStatus = {
  requestId: RequestId
  sessionId: SessionId
  purpose: "insight" | "summary" | "manual_question"
  status: "queued" | "sending" | "streaming" | "completed" | "cancelled" | "failed"
  error?: AppError
}

type WindowPreferences = {
  revision: number
  windowLabel: "main" | "transcript" | "insights"
  x?: number
  y?: number
  width: number
  height: number
  monitorName?: string
  backgroundOpacity: number
  alwaysOnTop: boolean
  compact: boolean
  clickThrough: boolean
}
type WindowPreferencesUpdate = Partial<Omit<WindowPreferences, "revision" | "windowLabel">>
type WindowStatus = { windowLabel: string; visible: boolean; clickThrough: boolean }

type GlobalShortcutSettings = {
  revision: number
  toggleTranscriptWindow: string
  toggleInsightsWindow: string
  quickHideAll: string
  disableClickThrough: string
}
```

`set_openrouter_api_key` is the only command that accepts secret material. The key may exist in transient local component state while the user submits it, but React must clear that state after invocation and must never place the key in Zustand, TanStack Query caches, local/session storage, logs, URLs, events, or error objects. The result contains status only; Rust stores the secret and never returns it.

Commands use these exact transport names and typed request/result forms:

| Command | Request | Result |
| --- | --- | --- |
| `get_bootstrap` | none | `BootstrapData` |
| `get_settings` | none | `AppSettings` |
| `update_settings` | `Versioned<AppSettingsUpdate>` | `AppSettings` |
| `choose_workspace` | none; backend-controlled folder picker | `WorkspaceStatus` |
| `get_dashboard_metrics` | `MetricsQuery` | `DashboardMetrics` |
| `list_projects` | `PageRequest` | `Page<Project>` |
| `get_project` | `{ projectId }` | `Project` |
| `create_project` | `CreateProjectInput` | `Project` |
| `update_project` | `{ projectId } & Versioned<UpdateProjectInput>` | `Project` |
| `plan_delete_project` | `{ projectId }` | `DeletePlan` |
| `confirm_delete_project` | `{ token }` | `DeleteResult` |
| `create_session` | `CreateSessionInput` | `Session` |
| `update_session` | `{ sessionId } & Versioned<UpdateSessionInput>` | `Session` |
| `start_session` | `{ sessionId } & RequestContext` | `Session` |
| `pause_session` | `{ sessionId; expectedRevision }` | `Session` |
| `resume_session` | `{ sessionId; expectedRevision } & RequestContext` | `Session` |
| `stop_session` | `{ sessionId; expectedRevision } & RequestContext` | `Session` |
| `list_recoverable_sessions` | none | `Session[]` |
| `recover_session` | `{ sessionId } & RequestContext` | `Session` |
| `export_session` | `{ sessionId }` | `ExportResult` |
| `list_audio_devices` | none | `AudioDeviceList` |
| `start_audio_device_test` | `DeviceTestInput & RequestContext` | `DeviceTestStatus` |
| `stop_audio_device_test` | `{ requestId }` | `DeviceTestStatus` |
| `get_transcript_page` | `{ sessionId } & PageRequest` | `Page<TranscriptSegment>` |
| `search_transcript` | `TranscriptSearchQuery` | `Page<TranscriptSearchHit>` |
| `edit_transcript_segment` | `{ segmentId } & Versioned<{ text: string }>` | `TranscriptSegment` |
| `set_segment_bookmark` | `{ segmentId } & Versioned<{ bookmark: boolean }>` | `TranscriptSegment` |
| `set_segment_importance` | `{ segmentId } & Versioned<{ important: boolean }>` | `TranscriptSegment` |
| `list_transcription_models` | none | `ModelInstallation[]` |
| `download_transcription_model` | `{ modelId } & RequestContext` | `ModelDownloadJob` |
| `cancel_model_download` | `{ requestId }` | `ModelDownloadJob` |
| `resume_model_download` | `{ modelId } & RequestContext` | `ModelDownloadJob` |
| `delete_transcription_model` | `{ modelId }` | `ModelInstallation` |
| `set_default_transcription_model` | `{ modelId; expectedSettingsRevision }` | `AppSettings` |
| `list_presets` | none | `Preset[]` |
| `create_preset` | `CreatePresetInput` | `Preset` |
| `update_preset` | `{ presetId } & Versioned<UpdatePresetInput>` | `Preset` |
| `duplicate_preset` | `{ presetId; name: string }` | `Preset` |
| `export_preset` | `{ presetId }` | `ExportResult` |
| `delete_custom_preset` | `{ presetId; expectedRevision }` | `DeleteResult` |
| `set_openrouter_api_key` | `{ apiKey: string }` | `CredentialStatus` |
| `delete_openrouter_api_key` | none | `CredentialStatus` |
| `get_openrouter_credential_status` | none | `CredentialStatus` |
| `validate_openrouter_api_key` | `RequestContext` | `CredentialValidation` |
| `list_openrouter_models` | `{ forceRefresh: boolean } & RequestContext` | `OpenRouterModel[]` |
| `ask_about_segment` | `SegmentQuestionInput & RequestContext` | `LlmRequestAccepted` |
| `generate_insight` | `InsightRequestInput & RequestContext` | `LlmRequestAccepted` |
| `regenerate_insight` | `{ insightId } & RequestContext` | `LlmRequestAccepted` |
| `pin_insight` | `{ insightId } & Versioned<{ pinned: boolean }>` | `Insight` |
| `dismiss_insight` | `{ insightId; expectedRevision }` | `Insight` |
| `generate_summary` | `{ sessionId } & RequestContext` | `LlmRequestAccepted` |
| `cancel_llm_request` | `{ requestId }` | `LlmRequestStatus` |
| `get_window_preferences` | `{ windowLabel }` | `WindowPreferences` |
| `update_window_preferences` | `{ windowLabel } & Versioned<WindowPreferencesUpdate>` | `WindowPreferences` |
| `set_window_visibility` | `{ windowLabel; visible: boolean }` | `WindowStatus` |
| `set_window_click_through` | `{ windowLabel; enabled: boolean }` | `WindowStatus` |
| `set_global_shortcuts` | `Versioned<GlobalShortcutSettings>` | `GlobalShortcutSettings` |

Commands return `Result<T, CommandError>`, serialized as `{ error: AppError }`. The envelope contains only a sanitized `AppError` and never includes a secret, raw source error, path where unnecessary, or raw provider response. Collection limits, string bounds, UUIDs, BCP-47 tags, revisions, paths, and finite numeric ranges are validated in Rust even if Zod already rejected them in React.

All named semantic events use:

```ts
type EventEnvelope<T> = {
  schemaVersion: 1
  eventId: string
  emittedAt: Rfc3339Utc
  sessionSequence?: number
  requestId?: RequestId
  payload: T
}
```

| Event | Payload |
| --- | --- |
| `audio-level-updated` | `{ sessionId?: SessionId; testId?: RequestId; source: AudioSource; rmsDbfs: number; peakDbfs: number; clipping: boolean; muted: boolean; atMs: number }` |
| `transcription-partial` | `{ segment: TranscriptSegment }` with stable ID and `status=partial` |
| `transcription-final` | `{ segment: TranscriptSegment; replacesPartialId?: SegmentId }` |
| `session-status-changed` | `{ sessionId; previous: SessionState; current: SessionState; reason?: string; recoverable: boolean; channelHealth; summaryStatus }` |
| `insight-generated` | `{ insight: Insight }` after local validation |
| `summary-updated` | `{ summary: SessionSummary; changedSections: string[] }` |
| `model-download-progress` | `{ job: ModelDownloadJob; bytesPerSecond?: number }` |
| `application-error` | `{ error: AppError; sessionId?: SessionId; source?: AudioSource }` |
| `audio-device-status-changed` | `{ source; previous: ChannelHealth; current: ChannelHealth; isDefaultChange: boolean }` |
| `llm-request-status` | `{ requestId; sessionId; purpose; status: "queued" | "sending" | "streaming" | "completed" | "cancelled" | "failed"; error?: AppError }` |
| `persistence-status` | `{ sessionId; journalSequence; snapshotSequence; status: "clean" | "writing" | "conflict" | "failed"; error?: AppError }` |
| `session-recovered` | `{ session: Session; replayedEvents: number; restoredPartial: boolean }` |

Use ordered Tauri Channels for high-frequency LLM deltas, model-download byte deltas, and optional combined session streams. Persist only complete locally validated LLM results, not raw streaming fragments.

## Persistence ownership and recovery

User content layout:

```text
workspace/projects/<slug>--<uuid8>/
  project.md
  presets/
  sessions/<yyyy-mm-dd-slug>--<uuid8>/
    session.md
    transcript.md
    summary.md
    insights.md
    actions.md
    questions.md
    recovery.journal
    audio/
```

Markdown owns important project/session content. SQLite owns rebuildable projections, FTS5, non-secret settings, window/device preferences, model/download state, detailed metrics/cost entries, sanitized errors, and migrations. Windows Credential Manager alone owns the API key.

The per-session writer appends checksummed sequenced journal entries and syncs final segments, edits, bookmarks, and state changes before acknowledgement. It materializes Markdown after two seconds or five final segments and immediately at lifecycle boundaries. Snapshot replacement uses a same-directory temporary file, disk synchronization, an atomic Windows replacement, and `.bak` recovery.

Startup validates journal records, tolerates only a torn final record, replays after the Markdown checkpoint, and exposes interrupted work as paused with `recovery_required`. SQLite corruption is handled by preserving and rebuilding the projection. An external Markdown hash conflict pauses snapshot replacement while journaling continues.

## OpenRouter and prompts

OpenRouter integration is Rust-only and text-only. LLM features start disabled. Enabling them shows an external-service indicator. Zero-data-retention/provider data-collection restrictions are enabled by default; relaxing them requires explicit confirmation.

The context builder combines frozen project/session/preset context, accumulated summary, a recent sliding window, deduplicated FTS-selected segments, and the requested output. It never resends the full transcript by default. Transcript text is delimited as untrusted data and cannot override internal instructions.

Prompt specifications have ID, version, purpose, variables, approximate context limit, output schema, and fallback. Use structured outputs when the selected model supports them, validate locally, and allow at most one budget-aware repair attempt.

Reserve a conservative request cost before sending, block when the session limit would be exceeded, and reconcile against actual OpenRouter usage. Retry only transient network, rate-limit, overload, or selected provider/server failures and honor `Retry-After`.

## Prototype gates

- Dual WASAPI capture across common device classes with unaffected-channel recovery.
- QPC-derived timestamp error below 20 ms over two hours, excluding physical acoustic latency.
- PCM/float, channel-count, and sample-rate format matrix.
- Earshot versus Silero VAD bake-off with documented thresholds.
- Tiny/Base real-time factor below 1.0 on supported configurations and target final p95 below two seconds.
- Vulkan startup/inference failure with verified CPU recovery.
- Two-hour slowed-inference backpressure soak with plateaued memory and explicit gaps.
- Journal/snapshot crash fault injection with no loss of acknowledged final segments.
- Development and packaged Credential Manager secret-canary tests.
- Transparent-window readability and emergency click-through escape.
- Mock OpenRouter streaming, cancellation, typed errors, malformed output, privacy, and budget tests.
- Dependency audit and third-party/model notice review before distribution.

## Delivery phases

1. Architecture and coordination artifacts.
2. Tauri/React foundation and security baseline.
3. Windows audio, models, and local transcription.
4. Projects, Markdown/SQLite persistence, search, and recovery.
5. OpenRouter, prompts, insights, and summaries.
6. Multiple-window desktop experience.
7. Quality, installer, audits, notices, and Windows x64 packaging.

Each phase closes only after its acceptance evidence is recorded in `docs/project-memory.md` and the rolling `AGENTS.md` checkpoint is updated.

## Primary references

- [Manifest requirements](../Manifest.md)
- [Handy reference implementation](https://github.com/cjpais/handy)
- [Microsoft WASAPI loopback](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording)
- [Tauri 2 documentation](https://v2.tauri.app/)
- [OpenRouter documentation](https://openrouter.ai/docs/quickstart)
- [shadcn/ui Vite installation](https://ui.shadcn.com/docs/installation/vite)
