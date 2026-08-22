mod credentials;
mod error;
mod insights;
mod manual_question;
mod models;
mod openrouter;
mod projects;
mod settings;
mod summaries;
mod transcription;
mod transcripts;

pub(crate) use credentials::{CredentialStatus, OpenRouterApiKey};
pub(crate) use error::{AppError, CommandError};
pub(crate) use insights::{
    GenerateRecentInsightsRequest, InsightUsage, RecentInsight, RecentInsightsResponse,
};
pub(crate) use manual_question::{
    AskManualQuestionRequest, ManualQuestionResponse, ManualQuestionUsage,
};
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
pub(crate) use summaries::{
    GenerateSessionSummaryRequest, GenerateSessionSummaryResponse, GetSessionSummaryRequest,
    SessionSummaryContent, SessionSummaryDocument, SessionSummaryStatus, SummaryActionItem,
    SummaryUsage,
};
pub(crate) use transcription::{
    LiveEventEnvelope, LiveSegmentStatus, LiveTranscriptSegment, LiveTranscriptionInput,
    LiveTranscriptionRunState, LiveTranscriptionStatus, TranscriptionFinalPayload,
    TranscriptionGapPayload, TranscriptionPartialPayload,
};
pub(crate) use transcripts::{
    TranscriptPage, TranscriptPageRequest, TranscriptSearchHit, TranscriptSearchPageView,
    TranscriptSearchQuery, TranscriptSegmentStatus, TranscriptSegmentView,
};
