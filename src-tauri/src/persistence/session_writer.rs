use std::{fmt, path::Path};

use super::{
    session_journal::{JournalAppend, JournalReceipt, SessionJournal},
    session_store::SessionLocator,
    transcript_index::TranscriptSearchCatalog,
    transcript_store::{TranscriptSnapshotFingerprint, TranscriptStore},
};

const SNAPSHOT_INTERVAL_MS: u64 = 2_000;
const SNAPSHOT_SEGMENT_BATCH: u8 = 5;
const MAX_SAFE_TICK_MS: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionWriterError {
    pub(crate) code: &'static str,
}

impl SessionWriterError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for SessionWriterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for SessionWriterError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionWriterProjection {
    Current {
        checkpoint_sequence: u64,
        index_generation: Option<u64>,
    },
    Deferred {
        checkpoint_sequence: u64,
        pending_final_segments: u8,
        flush_deadline_ms: u64,
    },
    SnapshotPending {
        checkpoint_sequence: u64,
        pending_final_segments: u8,
        code: &'static str,
    },
    IndexPending {
        checkpoint_sequence: u64,
        code: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionWriterReceipt {
    pub(crate) sequence: u64,
    pub(crate) checksum: String,
    pub(crate) already_committed: bool,
    pub(crate) projection: SessionWriterProjection,
}

pub(crate) struct PersistedSessionWriter {
    journal: SessionJournal,
    transcripts: TranscriptStore,
    search: TranscriptSearchCatalog,
    locator: SessionLocator,
    snapshot_fingerprint: Option<TranscriptSnapshotFingerprint>,
    checkpoint_sequence: u64,
    pending_final_segments: u8,
    pending_since_ms: Option<u64>,
    last_tick_ms: u64,
    index_dirty: bool,
    index_generation: Option<u64>,
    snapshot_error: Option<&'static str>,
    index_error: Option<&'static str>,
}

impl PersistedSessionWriter {
    pub(crate) fn open(
        workspace_path: &Path,
        app_data_directory: &Path,
        locator: SessionLocator,
        now_ms: u64,
    ) -> Result<Self, SessionWriterError> {
        validate_initial_tick(now_ms)?;
        let journal = SessionJournal::open(workspace_path).map_err(map_journal_error)?;
        let replay = journal.replay(&locator).map_err(map_journal_error)?;
        let transcripts = TranscriptStore::open(workspace_path).map_err(map_transcript_error)?;
        let search =
            TranscriptSearchCatalog::open(workspace_path, app_data_directory.to_path_buf())
                .map_err(map_transcript_error)?;
        let existing = transcripts.read_transcript(&locator).ok();
        let mut writer = Self {
            journal,
            transcripts,
            search,
            locator,
            snapshot_fingerprint: existing.as_ref().map(|snapshot| snapshot.fingerprint),
            checkpoint_sequence: existing
                .as_ref()
                .map_or(0, |snapshot| snapshot.checkpoint_sequence),
            pending_final_segments: 0,
            pending_since_ms: None,
            last_tick_ms: now_ms,
            index_dirty: true,
            index_generation: None,
            snapshot_error: None,
            index_error: None,
        };

        // Reopening is deliberately conservative: any journal gap is materialized immediately,
        // and even a current snapshot refreshes the rebuildable index. This repairs crashes at
        // either derived-projection boundary without another transcription event.
        if replay.last_sequence != writer.checkpoint_sequence
            || writer.snapshot_fingerprint.is_none()
        {
            writer.flush_projection(None)?;
        } else {
            writer.refresh_index(None)?;
        }
        Ok(writer)
    }

    pub(crate) fn projection(&self) -> SessionWriterProjection {
        if let Some(code) = self.snapshot_error {
            return SessionWriterProjection::SnapshotPending {
                checkpoint_sequence: self.checkpoint_sequence,
                pending_final_segments: self.pending_final_segments,
                code,
            };
        }
        if let Some(pending_since_ms) = self.pending_since_ms {
            return SessionWriterProjection::Deferred {
                checkpoint_sequence: self.checkpoint_sequence,
                pending_final_segments: self.pending_final_segments,
                flush_deadline_ms: pending_since_ms.saturating_add(SNAPSHOT_INTERVAL_MS),
            };
        }
        if let Some(code) = self.index_error {
            return SessionWriterProjection::IndexPending {
                checkpoint_sequence: self.checkpoint_sequence,
                code,
            };
        }
        SessionWriterProjection::Current {
            checkpoint_sequence: self.checkpoint_sequence,
            index_generation: self.index_generation,
        }
    }

    pub(crate) fn append(
        &mut self,
        append: JournalAppend,
        now_ms: u64,
    ) -> Result<SessionWriterReceipt, SessionWriterError> {
        self.append_with_fault(append, now_ms, None)
    }

    pub(crate) fn poll(
        &mut self,
        now_ms: u64,
    ) -> Result<SessionWriterProjection, SessionWriterError> {
        self.advance_tick(now_ms)?;
        if self.snapshot_due(now_ms) {
            self.flush_projection(None)?;
        } else if self.index_dirty && self.pending_final_segments == 0 {
            self.refresh_index(None)?;
        }
        Ok(self.projection())
    }

    pub(crate) fn flush(
        &mut self,
        now_ms: u64,
    ) -> Result<SessionWriterProjection, SessionWriterError> {
        self.advance_tick(now_ms)?;
        self.flush_projection(None)?;
        Ok(self.projection())
    }

    fn append_with_fault(
        &mut self,
        append: JournalAppend,
        now_ms: u64,
        fault: Option<WriterFault>,
    ) -> Result<SessionWriterReceipt, SessionWriterError> {
        self.advance_tick(now_ms)?;
        let finalized_segment = matches!(
            append.mutation,
            super::session_journal::JournalMutation::FinalizedTranscriptSegment(_)
        );
        let lifecycle_boundary = matches!(
            append.mutation,
            super::session_journal::JournalMutation::LifecycleChange(_)
        );
        let receipt = self
            .journal
            .append(&self.locator, append)
            .map_err(map_journal_error)?;
        inject_fault(fault, WriterFault::AfterJournalAcknowledged)?;

        if finalized_segment && !receipt.already_committed {
            self.pending_final_segments = self
                .pending_final_segments
                .saturating_add(1)
                .min(SNAPSHOT_SEGMENT_BATCH);
            self.pending_since_ms.get_or_insert(now_ms);
        }

        if lifecycle_boundary || self.snapshot_due(now_ms) {
            self.flush_projection(fault)?;
        }
        Ok(self.writer_receipt(receipt))
    }

    fn snapshot_due(&self, now_ms: u64) -> bool {
        self.pending_final_segments >= SNAPSHOT_SEGMENT_BATCH
            || self
                .pending_since_ms
                .is_some_and(|started| now_ms.saturating_sub(started) >= SNAPSHOT_INTERVAL_MS)
    }

    fn flush_projection(&mut self, fault: Option<WriterFault>) -> Result<(), SessionWriterError> {
        match self
            .transcripts
            .materialize(&self.locator, self.snapshot_fingerprint)
        {
            Ok(snapshot) => {
                self.snapshot_fingerprint = Some(snapshot.fingerprint);
                self.checkpoint_sequence = snapshot.checkpoint_sequence;
                self.pending_final_segments = 0;
                self.pending_since_ms = None;
                self.snapshot_error = None;
                self.index_dirty = true;
                inject_fault(fault, WriterFault::AfterSnapshotAcknowledged)?;
                self.refresh_index(fault)
            }
            Err(error) => {
                self.snapshot_error = Some(error.code);
                Ok(())
            }
        }
    }

    fn refresh_index(&mut self, fault: Option<WriterFault>) -> Result<(), SessionWriterError> {
        inject_fault(fault, WriterFault::BeforeIndexRefresh)?;
        match self.search.rebuild_index() {
            Ok(report) => {
                self.index_generation = Some(report.generation);
                self.index_dirty = false;
                self.index_error = None;
            }
            Err(error) => {
                self.index_dirty = true;
                self.index_error = Some(error.code);
            }
        }
        Ok(())
    }

    fn advance_tick(&mut self, now_ms: u64) -> Result<(), SessionWriterError> {
        if now_ms > MAX_SAFE_TICK_MS || now_ms < self.last_tick_ms {
            return Err(SessionWriterError::new("session_writer_clock_invalid"));
        }
        self.last_tick_ms = now_ms;
        Ok(())
    }

    fn writer_receipt(&self, receipt: JournalReceipt) -> SessionWriterReceipt {
        SessionWriterReceipt {
            sequence: receipt.sequence,
            checksum: receipt.checksum,
            already_committed: receipt.already_committed,
            projection: self.projection(),
        }
    }
}

fn validate_initial_tick(now_ms: u64) -> Result<(), SessionWriterError> {
    if now_ms > MAX_SAFE_TICK_MS {
        return Err(SessionWriterError::new("session_writer_clock_invalid"));
    }
    Ok(())
}

fn map_journal_error(error: super::session_journal::SessionJournalError) -> SessionWriterError {
    SessionWriterError::new(error.code)
}

fn map_transcript_error(
    error: super::transcript_store::TranscriptStoreError,
) -> SessionWriterError {
    SessionWriterError::new(error.code)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriterFault {
    AfterJournalAcknowledged,
    AfterSnapshotAcknowledged,
    BeforeIndexRefresh,
}

fn inject_fault(
    requested: Option<WriterFault>,
    checkpoint: WriterFault,
) -> Result<(), SessionWriterError> {
    if requested == Some(checkpoint) {
        return Err(SessionWriterError::new("session_writer_fault_injected"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use uuid::Uuid;

    use crate::{
        audio::AudioSource,
        domain::{Project, Session, SessionState},
        persistence::{
            FinalizedTranscriptSegment, JournalMutation, LifecycleChange, ProjectStore,
            SessionStore, TranscriptSearchRequest,
        },
    };

    use super::*;

    struct Fixture {
        workspace: tempfile::TempDir,
        app_data: tempfile::TempDir,
        project: Project,
        session: Session,
        locator: SessionLocator,
    }

    impl Fixture {
        fn new() -> Self {
            let value: serde_json::Value = serde_json::from_str(include_str!(
                "../../../fixtures/contracts/project-session-v1.json"
            ))
            .unwrap();
            let project = serde_json::from_value(value["project"].clone()).unwrap();
            let session = serde_json::from_value(value["session"].clone()).unwrap();
            let workspace = tempfile::tempdir().unwrap();
            let app_data = tempfile::tempdir().unwrap();
            ProjectStore::open(workspace.path())
                .unwrap()
                .create_project(&project)
                .unwrap();
            SessionStore::open(workspace.path())
                .unwrap()
                .create_session(&project, &session)
                .unwrap();
            let locator = SessionLocator::from_records(&project, &session).unwrap();
            Self {
                workspace,
                app_data,
                project,
                session,
                locator,
            }
        }

        fn writer(&self, now_ms: u64) -> PersistedSessionWriter {
            PersistedSessionWriter::open(
                self.workspace.path(),
                self.app_data.path(),
                self.locator.clone(),
                now_ms,
            )
            .unwrap()
        }

        fn transcript_path(&self) -> std::path::PathBuf {
            self.workspace
                .path()
                .join("projects")
                .join(&self.project.folder_name)
                .join("sessions")
                .join(&self.session.folder_name)
                .join("transcript.md")
        }
    }

    fn segment(event: u128, start_ms: u64, text: &str) -> JournalAppend {
        JournalAppend {
            event_id: Uuid::from_u128(event),
            recorded_at: "2026-08-12T10:00:00Z".to_owned(),
            mutation: JournalMutation::FinalizedTranscriptSegment(FinalizedTranscriptSegment {
                id: Uuid::from_u128(event + 100),
                source: AudioSource::Microphone,
                start_ms,
                end_ms: start_ms + 500,
                text: text.to_owned(),
                language: "en-GB".to_owned(),
            }),
        }
    }

    fn lifecycle(event: u128, previous: SessionState, current: SessionState) -> JournalAppend {
        JournalAppend {
            event_id: Uuid::from_u128(event),
            recorded_at: "2026-08-12T10:00:00Z".to_owned(),
            mutation: JournalMutation::LifecycleChange(LifecycleChange {
                previous,
                current,
                occurred_at: "2026-08-12T10:00:00Z".to_owned(),
            }),
        }
    }

    #[test]
    fn fifth_final_segment_materializes_and_refreshes_search() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(10);
        for index in 0..4 {
            let receipt = writer
                .append(
                    segment(index + 1, index as u64 * 1_000, "bounded phrase"),
                    10,
                )
                .unwrap();
            assert!(matches!(
                receipt.projection,
                SessionWriterProjection::Deferred { pending_final_segments, .. }
                    if pending_final_segments == index as u8 + 1
            ));
        }
        let receipt = writer
            .append(segment(5, 4_000, "searchable fifth phrase"), 10)
            .unwrap();
        assert!(matches!(
            receipt.projection,
            SessionWriterProjection::Current {
                checkpoint_sequence: 5,
                ..
            }
        ));
        let page = writer
            .search
            .search(&TranscriptSearchRequest {
                project_id: fixture.project.id,
                session_id: Some(fixture.session.id),
                query: "searchable".to_owned(),
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(page.items.len(), 1);
    }

    #[test]
    fn two_second_deadline_and_explicit_flush_are_deterministic() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(100);
        writer.append(segment(1, 0, "deadline"), 100).unwrap();
        assert!(matches!(
            writer.poll(2_099).unwrap(),
            SessionWriterProjection::Deferred { .. }
        ));
        assert!(matches!(
            writer.poll(2_100).unwrap(),
            SessionWriterProjection::Current {
                checkpoint_sequence: 1,
                ..
            }
        ));
        writer.append(segment(2, 1_000, "explicit"), 2_101).unwrap();
        assert!(matches!(
            writer.flush(2_101).unwrap(),
            SessionWriterProjection::Current {
                checkpoint_sequence: 2,
                ..
            }
        ));
    }

    #[test]
    fn lifecycle_boundary_flushes_below_the_segment_threshold() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(0);
        writer.append(segment(1, 0, "before lifecycle"), 1).unwrap();
        let receipt = writer
            .append(lifecycle(2, SessionState::Idle, SessionState::Preparing), 2)
            .unwrap();
        assert!(matches!(
            receipt.projection,
            SessionWriterProjection::Current {
                checkpoint_sequence: 2,
                ..
            }
        ));
    }

    #[test]
    fn identical_retry_does_not_advance_the_batch_or_duplicate_content() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(0);
        let append = segment(1, 0, "exactly once");
        writer.append(append.clone(), 1).unwrap();
        let retry = writer.append(append, 2).unwrap();
        assert!(retry.already_committed);
        assert_eq!(retry.sequence, 1);
        assert!(matches!(
            retry.projection,
            SessionWriterProjection::Deferred {
                pending_final_segments: 1,
                ..
            }
        ));
        writer.flush(2).unwrap();
        let snapshot = writer
            .transcripts
            .read_transcript(&fixture.locator)
            .unwrap();
        assert_eq!(snapshot.segment_count, 1);
    }

    #[test]
    fn reopen_repairs_a_crash_after_the_journal_acknowledgement() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(0);
        let error = writer
            .append_with_fault(
                segment(1, 0, "survives journal crash"),
                1,
                Some(WriterFault::AfterJournalAcknowledged),
            )
            .unwrap_err();
        assert_eq!(error.code, "session_writer_fault_injected");
        drop(writer);

        let reopened = fixture.writer(2);
        assert!(matches!(
            reopened.projection(),
            SessionWriterProjection::Current {
                checkpoint_sequence: 1,
                ..
            }
        ));
        let snapshot = reopened
            .transcripts
            .read_transcript(&fixture.locator)
            .unwrap();
        assert_eq!(snapshot.segment_count, 1);
    }

    #[test]
    fn reopen_refreshes_index_after_a_snapshot_boundary_crash() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(0);
        let error = writer
            .append_with_fault(
                lifecycle(1, SessionState::Idle, SessionState::Preparing),
                1,
                Some(WriterFault::AfterSnapshotAcknowledged),
            )
            .unwrap_err();
        assert_eq!(error.code, "session_writer_fault_injected");
        drop(writer);
        let reopened = fixture.writer(2);
        assert!(matches!(
            reopened.projection(),
            SessionWriterProjection::Current {
                checkpoint_sequence: 1,
                index_generation: Some(_),
            }
        ));
    }

    #[test]
    fn index_refresh_failure_is_retryable_without_rewriting_the_snapshot() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(0);
        let error = writer
            .append_with_fault(
                lifecycle(1, SessionState::Idle, SessionState::Preparing),
                1,
                Some(WriterFault::BeforeIndexRefresh),
            )
            .unwrap_err();
        assert_eq!(error.code, "session_writer_fault_injected");
        let before = fs::read(fixture.transcript_path()).unwrap();
        assert!(matches!(
            writer.poll(2).unwrap(),
            SessionWriterProjection::Current {
                checkpoint_sequence: 1,
                ..
            }
        ));
        assert_eq!(fs::read(fixture.transcript_path()).unwrap(), before);
    }

    #[test]
    fn external_markdown_conflict_keeps_journaling_and_never_overwrites() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(0);
        let path = fixture.transcript_path();
        fs::write(&path, b"external untrusted edit").unwrap();
        for index in 0..5 {
            writer
                .append(segment(index + 1, index as u64 * 1_000, "preserved"), 1)
                .unwrap();
        }
        assert!(matches!(
            writer.projection(),
            SessionWriterProjection::SnapshotPending {
                pending_final_segments: 5,
                code: "transcript_snapshot_conflict",
                ..
            }
        ));
        assert_eq!(fs::read(&path).unwrap(), b"external untrusted edit");
        assert_eq!(
            writer
                .journal
                .replay(&fixture.locator)
                .unwrap()
                .last_sequence,
            5
        );
    }

    #[test]
    fn clock_rollback_is_rejected_before_any_journal_write() {
        let fixture = Fixture::new();
        let mut writer = fixture.writer(10);
        let error = writer.append(segment(1, 0, "too early"), 9).unwrap_err();
        assert_eq!(error.code, "session_writer_clock_invalid");
        assert_eq!(
            writer
                .journal
                .replay(&fixture.locator)
                .unwrap()
                .last_sequence,
            0
        );
    }
}
