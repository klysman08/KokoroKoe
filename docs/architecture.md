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
  |-- transcription engine -> Whisper CPU / supervised Vulkan worker
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

Capture threads never wait on inference. Disable partial work at 20 seconds of queued final speech, discard queued partial work at that transition, and restore partial admission only below 10 seconds. Cap pending final backlog at 10 minutes, 9,600,000 normalized samples, and 4,096 jobs so minimum-duration utterances cannot create unbounded queue-node overhead. Reject the newest whole final when any ceiling would be exceeded and return its source/timeline in a fixed `transcription_final_backlog_exceeded` gap record; never silently evict an accepted older final.

Capture recovery is source-local. A failed stream retains the single session QPC epoch, retries only its selected endpoint policy, and cannot stop the other capture supervisor. Aggregate diagnostics retain capture-attempt and failure counts, pending/completed recovery-gap counts and durations, and the last fixed failure code after the transient current error clears. P3-007 deterministically verifies two hours at different source cadences with less than 20 ms calculated error and live-probes one injected microphone stream failure; physical endpoint removal, Windows Audio restart, suspend/resume, and the broader device matrix remain separate hardware gates.

Whisper Tiny and Base multilingual models are the initial catalog entries. Base is the intended default only if it passes the Phase 3 throughput gate on the minimum supported hardware; otherwise Tiny becomes the default and Base remains an accuracy-oriented option. CPU is mandatory. Vulkan is the first acceleration path and runs only in a supervised local worker under [ADR 0007](adr/0007-supervised-vulkan-transcription-worker.md). The worker must attest Vulkan rather than silently accept upstream CPU fallback. Startup failure, timeout, protocol failure, nonzero exit, or crash discards incomplete worker output and retries the same finalized utterance exactly once on CPU without emitting both results.

P3-008 implements worker protocol v1 through anonymous stdin/stdout pipes to a worker mode in the same KokoroKoe executable. Frames use a four-byte little-endian length followed by strict JSON and are capped at 16 MiB; one sequential request carries at most one validated 480,000-sample finalized utterance. The child must answer with a Vulkan-attested hello before work. Parent supervision fixes startup, write, and inference/read deadlines, polls explicit cancellation, validates returned request/source/timeline/text bounds, and owns the child through a Windows Job Object configured to terminate its process tree. Debug-only fault modes are absent from release builds. The CPU factory remains lazy and is invoked only after the failed worker is terminated; cancellation does not trigger CPU inference.

P3-009 connects each `SourceProcessor`'s finalized VAD output to the scheduler through a conservative source-local ordering frontier. The frontier is the earliest session timestamp at which that open VAD stream could still begin a future final, including pending frames, retained pre-roll, or an active utterance. A queued final is eligible only when its start is strictly below both open frontiers; a source sealed after its terminal VAD flush contributes infinity. This prevents a delayed source from delivering an older final after newer inference has started while retaining start/end/arrival tie ordering. Pause and stop accept both terminal flushes, seal both sources, discard provisional work, and account for every accepted final as one result or fixed gap before completing. Resume resets only the open frontiers and preserves the nondecreasing session timeline. Explicit cancellation creates fixed gaps and never causes CPU recovery.

P3-010 fixes the initial model catalog to multilingual Tiny and Base at one immutable upstream revision with exact byte counts, SHA-256 digests, MIT provenance, languages, approximate memory requirements, and CPU/Vulkan declarations. Rust alone owns HTTP, the canonical app-local model root, fixed catalog filenames, resume metadata, compatibility checks, full-file verification, selection, and deletion. Resumable staging uses exact catalog-derived `.part` and strict JSON sidecar names; only a matching byte-range response appends, while a full response restarts. Exact length and SHA-256 verification plus file sync precede a same-directory rename to the final filename. Default selection remains in-memory until settings/download-state persistence is deliberately wired; no model command, event, capability, or UI exists at this gate.

The first local runtime uses KokoroKoe-owned bounded C ABI shim API v2 over pinned MIT-licensed whisper.cpp rather than the Unlicense Rust wrapper. Rust owns paths, samples, source/timeline labels, explicit CPU/Vulkan selection, errors, and result bounds. Each native module remains loaded for its process lifetime because dynamically registered GGML backends are unsafe to unload/reload; model contexts and inference results are explicitly freed. A healthy worker alone owns the Vulkan model. The parent loads the CPU model only after the worker is unavailable or terminated, so only one heavy model remains loaded.

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
  installedBytes: number
  installedAt?: Rfc3339Utc
  selectedAsDefault: boolean
  availableBackends: ("cpu" | "vulkan")[]
  compatibility: {
    availableDiskBytes: number
    requiredDiskBytes: number
    availableMemoryBytes: number
    approximateMemoryBytes: number
    diskCompatible: boolean
    memoryCompatible: boolean
  }
  downloadJob?: ModelDownloadJob
  lastError?: AppError
}

type LiveTranscriptionInput = {
  acknowledgedCaptureConsent: boolean
  microphoneSelection: DeviceSelection
  systemOutputSelection: DeviceSelection
}
type LiveTranscriptionRunState = "idle" | "starting" | "running" | "stopping" | "stopped" | "failed"
type LiveTranscriptionStatus = {
  state: LiveTranscriptionRunState
  requestId?: RequestId
  startedAt?: Rfc3339Utc
  stoppedAt?: Rfc3339Utc
  error?: AppError
}
type LiveTranscriptSegment = {
  id: SegmentId
  source: AudioSource
  startMs: number
  endMs: number
  text: string
  status: SegmentStatus
  language: string
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
| `start_live_transcription` | `LiveTranscriptionInput & RequestContext` | `LiveTranscriptionStatus` |
| `get_live_transcription_status` | none | `LiveTranscriptionStatus` |
| `stop_live_transcription` | `{ requestId }` | `LiveTranscriptionStatus` |
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
| `transcription-gap` | `{ source: AudioSource; startMs: number; endMs: number; code: string }` for the transient P3-015 live run |
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

P3-015 uses the three `live_transcription` commands only for one transient Phase 3 run before Phase 4 project/session ownership exists. Its transcript envelopes require `requestId` and `sessionSequence`, target only the invoking `main` webview, and carry `LiveTranscriptSegment` without a persisted `sessionId`. The React surface holds at most 500 inert plain-text records. These commands are not aliases for the later architecture-defined project-backed `start_session`/pause/resume/stop lifecycle; the future session service will own persistence and wrap the same Rust audio/transcription coordinator. Missing native runtime assets fail with a fixed sanitized error and do not broaden frontend filesystem/process access.

The bounded model manager is the exception to the optional combined-stream guidance: it emits the versioned `model-download-progress` semantic event directly to the exact `main` webview at a throttled rate. Event delivery failure is non-fatal to the Rust-owned download and verification job. Source URLs, hashes, local paths, and resume validators remain Rust-owned even though descriptor metadata is validated at the transport boundary; the Settings UI does not render those fields.

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

P4-001 freezes the version-one Project/Session transport records and the layout above before any meeting-content write exists. Project folders are `<portable-slug>--<project-uuid8>`; session folders are `<yyyy-mm-dd>-<portable-slug>--<session-uuid8>`. The persisted folder name is stable across later display-name edits. Portable slugs contain only lowercase ASCII letters, digits, and hyphens, are capped at 48 bytes, and use `project` or `session` when a name has no ASCII alphanumeric content. The UUID suffix supplies identity and uniqueness.

The folder contract produces only `/`-separated paths relative to the workspace, rooted below `projects/`, from validated stored identifiers and fixed document names. It accepts no path from React or another caller and performs no filesystem operation. A later writer must still pin and revalidate the workspace directory identity, reject reparse/containment changes at every sensitive open, and use handle-relative or equivalently race-resistant Windows operations under ADR 0005; string validation alone is not a filesystem sandbox.

Project and session snapshots preserve the chosen preset version, resolved microphone/system-output endpoint metadata and selection policy, transcription model, language, privacy choice, lifecycle/channel state, and optional role-specific LLM model identifiers. This freezes meeting context without enabling OpenRouter or retained audio. P4-001 adds no command, capability, database migration, UI, Markdown parser input, journal, or content write.

P4-002 adds the first content write as a Rust-only project-creation boundary. The store re-probes and canonicalizes the configured workspace, opens it and every derived directory with reparse-point-aware Windows handles that deny delete sharing, records volume/file identity, and revalidates both the path chain and pinned identity around each sensitive operation. It creates only `projects/`, the validated P4-001 project folder, and `project.md`. The snapshot contains strict version-one YAML front matter with every Project field encoded as a JSON-compatible YAML scalar or flow collection so user text cannot escape the metadata shape. A fully written and disk-synchronized `.project.md.tmp` is replaced in the same pinned directory with `MoveFileExW` replacement and write-through flags. Duplicate project creation never overwrites the existing directory; failed attempts remove only their own temporary/final file and newly created empty directories. This boundary adds no reader, update path, journal, SQLite projection, command, capability, UI, session, retained audio, or external-network operation.

P4-003 makes that project snapshot boundary readable, revisioned, and recoverable without accepting general YAML execution features. Rust reads at most 128 KiB through a reparse-aware file handle that denies write sharing, accepts LF or CRLF, and parses only the exact ordered version-one keys whose values use the P4-002 JSON-compatible YAML scalar/flow subset. Unknown, duplicate, reordered, tagged, malformed, non-UTF-8, oversized, identity-mismatched, or trailing content is rejected with fixed path-free errors. A read returns the validated Project plus a SHA-256 fingerprint of the exact bytes. Updates require the stored `expectedRevision`, that fingerprint, immutable ID/folder/creation identity, a one-step revision increment, and a nondecreasing update time. The fully synced temporary snapshot replaces `project.md` through `ReplaceFileW`, retaining the previous valid file as `project.md.bak`; the primary remains write-locked across replacement so a concurrent editor cannot win the check/write race. A valid backup restores a missing, malformed, or oversized primary, while malformed input without a valid backup is preserved for diagnosis. Recovery of an existing primary uses atomic replacement without consuming the backup; recovery of a missing primary uses no-clobber publication so a newly appearing external file is never overwritten. This task adds no discovery scan, SQLite projection, command, capability, UI, session, journal, retained audio, or network operation.

P4-004 adds a Rust-only project catalog over that snapshot boundary. Discovery enumerates at most 4,096 direct entries below the pinned `projects/` directory and never recurses. Unexpected files, invalid names, unsafe directories, unreadable or unrecoverable snapshots, and duplicate full project IDs become fixed-code issue records; issue detail is bounded to 256 safe relative entry names. Exceeding the entry limit returns no partial catalog. Every accepted document is parsed and identity-checked by the pinned store, with valid-backup recovery reported on the returned snapshot. Duplicate identities are all excluded. Accepted projects sort by parsed `updatedAt` descending, then `createdAt` descending, then stable folder name.

The catalog materializes accepted Project JSON, folder identity, exact snapshot SHA-256, and assigned sort rank into a dedicated app-local `project-index.sqlite3`; Markdown remains the only authoritative project copy. A single immediate SQLite transaction replaces the complete projection and advances its generation, so a failed rebuild preserves the previous generation. An empty index is rebuilt on first page access. A physically corrupt database/WAL family is quarantined under an opaque UUID suffix and rebuilt only from discovery results. Pages accept 1–100 records and use opaque generation-bound cursors; a rebuild between pages returns a fixed stale-page error rather than silently shifting results. This internal boundary adds no product command, capability, UI, session record, transcript journal, retained audio, or network operation.

P4-005 exposes that catalog through only four exact-`main` commands: `list_projects`, `get_project`, `create_project`, and `update_project`. Both Rust and React validate the same strict bounded request/response shapes, authorization precedes service access, and filesystem/SQLite operations run on dedicated blocking work. Workspace settings changes and project operations share one serialization boundary. Project index schema v2 stores only a SHA-256 identity of the canonical workspace, not its path, so changing workspaces invalidates and rebuilds the shared app-local projection before data is returned.

Create/update invalidates the current projection generation in an immediate transaction before mutating authoritative Markdown. Create derives UUID, stable folder identity, timestamps, and initial revision inside Rust. Update rereads the current exact-byte fingerprint and requires the caller's displayed revision before one-step revision advancement. A command acknowledges success only after a complete rebuild contains the durable snapshot. If Markdown publication succeeds but rebuild fails, Rust returns a fixed pending-refresh error and the next list/read repairs SQLite from Markdown. The Home UI requests 24 records per page and offers bounded project creation and versioned metadata editing without paths, fingerprints, raw Markdown, or generic filesystem/network/process access. Delete/export and all session persistence remain later boundaries.

P4-006 adds the Rust-only initial session persistence boundary beneath an existing authoritative project. Rust accepts a bounded resolved preset, device snapshots, transcription/optional role-model identifiers, language, objective/context, and the retain-audio preference; it derives the UUID, date-stable folder name, timestamps, revision one, idle lifecycle, stopped channel health, and zero usage. The store opens and revalidates the existing project through pinned no-delete-share handles, rejects reparse-backed descendants, and creates only `projects/<project>/sessions/<session>/session.md`. A true retain-audio preference records policy but creates neither an audio directory nor audio bytes.

The strict version-one `session.md` front matter encodes every field with JSON-compatible YAML scalars or flow collections and is capped at 512 KiB. A fully written and synchronized `.session.md.tmp` is published without replacement through same-directory `MoveFileExW` write-through semantics, so duplicates preserve the original snapshot. Fault cleanup removes only invocation-owned artifacts, including after directory creation, temporary-file synchronization, or final publication. This task adds no reader/update/recovery path, transcript journal, SQLite session projection, command, event, capability, UI, live-capture wiring, retained audio bytes, OpenRouter operation, extra window, or packaging change.

P4-007 makes that session snapshot strictly readable, revisioned, and recoverable. Rust reads at most 512 KiB through a leaf-reparse-aware handle that denies write sharing, accepts LF or CRLF, and parses only the exact ordered version-one keys and optional lifecycle timestamps produced by the canonical writer. Unknown, duplicate, reordered, tagged, malformed, non-UTF-8, oversized, identity-mismatched, or trailing input fails with fixed path-free errors. Successful reads return the validated Session and a SHA-256 fingerprint of the exact source bytes.

Updates require the displayed revision and exact-byte fingerprint, preserve `id`, `projectId`, `folderName`, and `createdAt`, advance the revision exactly once, and keep `updatedAt` nondecreasing. The validated primary remains write-locked through synchronized same-directory replacement; `ReplaceFileW` retains the previous valid snapshot as `session.md.bak`. A valid identity-matching backup restores a missing, malformed, or oversized primary while preserving external work through no-clobber publication for a missing file. Failed post-replacement verification rolls back to the last acknowledged snapshot, and torn temporary files are removed only after a valid primary read. This task adds no session discovery/SQLite projection, transcript journal, command/event/capability/UI, live-capture wiring, retained audio bytes, OpenRouter operation, extra window, or packaging change.

P4-008 adds a Rust-only session catalog over those authoritative snapshots. Discovery first accepts projects through the existing bounded project catalog, then enumerates only direct children of each pinned `sessions/` directory. Across all projects it scans at most 4,096 entries, records at most 256 safe path-free issue details, returns no partial result after the global entry ceiling, and never follows a nested directory as another discovery root. Unexpected files, invalid names, unsafe directories, unreadable snapshots, recovery failures, and duplicate full Session IDs become fixed-code issues. Every accepted session is re-read through the P4-007 identity and recovery boundary; recovered snapshots remain explicitly marked. All candidates for a duplicated full ID are excluded. Stable ordering uses parsed `startedAt` when present or `createdAt` otherwise, then `updatedAt`, project folder, and session folder.

Accepted Session JSON, project/session folder identity, exact snapshot SHA-256, and assigned sort rank are materialized into a dedicated app-local `session-index.sqlite3`; the Markdown snapshots remain authoritative. The projection stores only a SHA-256 identity for the canonical workspace, never its path. An immediate transaction replaces all rows and advances one generation, so an injected or real failure preserves the previous projection. Empty or workspace-mismatched state rebuilds before listing. Physically corrupt database/WAL files are quarantined under opaque UUID suffixes, while semantically invalid rows trigger a Markdown rebuild. Pages accept 1–100 records and use opaque `s1` generation-bound cursors; a rebuild between pages fails with a fixed stale-page code. This task adds no Tauri command, capability, UI, transcript journal/FTS, live-capture wiring, retained audio bytes, OpenRouter operation, extra window, or packaging change.

P4-009 exposes session metadata through four exact-`main` commands: `list_sessions`, `get_session`, `create_session`, and `update_session`. Lists are project-scoped, limited to twelve rows per Home request, and use cursors bound to the index generation and full project ID so a cursor cannot be replayed against another project. Rust authorizes before service access, serializes session work with workspace changes, and performs filesystem/SQLite operations on blocking work. Create derives the session ID, stable dated folder, timestamps, revision, idle lifecycle, stopped channel health, and zero usage in Rust. Update rereads the authoritative snapshot, requires the displayed revision and exact fingerprint, preserves immutable identity/lifecycle ownership, and is allowed only while the session is idle.

Before either write, the session projection is invalidated transactionally. A command acknowledges only after the durable Markdown snapshot is present in a complete rebuild; if publication succeeds but refresh fails, the command returns `session_projection_refresh_pending` and the next project-scoped list repairs the cache from Markdown. Strict Rust/Zod contracts validate every request and response. The Home surface opens one bounded project-scoped session manager, resolves fixed device snapshots only from Rust-enumerated devices, uses project defaults and the currently configured built-in technical-interview preset snapshot, and supports metadata creation/editing without receiving paths, fingerprints, or raw Markdown. It records `retainAudio` policy but creates no audio directory or bytes. Transcript journals/FTS, live-capture persistence and lifecycle commands, retained audio, OpenRouter, delete/export, extra windows, and native packaging remain later boundaries.

P4-010 freezes the Rust-only `recovery.journal` boundary. Each newline-terminated canonical JSON record carries schema version one, the authoritative Session ID, a contiguous sequence, caller-stable event UUID, RFC 3339 record time, one finalized-transcript or lifecycle mutation, and a lowercase SHA-256 checksum over the same record without its checksum field. The journal is capped at 64 MiB and 100,000 records; each encoded record is capped at 64 KiB, finalized text at 32 KiB, and language at 64 bytes. Only architecture-defined lifecycle transitions are accepted, and replay begins from `idle`.

Append opens only the derived journal beneath the pinned and revalidated session directory, excludes another writer, validates the complete existing replay and candidate mutation, assigns the next sequence in Rust, writes one record, and synchronizes the file before acknowledgement. Reusing an event UUID with identical time and mutation returns the original receipt without another write; conflicting reuse fails. Replay checks every version, Session ID, sequence, mutation, and checksum and applies each event UUID once in sequence order. Only bytes after the last newline may be treated as a torn final record; any malformed, reordered, checksum-invalid, identity-mismatched, semantically conflicting, or oversized complete record fails closed. The next append truncates an observed torn tail before adding a new synchronized record. This boundary does not yet materialize transcript Markdown, checkpoint snapshots, expose commands/events/UI, wire P3-015 live capture, retain audio, build FTS, call OpenRouter, delete/export content, add windows, or package runtime assets.

P4-011 adds the Rust-only `transcript.md` materialization boundary. The canonical LF-only document is capped at 64 MiB and contains exact ordered JSON-compatible YAML front matter for schema/document type, immutable Project and Session identities, immutable creation time, the last included journal record time, and the materialized journal sequence/checksum. Each finalized segment then renders in replay order with a millisecond timestamp heading, source label, exact text, and a fixed metadata comment carrying segment ID, stable source identifier, timeline, final status, and language. Segment text rejects carriage returns and non-tab/non-newline control characters before it can enter the journal. A frozen golden Markdown fixture makes the complete byte representation explicit.

Reading a transcript validates the strict checkpoint shape, replays the journal through that sequence, verifies the checkpoint checksum, and requires the replay to reproduce every snapshot byte exactly. Materialization accepts no caller path, opens only the derived file through the pinned Session directory, caps all reads, rejects leaf reparse points, and requires the exact fingerprint of an existing valid snapshot before replacement. It synchronizes a same-directory temporary file, uses write-through atomic publication or `ReplaceFileW`, retains the previous acknowledged version as `transcript.md.bak`, and rereads the result before acknowledgement. A valid journal-verifiable backup can recover a missing, malformed, or oversized primary without consuming the backup; a failed post-replacement verification restores the last acknowledged snapshot. Deleting the derived Markdown and replaying the same journal produces identical bytes. This boundary does not add FTS/SQLite projection, commands/events/UI, live-capture wiring or batching policy, retained audio, OpenRouter, delete/export, extra windows, or packaging.

P4-012 adds the Rust-only rebuildable transcript search projection. Discovery begins only from sessions accepted by the bounded Session catalog and accepts transcript content only after the P4-011 reader verifies its exact Markdown bytes against the checksummed journal prefix. It carries forward bounded Session issues, records at most 256 safe relative issue names, accepts valid empty transcripts, and fails closed rather than returning a partial projection above 100,000 finalized segments or 64 MiB of aggregate segment text.

The dedicated app-local `transcript-index.sqlite3` schema stores one validated finalized segment per row and uses an external-content FTS5 table with the Unicode tokenizer. Rows retain only Project/Session/segment identity, stable source, timeline, language, bounded text, journal checkpoint sequence/checksum, and the exact transcript SHA-256. An immediate transaction replaces every content row, rebuilds FTS, stores a deterministic SHA-256 over the complete ordered projection, and advances a workspace-bound generation. SQLite never becomes the authoritative transcript copy. Physical corruption quarantines the database/WAL family under an opaque suffix; workspace mismatch, row/FTS count mismatch, digest mismatch, and semantically invalid result rows rebuild from Markdown. A failed rebuild preserves the prior generation, and future schema versions are preserved rather than downgraded.

Internal search requires a Project ID, may narrow to one Session, accepts 1–100 results, a 1–256-byte control-free query of at most sixteen terms, and an opaque `t1` cursor bound to generation, Project, optional Session, and the normalized query hash. Terms are safely quoted into an AND expression. Results order by FTS rank, timeline, and segment identity and return only validated identity/source/timeline/language plus a whitespace-normalized snippet capped at 240 characters. Rebuilds make old cursors fail as stale. This task adds no command/event/capability/UI, live-capture integration, retained audio, OpenRouter, delete/export, extra window, or packaging change.

P4-013 exposes saved transcript reading and search through two exact-`main` read-only commands: `get_transcript_page` and `search_transcript`. Rust authorizes the invoking window before service access, validates the shared bounded transport contracts, serializes each operation with workspace selection, and performs every Markdown/SQLite operation on blocking work. A transcript page revalidates the authoritative Project and Session, reads only the P4-011 journal-verifiable snapshot, returns at most 100 finalized segments, and uses an opaque `r1` cursor bound to the full Project/Session scope, exact transcript SHA-256, and an offset no greater than 100,000. Snapshot replacement makes an old page fail as stale rather than mixing versions.

Search preserves the P4-012 Project-required/optional-Session contract, 256-byte/sixteen-term query ceiling, 100-result ceiling, and generation/query/scope-bound `t1` cursor. The project-scoped Home session manager requests 50 transcript segments or 20 search hits per page and retains at most ten loaded pages per query. It receives no Markdown bytes, fingerprints, paths, journal records, SQLite metadata, or issue details. Finalized text and snippets render only through React's escaped text nodes with whitespace preservation; HTML, Markdown links, images, and embedded content never activate. This boundary adds no mutation, event, live-capture/materialization scheduling, lifecycle recovery, retained audio, OpenRouter, delete/export, extra window, or packaging behavior.

P4-014 adds the Rust-only persisted-session writer coordinator around the existing journal, transcript materializer, and search catalog. One mutable coordinator is bound to one validated Session locator and accepts only P4-010 finalized-segment and lifecycle mutations. The synchronized journal append remains the durable acknowledgement boundary: an identical event-ID retry returns its original sequence/checksum and does not advance the in-memory batch, while a transcript conflict or search-refresh failure is returned as a fixed derived-projection state without preventing later journal appends.

The coordinator caps its pending-final counter at five, records a monotonic caller-clock deadline two seconds after the first pending final, materializes on that deadline, on the fifth final, on an explicit flush, and at every lifecycle boundary, then rebuilds P4-012 only after the new transcript snapshot is acknowledged. Reopening conservatively compares the journal replay with the verified transcript checkpoint, immediately materializes any gap, and refreshes the rebuildable index even when the snapshot is already current; this repairs crashes after either the journal or snapshot acknowledgement boundary. External Markdown conflicts preserve the untrusted file and leave the synchronized journal available for later repair. P4-014 does not yet own a background timer, expose lifecycle commands/events/UI, update `session.md`, connect P3-015 capture/transcription, retain audio, call OpenRouter, delete/export, add windows, or package runtime assets.

P4-015 makes that writer the product owner of one persisted live Session. Four exact-`main` commands start, pause, resume, and stop an existing Session with optimistic revision checks; start and resume additionally require explicit capture consent and a fresh request UUID. Rust uses the Session's frozen microphone/system-output selections and transcription model rather than mutable settings, starts only one persisted run, and routes the existing P3 partial/final/gap stream through Project/Session-scoped events. A final is appended and synchronized through P4-014 before its UI event is emitted. A 100 ms owner-local poll drives the accepted two-second materialization deadline while the run is active, and lifecycle commands stop/drain the transcription owner before appending their durable journal boundary.

Authoritative `session.md` advances only after its corresponding journal boundary is synchronized. Start records `idle|paused -> preparing -> capturing -> transcribing`; pause records `transcribing -> paused`; stop records `transcribing|paused -> stopping -> completed`; runtime failure records `failed`. Startup scans authoritative Sessions and journals before serving commands: an interrupted transcribing journal is durably paused, preparing/capturing/stopping is failed, terminal journal state is reconciled directly, and the Session is marked with fixed `recovery_required` detail before the rebuildable session index is refreshed. Persistence status events expose only scoped journal/snapshot sequence and fixed clean/deferred/conflict/failed state. This boundary retains no audio bytes, calls no external model service, adds no transcript editing/bookmarks, delete/export, extra window, or native-runtime package.

Markdown owns important project/session content. SQLite owns rebuildable projections, FTS5, non-secret settings, window/device preferences, model/download state, detailed metrics/cost entries, sanitized errors, and migrations. Windows Credential Manager alone owns the API key.

The per-session writer appends checksummed sequenced journal entries and syncs final segments, edits, bookmarks, and state changes before acknowledgement. It materializes Markdown after two seconds or five final segments and immediately at lifecycle boundaries. Snapshot replacement uses a same-directory temporary file, disk synchronization, an atomic Windows replacement, and `.bak` recovery.

Startup validates journal records, tolerates only a torn final record, replays after the Markdown checkpoint, and exposes interrupted work as paused with `recovery_required`. SQLite corruption is handled by preserving and rebuilding the projection. An external Markdown hash conflict pauses snapshot replacement while journaling continues.

Markdown-derived React UI uses the fixed `SanitizedMarkdown` boundary defined by [ADR 0006](adr/0006-untrusted-markdown-rendering-boundary.md). It ignores raw HTML, applies an explicit sanitization schema, excludes images and embedded content, rejects unsafe address schemes, and keeps allowed external links inert. Reading files and validating YAML front matter remain Rust-owned persistence responsibilities.

## OpenRouter and prompts

OpenRouter integration is Rust-only and text-only. LLM features start disabled. Enabling them shows an external-service indicator. Zero-data-retention/provider data-collection restrictions are enabled by default; relaxing them requires explicit confirmation.

P5-001 stores the optional API key as a Windows Credential Manager generic credential under the fixed application target `KokoroKoe_OpenRouter_API_Key`, using the current user's credential set and `CRED_PERSIST_LOCAL_MACHINE`. The key crosses React only from a password input's component-local state into the exact-main `set_openrouter_api_key` invocation, then remains Rust/OS-owned; it is never returned, emitted, logged, placed in TanStack Query or Zustand, written to Markdown/SQLite/app files, or exposed through generic frontend permissions. Rust validates a bounded control-free value without assuming an OpenRouter prefix, overwrites its owned UTF-8 buffer on drop, and exposes only configured status plus idempotent removal. `validatedAt` remains absent until a later task performs an explicit text-only OpenRouter validation request; configured does not mean validated.

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
