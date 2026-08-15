mod credentials;
mod error;
mod models;
mod openrouter;
mod projects;
mod settings;
mod transcription;
mod transcripts;

pub(crate) use credentials::{CredentialStatus, OpenRouterApiKey};
pub(crate) use error::{AppError, CommandError};
#[cfg(test)]
pub(crate) use models::ModelContractFixture;
pub(crate) use models::{
    EventEnvelope, ModelBackend, ModelCompatibility, ModelDescriptor, ModelDownloadJob,
    ModelDownloadProgress, ModelDownloadStatus, ModelInstallation, ModelInstallationStatus,
    PerformanceClass, RequestId, now_rfc3339,
};
pub(crate) use openrouter::{
    CredentialValidation, OpenRouterDataCollection, OpenRouterModel, validate_rfc3339,
};
pub(crate) use projects::{
    AudioDeviceSnapshot, CreateProjectInput, CreateSessionSnapshotInput, InsightType,
    LlmRoleModels, PageRequest, PresetSnapshot, Project, ProjectId, ProjectPage, Session,
    SessionId, SessionPage, SessionState, UpdateProjectInput, UpdateSessionInput,
};
pub(crate) use settings::{AppSettings, AppSettingsUpdate, PresetId, Versioned, WorkspaceStatus};
pub(crate) use transcription::{
    LiveEventEnvelope, LiveSegmentStatus, LiveTranscriptSegment, LiveTranscriptionInput,
    LiveTranscriptionRunState, LiveTranscriptionStatus, TranscriptionFinalPayload,
    TranscriptionGapPayload, TranscriptionPartialPayload,
};
pub(crate) use transcripts::{
    TranscriptPage, TranscriptPageRequest, TranscriptSearchHit, TranscriptSearchPageView,
    TranscriptSearchQuery, TranscriptSegmentStatus, TranscriptSegmentView,
};
