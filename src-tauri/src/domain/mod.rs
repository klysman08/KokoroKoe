mod error;
mod models;
mod projects;
mod settings;
mod transcription;

pub(crate) use error::{AppError, CommandError};
#[cfg(test)]
pub(crate) use models::ModelContractFixture;
pub(crate) use models::{
    EventEnvelope, ModelBackend, ModelCompatibility, ModelDescriptor, ModelDownloadJob,
    ModelDownloadProgress, ModelDownloadStatus, ModelInstallation, ModelInstallationStatus,
    PerformanceClass, RequestId, now_rfc3339,
};
#[allow(unused_imports)]
pub(crate) use projects::CreateSessionSnapshotInput;
pub(crate) use projects::{
    CreateProjectInput, LlmRoleModels, PageRequest, Project, ProjectId, ProjectPage, Session,
    SessionId, UpdateProjectInput,
};
pub(crate) use settings::{AppSettings, AppSettingsUpdate, PresetId, Versioned, WorkspaceStatus};
pub(crate) use transcription::{
    LiveEventEnvelope, LiveSegmentStatus, LiveTranscriptSegment, LiveTranscriptionInput,
    LiveTranscriptionRunState, LiveTranscriptionStatus, TranscriptionFinalPayload,
    TranscriptionGapPayload, TranscriptionPartialPayload,
};
