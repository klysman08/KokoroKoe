pub(crate) mod layout;
mod project_index;
mod project_service;
mod project_store;
mod session_index;
mod session_journal;
mod session_lifecycle;
mod session_service;
mod session_store;
#[allow(dead_code)] // P4-014 is a Rust-only coordinator until lifecycle commands are wired.
mod session_writer;
mod settings_store;
mod transcript_index;
mod transcript_service;
mod transcript_store;

#[allow(unused_imports)]
pub(crate) use project_index::{ProjectCatalog, ProjectIndexRebuildReport};
pub(crate) use project_service::ProjectService;
#[allow(unused_imports)]
pub(crate) use project_store::{ProjectCreateReceipt, ProjectStore, ProjectStoreError};
#[allow(unused_imports)]
pub(crate) use project_store::{ProjectLocator, ProjectSnapshot, ProjectSnapshotFingerprint};
#[allow(unused_imports)]
pub(crate) use session_index::{SessionCatalog, SessionIndexRebuildReport};
#[allow(unused_imports)]
pub(crate) use session_journal::{
    FinalizedTranscriptSegment, JournalAppend, JournalMutation, JournalReplay, LifecycleChange,
    SessionJournal, SessionJournalError,
};
pub(crate) use session_lifecycle::{PersistedSessionEvent, PersistedSessionLifecycleService};
pub(crate) use session_service::SessionService;
#[allow(unused_imports)]
pub(crate) use session_store::{
    DiscoveredSession, SessionCreateReceipt, SessionDiscoveryIssue, SessionDiscoveryReport,
    SessionLocator, SessionSnapshot, SessionSnapshotFingerprint, SessionStore, SessionStoreError,
};
#[allow(unused_imports)]
pub(crate) use session_writer::{
    PersistedSessionWriter, SessionWriterError, SessionWriterProjection, SessionWriterReceipt,
};
pub(crate) use settings_store::SettingsService;
#[allow(unused_imports)]
pub(crate) use transcript_index::{
    TranscriptSearchCatalog, TranscriptSearchPage, TranscriptSearchRequest, TranscriptSearchResult,
};
pub(crate) use transcript_service::TranscriptService;
#[allow(unused_imports)]
pub(crate) use transcript_store::{
    TranscriptSnapshot, TranscriptSnapshotFingerprint, TranscriptStore, TranscriptStoreError,
};
