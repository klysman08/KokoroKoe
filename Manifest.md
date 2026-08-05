# Development of a Windows Application for Real-Time Transcription and Meeting Assistance

Act as a senior software architect and developer specialized in Rust, Tauri, React, and Windows audio applications.

We are going to build a Windows desktop application capable of:

* Capturing the user's microphone audio and the system audio output simultaneously.
* Transcribing both audio channels locally and in real time.
* Organizing meetings and transcriptions by projects and sessions.
* Generating summaries, highlights, answers, and real-time insights using language models accessed through OpenRouter.
* Keeping audio and transcription data under the user's control, prioritizing privacy and local processing.

Before implementing anything, review the following technical references:

* Handy, as a reference for managing and running local transcription models:
  https://github.com/cjpais/handy
* OpenRouter:
  https://openrouter.ai/docs/quickstart
* Tauri:
  https://github.com/tauri-apps/tauri
* shadcn/ui with Vite:
  https://ui.shadcn.com/docs/installation/vite

Do not directly copy Handy's architecture or source code. Use it only as a technical and user-experience reference, while respecting its license.

## 1. Product Objective

The application will function as a real-time meeting assistant.

During a session, it must:

1. Capture microphone audio.
2. Capture Windows system output audio separately.
3. Transcribe both streams locally.
4. Visually identify the speaker based on the audio source:

   * “You” for microphone input.
   * “System” or “Participant” for system output.
5. Send only transcribed text, never audio, to OpenRouter.
6. Generate partial summaries, suggested answers, important points, pending questions, and other insights.
7. Save all generated content locally as Markdown files.

The MVP will target Windows only.

Do not implement macOS or Linux support in the first version, but avoid architectural decisions that would prevent future cross-platform support.

## 2. Required Technology Stack

Use the following technologies:

* Tauri 2.
* Rust for the backend.
* Vite.
* React.
* TypeScript in strict mode.
* shadcn/ui.
* Tailwind CSS.
* pnpm.
* Zustand or another lightweight global state management solution.
* TanStack Query only where it provides clear value for asynchronous operations.
* Zod for configuration and data validation.
* OpenRouter for LLM access.
* Whisper or a compatible local transcription implementation.
* Local storage for settings and metadata.
* Markdown with YAML front matter for generated documents.

For frontend initialization, verify the current shadcn CLI syntax before running commands.

The intended command is similar to:

```bash
pnpm dlx shadcn@latest init --preset b0 --template vite
```

If this preset or argument is no longer valid, use the currently recommended equivalent and document the change.

## 3. Architectural Principles

Organize the application into independent modules:

* Audio capture.
* Audio processing and normalization.
* Voice activity detection.
* Local transcription.
* Model management and downloads.
* Project and session management.
* Document persistence.
* OpenRouter integration.
* Prompt construction.
* Insight generation.
* Desktop UI and window management.
* Settings and secure credential storage.
* Local usage metrics.

The frontend must not directly access sensitive files, credentials, or external APIs.

Sensitive operations must be handled through Rust commands exposed by Tauri, using the minimum required permissions.

Define clear Rust and TypeScript contracts for events such as:

* `audio-level-updated`
* `transcription-partial`
* `transcription-final`
* `session-status-changed`
* `insight-generated`
* `summary-updated`
* `model-download-progress`
* `application-error`

Do not place all application logic in a single file, React component, or Tauri command.

## 4. Audio Capture

The application must detect and list:

* Available microphones.
* Available output devices.
* The default input device.
* The default output device.

Audio capture must maintain two independent logical channels:

```text
microphone
system_output
```

Requirements:

* Allow the user to select the input device.
* Allow the user to select the output device to monitor.
* Display an audio-level meter for each channel.
* Allow devices to be tested before starting a session.
* Detect silence and avoid transcribing empty segments.
* Maintain consistent timestamps between both audio streams.
* Clearly notify the user when a device is disconnected.
* Attempt to recover capture when the default device changes.
* Do not permanently store audio files by default.
* Provide an optional setting to retain the original session audio.
* Clearly document any technical limitations related to Windows system-audio capture.

Do not implement full speaker diarization in the MVP.

The initial separation between speakers will be based on the audio source.

## 5. Local Transcription

The user must be able to download and manage different local transcription models.

The model management interface must display:

* Model name.
* Download size.
* Disk space used.
* Supported languages.
* Estimated performance.
* Approximate memory requirements.
* Download status.
* Installation status.
* An option to delete the model.
* The default selected model.

Initially support Whisper-compatible models designed for local execution.

The architecture must allow additional transcription engines to be added in the future.

The user must be able to select the transcription model:

* In global settings.
* When creating or starting a session.

Transcription output must include:

* Partial results.
* Final results.
* Audio-source identification.
* Start and end timestamps.
* Detected or configured language.
* Confidence indicator when available.
* Error recovery without terminating the entire session.
* A processing queue with backpressure control if transcription becomes slower than incoming audio.

Use hardware acceleration when available, while maintaining a CPU fallback.

## 6. OpenRouter and LLM Models

OpenRouter will be used only to analyze text.

The application must allow the user to:

* Enter and validate an API key.
* Store the API key securely on Windows.
* Search or list available models.
* Select different models for:

  * Fast real-time insights.
  * Session summaries.
  * Manual questions.
* Display relevant model information when available:

  * Provider.
  * Model name.
  * Context window.
  * Approximate price.
  * Streaming support.
* Define a spending limit per session.
* Define a maximum token limit.
* Use response streaming where it improves the user experience.
* Cancel active requests.
* Handle timeouts, rate limits, insufficient balance, and provider failures.
* Implement limited retries with exponential backoff.
* Never include the API key in logs.
* Never store the API key in Markdown files or plain text.

Use OpenRouter's OpenAI-compatible API.

The integration must allow models to be changed without affecting the rest of the application architecture.

## 7. Projects, Sessions, and Context

Every session must belong to a project.

### Project

A project must include:

* ID.
* Name.
* Description.
* Global context.
* Optional participants.
* Tags.
* Creation date.
* Last updated date.
* Local folder.
* Default preset.
* Default transcription model.
* Preferred LLM models.

Example projects:

* Backend engineer hiring process.
* Weekly company meetings.
* Product discovery.
* Academic research.
* Customer support.

### Session

Before starting a session, the user must define:

* Title.
* Project.
* Objective.
* Session-specific context.
* Preset.
* Language.
* Audio devices.
* Local transcription model.
* LLM model for insights.
* LLM model for summaries.
* Audio-retention preference.

The final context sent to the LLM must be composed of:

1. Global project context.
2. Session-specific context.
3. Preset instructions.
4. Relevant transcript excerpts.
5. The accumulated session summary.
6. The requested output type.

Do not resend the complete transcript with every request.

Implement a strategy based on:

* A sliding context window.
* An accumulated summary.
* Retrieval of relevant transcript segments.
* Token and cost controls.

## 8. Presets

Initially include the following presets:

* Technical interview.
* Behavioral interview.
* Business meeting.
* Sales meeting.
* Product meeting.
* Brainstorming.
* Class or lecture.
* Customer support.
* Custom preset.

Each preset must define:

* The assistant's expected role.
* Analysis objectives.
* Insight types.
* Response tone.
* Final summary structure.
* Information that should be highlighted.
* Information or behaviors that should be avoided.

Allow users to create, edit, duplicate, export, and delete custom presets.

## 9. Main User Interfaces

The application will have three primary areas.

Native Tauri windows may be used where appropriate.

### 9.1. Home and Settings

The Home view must include:

* Project list.
* Recent sessions.
* A button to start a new session.
* Access to local model management.
* OpenRouter configuration.
* Audio-device configuration.
* Storage-folder configuration.
* Preset management.
* General preferences.

The dashboard must display local metrics such as:

* Number of sessions.
* Total transcribed time.
* Most frequently used projects.
* Most frequently used Whisper model.
* Most frequently used LLM models.
* Input and output token usage.
* Estimated cost by period.
* Number of generated insights.
* Disk space used.
* Recent errors.

Do not implement external telemetry in the MVP.

All metrics must remain local.

### 9.2. Real-Time Transcription Window

The transcription window must:

* Display the conversation in chronological order.
* Clearly differentiate microphone input from system output.
* Display timestamps.
* Visually identify partial transcription results.
* Replace partial results with final results.
* Auto-scroll while the user is viewing the latest content.
* Stop auto-scrolling when the user navigates to older messages.
* Provide a button to return to the latest transcript segment.
* Allow transcription to be paused and resumed.
* Allow manual bookmarks.
* Allow finalized segments to be edited.
* Allow transcript segments to be copied.
* Allow searching within the session.
* Allow the user to ask the LLM about a specific segment.

Each transcript block must provide actions such as:

* Ask about this segment.
* Suggest a response.
* Explain.
* Summarize.
* Mark as important.
* Copy.
* Correct text.

When asking about a segment, send the LLM:

* The selected segment.
* A limited number of previous and following segments.
* Session context.
* A summarized version of the conversation.

### 9.3. Insights Window

The insights window must generate recommendations based on the latest transcript segments.

Initial insight types:

* Suggested response.
* Follow-up questions.
* Points requiring clarification.
* Facts or numbers mentioned.
* Risks and objections.
* Decisions made.
* Tasks and owners.
* Contradictions or inconsistencies.
* Topics not yet addressed.

The interface must:

* Display one insight at a time or use insight cards.
* Allow navigation between previous and next insights.
* Allow an insight to be pinned.
* Allow an insight to be dismissed.
* Allow an insight to be copied.
* Allow an alternative version to be generated.
* Show which transcript segment the insight is related to.
* Differentiate provisional insights from confirmed insights.
* Avoid repeatedly generating the same insight.

Insights must be useful, concise, and relevant.

They must not interrupt the user with unnecessary updates after every sentence.

## 10. Window Controls

The transcription and insights windows must support:

* Independent opacity controls.
* An “always on top” option.
* Resizing.
* Persistent position.
* Persistent dimensions.
* Compact mode.
* A quick-hide option.
* Configurable keyboard shortcuts to show or hide the windows.
* An optional click-through mode, if technically viable and safe.
* Monitor preference for multi-display environments.

Opacity should affect the window background without significantly reducing text readability.

## 11. Markdown Persistence

All generated documents must be stored locally as Markdown files.

Suggested structure:

```text
workspace/
  projects/
    <project-slug>/
      project.md
      presets/
      sessions/
        <yyyy-mm-dd-session-slug>/
          session.md
          transcript.md
          summary.md
          insights.md
          actions.md
          questions.md
          audio/
```

Each document must include YAML front matter.

Example for `transcript.md`:

```yaml
---
schema_version: 1
document_type: transcript
project_id: "project-uuid"
session_id: "session-uuid"
title: "Weekly product meeting"
created_at: "2026-08-05T14:00:00Z"
updated_at: "2026-08-05T15:20:00Z"
language: "en-US"
transcription_engine: "whisper"
transcription_model: "model-id"
microphone_device: "device-id"
output_device: "device-id"
participants:
  - "User"
tags:
  - product
  - planning
---
```

Each transcript entry must preserve:

* ID.
* Channel.
* Timestamp.
* Text.
* Partial or final status.
* Last edited date.
* Optional reference to the original audio file.

Example:

```markdown
## 00:03:14 — Participant

We need to finish the first version by Friday.

<!--
segment_id: segment-uuid
source: system_output
start_ms: 194000
end_ms: 198500
status: final
-->
```

The `summary.md` document must include:

* Executive summary.
* Main topics.
* Decisions.
* Action items.
* Owners.
* Deadlines.
* Risks.
* Open questions.
* Next steps.

Saving must be incremental and resilient.

A crash or unexpected application shutdown must not delete the entire session.

Use atomic writes, temporary files, journaling, or an equivalent strategy to reduce the risk of file corruption.

## 12. Local Database and Index

Markdown documents will be the portable and human-readable source of truth for important user content.

A lightweight local database such as SQLite may also be used for:

* Indexing.
* Search.
* Relationships between projects and sessions.
* Settings.
* Cache.
* Download state.
* Metrics.
* Fast UI loading.

The database must not be the only copy of transcripts and important generated documents.

Clearly document which data is stored in SQLite and which data is stored in Markdown files.

## 13. Security and Privacy

Mandatory requirements:

* Process audio and transcription locally.
* Send only necessary text to OpenRouter.
* Clearly indicate whenever content is being sent to an external service.
* Allow LLM features to be completely disabled.
* Store the API key using a secure operating-system mechanism.
* Apply minimum required Tauri permissions.
* Validate all file-system paths.
* Prevent path traversal.
* Never execute content obtained from transcripts.
* Sanitize untrusted Markdown before rendering it.
* Never include secrets in logs, reports, or error messages.
* Allow projects and sessions to be permanently deleted.
* Allow data to be exported without depending on the application.
* Do not include external telemetry without explicit user consent.

Display a notice explaining that users are responsible for complying with applicable consent, recording, and transcription laws.

## 14. States and Error Handling

Explicitly model the session states:

```text
idle
preparing
capturing
transcribing
paused
stopping
processing_summary
completed
failed
```

The interface must handle:

* No microphone available.
* System-output capture failure.
* Device disconnection.
* Missing transcription model.
* Incompatible model.
* Insufficient memory.
* Download failure.
* Transcription failure.
* Invalid API key.
* Insufficient OpenRouter balance.
* Rate limiting.
* Timeout.
* No internet connection.
* Invalid model response.
* Folder without write permission.
* Full disk.
* Corrupted Markdown document.

Errors must be displayed in clear language.

The user must also be able to view technical details and copy a sanitized error report.

## 15. Performance

Initial MVP goals:

* The UI must not freeze during audio capture or model inference.
* Audio capture, transcription, persistence, and LLM requests must run outside the main UI thread.
* Partial transcription must appear with low latency, according to the capabilities of the user's hardware.
* Memory usage must be monitored when loading models.
* Only one heavy model should remain loaded when memory is limited.
* Downloads should be resumable where possible.
* The application must continue saving transcripts when OpenRouter is unavailable.
* Insight generation must never block transcription.

## 16. Prompt System

Create a dedicated prompt-construction module.

Do not spread prompt strings across React components.

Each prompt must have:

* ID.
* Version.
* Purpose.
* Expected variables.
* Approximate context limit.
* Output schema.
* Fallback strategy.

Whenever possible, request structured JSON output and validate it before updating application state.

Example insight schema:

```ts
type Insight = {
  id: string
  type:
    | "suggested_response"
    | "follow_up_question"
    | "risk"
    | "decision"
    | "action_item"
    | "clarification"
    | "contradiction"
  title: string
  content: string
  rationale?: string
  relatedSegmentIds: string[]
  confidence?: number
  createdAt: string
}
```

Instructions spoken during the meeting must not override internal application instructions.

Treat transcript content as untrusted input to reduce prompt-injection risks.

## 17. MVP Scope

The MVP must include:

1. Project creation.
2. Session creation and startup.
3. Microphone and output-device selection.
4. Independent capture of both audio channels.
5. Download and selection of at least two local transcription models.
6. Partial and final transcription.
7. Visual differentiation by audio source.
8. Secure OpenRouter API-key configuration.
9. LLM model selection.
10. Manual questions about a transcript segment.
11. Generation of recent insights.
12. Final summary generation.
13. Markdown persistence.
14. Independent window-opacity controls.
15. Basic recovery after unexpected shutdown.
16. A basic local dashboard.

Do not include the following in the initial MVP:

* Cloud collaboration.
* Login or user accounts.
* Cross-device synchronization.
* Mobile applications.
* Advanced speaker diarization.
* Direct Zoom, Microsoft Teams, or Google Meet integrations.
* Bots that automatically join meetings.
* Plugin marketplace.
* Model training or fine-tuning.
* Remote telemetry.
* A proprietary cloud backend.

## 18. Implementation Phases

Execute the project in phases.

### Phase 1 — Planning

Before writing code:

* Analyze the requirements.
* List technical risks.
* Propose the architecture.
* Define the directory structure.
* Define the data models.
* Define events between Rust and React.
* Define the Windows audio-capture strategy.
* Define the Whisper execution strategy.
* Define the persistence strategy.
* Identify dependencies and their licenses.
* Separate confirmed decisions from assumptions.

### Phase 2 — Foundation

* Initialize Tauri, Vite, React, and TypeScript.
* Configure shadcn/ui.
* Configure linting, formatting, and tests.
* Create navigation and the application layout.
* Implement global state management.
* Implement settings storage.
* Implement sanitized logging.
* Configure minimum required capabilities and permissions.

### Phase 3 — Audio and Transcription

* List available devices.
* Implement input and output device tests.
* Capture both audio channels.
* Create the audio-processing pipeline.
* Integrate a local transcription model.
* Implement partial and final results.
* Display real-time transcription.

### Phase 4 — Projects and Persistence

* Create projects and sessions.
* Implement the folder structure.
* Implement YAML front matter.
* Implement incremental saving.
* Implement session recovery.
* Implement basic local search.

### Phase 5 — OpenRouter and Insights

* Implement secure API-key storage.
* Implement model selection.
* Implement versioned prompts.
* Implement questions about transcript segments.
* Implement real-time insights.
* Implement accumulated and final summaries.
* Implement token and cost controls.

### Phase 6 — Desktop Experience

* Implement multiple windows.
* Implement opacity controls.
* Implement “always on top.”
* Implement compact mode.
* Implement keyboard shortcuts.
* Persist window positions and dimensions.

### Phase 7 — Quality

* Unit tests.
* Integration tests.
* Audio-pipeline tests.
* Persistence tests.
* Recovery tests.
* Offline tests.
* Tests with different audio devices.
* Tests across different hardware capabilities.
* Tauri permission and security tests.

## 19. MVP Acceptance Criteria

The MVP will be considered functional when:

* The user can create a project and a session.
* The application can detect a microphone and an output device.
* Both audio channels can be captured separately.
* Speech appears in the UI with a source label and timestamp.
* The user can download and select a local model.
* Transcription continues working without internet access.
* The user can configure OpenRouter and select a model.
* A question about a transcript segment returns a contextualized answer.
* Insights are generated without blocking transcription.
* Markdown files are generated when the session ends.
* The summary includes decisions, action items, and open questions.
* The application can recover an interrupted session without losing the entire transcript.
* The API key never appears in files, logs, or error messages.
* The opacity of the transcription and insights windows can be controlled independently.
* OpenRouter being unavailable does not prevent local capture and transcription.

## 20. Expected Response Format During Development

While working on this project:

1. Do not attempt to implement the entire application at once.
2. Begin by presenting the architecture and implementation plan.
3. Explain important technical decisions.
4. List the files that will be created or modified.
5. Generate complete, typed, and executable code.
6. Avoid pseudocode when a real implementation is possible.
7. Do not invent library APIs.
8. Validate APIs and versions using official documentation.
9. Do not hide technical limitations.
10. Run or describe verifiable tests for each phase.
11. At the end of each phase, provide:

    * What was completed.
    * How to test it.
    * Known limitations.
    * The recommended next step.

Start with Phase 1.

Deliver:

* A summary of the product requirements.
* The proposed architecture.
* A textual component diagram.
* The data flow from audio capture to Markdown persistence.
* The directory structure.
* The main data models.
* Tauri commands and events.
* Suggested dependencies.
* Technical risks.
* Decisions that require prototype validation.
* An incremental implementation plan.
