#![allow(dead_code)]

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{audio::AudioSource, domain::Session};

use super::{
    project_store::{
        atomic_publish_new, atomic_replace_existing, atomic_replace_with_backup,
        open_snapshot_without_write_share,
    },
    session_journal::{
        FinalizedTranscriptSegment, JournalReplay, SessionJournal, SessionJournalError,
    },
    session_store::{SessionLocator, SessionStore, SessionStoreError},
};

const TRANSCRIPT_DOCUMENT: &str = "transcript.md";
const TEMP_TRANSCRIPT_DOCUMENT: &str = ".transcript.md.tmp";
const BACKUP_TRANSCRIPT_DOCUMENT: &str = "transcript.md.bak";
const MAX_TRANSCRIPT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TRANSCRIPT_DISCOVERY_ISSUES: usize = 256;
const MAX_DISCOVERED_SEGMENTS: usize = 100_000;
const MAX_DISCOVERED_TEXT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TranscriptStoreError {
    pub(crate) code: &'static str,
}

impl TranscriptStoreError {
    pub(super) fn new(code: &'static str) -> Self {
        Self { code }
    }

    fn recoverable(self) -> bool {
        matches!(
            self.code,
            "transcript_snapshot_missing"
                | "transcript_snapshot_invalid"
                | "transcript_snapshot_too_large"
        )
    }
}

impl fmt::Display for TranscriptStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for TranscriptStoreError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TranscriptSnapshotFingerprint([u8; 32]);

impl TranscriptSnapshotFingerprint {
    pub(super) fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptSnapshot {
    pub(crate) fingerprint: TranscriptSnapshotFingerprint,
    pub(crate) checkpoint_sequence: u64,
    pub(crate) checkpoint_checksum: Option<String>,
    pub(crate) segment_count: usize,
    pub(crate) recovered_from_backup: bool,
    pub(crate) segments: Vec<FinalizedTranscriptSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredTranscript {
    pub(crate) session: Session,
    pub(crate) project_folder: String,
    pub(crate) snapshot: TranscriptSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptDiscoveryIssue {
    pub(crate) entry_name: Option<String>,
    pub(crate) code: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptDiscoveryReport {
    pub(crate) transcripts: Vec<DiscoveredTranscript>,
    pub(crate) issues: Vec<TranscriptDiscoveryIssue>,
    pub(crate) scanned_sessions: u32,
    pub(crate) issues_truncated: bool,
}

pub(crate) struct TranscriptStore {
    sessions: SessionStore,
    journal: SessionJournal,
}

impl TranscriptStore {
    pub(crate) fn open(workspace_path: &Path) -> Result<Self, TranscriptStoreError> {
        Ok(Self {
            sessions: SessionStore::open(workspace_path).map_err(map_session_error)?,
            journal: SessionJournal::open(workspace_path).map_err(map_journal_error)?,
        })
    }

    pub(crate) fn read_transcript(
        &self,
        locator: &SessionLocator,
    ) -> Result<TranscriptSnapshot, TranscriptStoreError> {
        let session = self
            .sessions
            .read_session(locator)
            .map_err(map_session_error)?
            .session;
        let directory = self
            .sessions
            .open_existing_session(locator)
            .map_err(map_session_error)?;
        let paths = TranscriptPaths::new(directory.path());
        match self.read_locked(&paths.document, locator, &session) {
            Ok(snapshot) => {
                let _ = fs::remove_file(&paths.temporary);
                Ok(snapshot.into_public(false))
            }
            Err(error) if error.error.recoverable() => {
                let missing = error.error.code == "transcript_snapshot_missing";
                let backup = self
                    .read_locked(&paths.backup, locator, &session)
                    .map_err(|_| {
                        TranscriptStoreError::new("transcript_snapshot_recovery_failed")
                    })?;
                directory.revalidate().map_err(map_session_error)?;
                write_synced_temporary(&paths.temporary, &backup.bytes)?;
                let publish = if missing {
                    atomic_publish_new(&paths.temporary, &paths.document)
                } else {
                    atomic_replace_existing(&paths.temporary, &paths.document)
                };
                publish.map_err(|_| {
                    TranscriptStoreError::new("transcript_snapshot_recovery_failed")
                })?;
                drop(error);
                let restored = self
                    .read_locked(&paths.document, locator, &session)
                    .map_err(|_| {
                        TranscriptStoreError::new("transcript_snapshot_recovery_failed")
                    })?;
                Ok(restored.into_public(true))
            }
            Err(error) => Err(error.error),
        }
    }

    pub(crate) fn discover_transcripts(
        &self,
    ) -> Result<TranscriptDiscoveryReport, TranscriptStoreError> {
        let sessions = self
            .sessions
            .discover_sessions()
            .map_err(map_session_error)?;
        let mut report = TranscriptDiscoveryReport {
            transcripts: Vec::new(),
            issues: Vec::new(),
            scanned_sessions: sessions.scanned_entries,
            issues_truncated: sessions.issues_truncated,
        };
        for issue in sessions.issues {
            push_discovery_issue(&mut report, issue.entry_name, issue.code);
        }
        let mut discovered_segments = 0_usize;
        let mut discovered_text_bytes = 0_usize;
        for candidate in sessions.sessions {
            let session = candidate.snapshot.session;
            let project_folder = candidate.project_folder;
            let locator = SessionLocator::from_discovered(&session, project_folder.clone());
            match self.read_transcript(&locator) {
                Ok(snapshot) => {
                    discovered_segments = discovered_segments
                        .checked_add(snapshot.segments.len())
                        .ok_or_else(|| {
                            TranscriptStoreError::new("transcript_discovery_limit_exceeded")
                        })?;
                    discovered_text_bytes = snapshot
                        .segments
                        .iter()
                        .try_fold(discovered_text_bytes, |total, segment| {
                            total.checked_add(segment.text.len())
                        })
                        .ok_or_else(|| {
                            TranscriptStoreError::new("transcript_discovery_limit_exceeded")
                        })?;
                    if discovered_segments > MAX_DISCOVERED_SEGMENTS
                        || discovered_text_bytes > MAX_DISCOVERED_TEXT_BYTES
                    {
                        return Err(TranscriptStoreError::new(
                            "transcript_discovery_limit_exceeded",
                        ));
                    }
                    report.transcripts.push(DiscoveredTranscript {
                        session,
                        project_folder,
                        snapshot,
                    });
                }
                Err(error) => push_discovery_issue(
                    &mut report,
                    safe_entry_name(&project_folder, &session.folder_name),
                    error.code,
                ),
            }
        }
        Ok(report)
    }

    pub(crate) fn materialize(
        &self,
        locator: &SessionLocator,
        expected_fingerprint: Option<TranscriptSnapshotFingerprint>,
    ) -> Result<TranscriptSnapshot, TranscriptStoreError> {
        self.materialize_with_fault(locator, expected_fingerprint, None)
    }

    fn materialize_with_fault(
        &self,
        locator: &SessionLocator,
        expected_fingerprint: Option<TranscriptSnapshotFingerprint>,
        fault: Option<MaterializeFault>,
    ) -> Result<TranscriptSnapshot, TranscriptStoreError> {
        let session = self
            .sessions
            .read_session(locator)
            .map_err(map_session_error)?
            .session;
        let replay = self.journal.replay(locator).map_err(map_journal_error)?;
        let bytes = render_transcript(&session, &replay)?;
        let directory = self
            .sessions
            .open_existing_session(locator)
            .map_err(map_session_error)?;
        let paths = TranscriptPaths::new(directory.path());
        let current = match self.read_locked(&paths.document, locator, &session) {
            Ok(snapshot) => Some(snapshot),
            Err(error) if error.error.code == "transcript_snapshot_missing" => None,
            Err(_error) if expected_fingerprint.is_some() => {
                return Err(TranscriptStoreError::new("transcript_snapshot_conflict"));
            }
            Err(error) => return Err(error.error),
        };
        match (current.as_ref(), expected_fingerprint) {
            (None, None) => {}
            (Some(snapshot), Some(expected)) if snapshot.fingerprint == expected => {}
            _ => return Err(TranscriptStoreError::new("transcript_snapshot_conflict")),
        }

        remove_if_file(&paths.temporary)?;
        write_synced_temporary(&paths.temporary, &bytes)?;
        let mut replaced = false;
        let result = (|| {
            inject_fault(fault, MaterializeFault::TemporarySynced)?;
            directory.revalidate().map_err(map_session_error)?;
            if current.is_some() {
                remove_if_file(&paths.backup)?;
                atomic_replace_with_backup(&paths.temporary, &paths.document, &paths.backup)
                    .map_err(|_| TranscriptStoreError::new("transcript_snapshot_publish_failed"))?;
                replaced = true;
            } else {
                atomic_publish_new(&paths.temporary, &paths.document)
                    .map_err(|_| TranscriptStoreError::new("transcript_snapshot_publish_failed"))?;
            }
            inject_fault(fault, MaterializeFault::Replaced)?;
            directory.revalidate().map_err(map_session_error)?;
            let published = self
                .read_locked(&paths.document, locator, &session)
                .map_err(|error| error.error)?;
            if published.bytes != bytes {
                return Err(TranscriptStoreError::new(
                    "transcript_snapshot_verify_failed",
                ));
            }
            Ok(published.into_public(false))
        })();

        drop(current);
        if result.is_err() {
            let _ = fs::remove_file(&paths.temporary);
            if replaced {
                let _ = self.restore_backup(&paths, locator, &session);
            }
        }
        result
    }

    fn read_locked(
        &self,
        path: &Path,
        locator: &SessionLocator,
        session: &Session,
    ) -> Result<LockedTranscript, LockedTranscriptError> {
        let mut file = open_snapshot_without_write_share(path)
            .map_err(|error| LockedTranscriptError::new(map_open_error(error.code), None))?;
        let length = file
            .metadata()
            .map_err(|_| {
                LockedTranscriptError::new(
                    TranscriptStoreError::new("transcript_snapshot_read_failed"),
                    None,
                )
            })?
            .len();
        if length > MAX_TRANSCRIPT_BYTES {
            return Err(LockedTranscriptError::new(
                TranscriptStoreError::new("transcript_snapshot_too_large"),
                Some(file),
            ));
        }
        let mut bytes = Vec::with_capacity(length as usize);
        if Read::by_ref(&mut file)
            .take(MAX_TRANSCRIPT_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_TRANSCRIPT_BYTES
        {
            return Err(LockedTranscriptError::new(
                TranscriptStoreError::new("transcript_snapshot_read_failed"),
                Some(file),
            ));
        }
        let (sequence, checksum) = match parse_checkpoint(&bytes) {
            Ok(checkpoint) => checkpoint,
            Err(error) => return Err(LockedTranscriptError::new(error, Some(file))),
        };
        let replay = match self.journal.replay_through(locator, sequence) {
            Ok(replay) => replay,
            Err(_) => {
                return Err(LockedTranscriptError::new(
                    TranscriptStoreError::new("transcript_checkpoint_invalid"),
                    Some(file),
                ));
            }
        };
        let rendered = match render_transcript(session, &replay) {
            Ok(rendered) => rendered,
            Err(error) => return Err(LockedTranscriptError::new(error, Some(file))),
        };
        if replay.last_checksum != checksum || rendered != bytes {
            return Err(LockedTranscriptError::new(
                TranscriptStoreError::new("transcript_snapshot_invalid"),
                Some(file),
            ));
        }
        Ok(LockedTranscript {
            _file: file,
            fingerprint: TranscriptSnapshotFingerprint(Sha256::digest(&bytes).into()),
            checkpoint_sequence: sequence,
            checkpoint_checksum: checksum,
            segment_count: replay.finalized_segments.len(),
            segments: replay.finalized_segments,
            bytes,
        })
    }

    fn restore_backup(
        &self,
        paths: &TranscriptPaths,
        locator: &SessionLocator,
        session: &Session,
    ) -> Result<(), TranscriptStoreError> {
        let backup = self
            .read_locked(&paths.backup, locator, session)
            .map_err(|error| error.error)?;
        write_synced_temporary(&paths.temporary, &backup.bytes)?;
        atomic_replace_existing(&paths.temporary, &paths.document)
            .map_err(|_| TranscriptStoreError::new("transcript_snapshot_recovery_failed"))
    }
}

struct TranscriptPaths {
    document: PathBuf,
    temporary: PathBuf,
    backup: PathBuf,
}

impl TranscriptPaths {
    fn new(directory: &Path) -> Self {
        Self {
            document: directory.join(TRANSCRIPT_DOCUMENT),
            temporary: directory.join(TEMP_TRANSCRIPT_DOCUMENT),
            backup: directory.join(BACKUP_TRANSCRIPT_DOCUMENT),
        }
    }
}

struct LockedTranscript {
    _file: File,
    bytes: Vec<u8>,
    fingerprint: TranscriptSnapshotFingerprint,
    checkpoint_sequence: u64,
    checkpoint_checksum: Option<String>,
    segment_count: usize,
    segments: Vec<FinalizedTranscriptSegment>,
}

impl LockedTranscript {
    fn into_public(self, recovered_from_backup: bool) -> TranscriptSnapshot {
        TranscriptSnapshot {
            fingerprint: self.fingerprint,
            checkpoint_sequence: self.checkpoint_sequence,
            checkpoint_checksum: self.checkpoint_checksum,
            segment_count: self.segment_count,
            recovered_from_backup,
            segments: self.segments,
        }
    }
}

struct LockedTranscriptError {
    error: TranscriptStoreError,
    _file: Option<File>,
}

impl LockedTranscriptError {
    fn new(error: TranscriptStoreError, file: Option<File>) -> Self {
        Self { error, _file: file }
    }
}

impl From<TranscriptStoreError> for LockedTranscriptError {
    fn from(error: TranscriptStoreError) -> Self {
        Self::new(error, None)
    }
}

fn render_transcript(
    session: &Session,
    replay: &JournalReplay,
) -> Result<Vec<u8>, TranscriptStoreError> {
    let updated_at = replay
        .last_recorded_at
        .as_deref()
        .unwrap_or(&session.created_at);
    let mut document = String::from("---\nschema_version: 1\ndocument_type: \"transcript\"\n");
    document.push_str("project_id: ");
    document.push_str(&json_scalar(&session.project_id)?);
    document.push_str("\nsession_id: ");
    document.push_str(&json_scalar(&session.id)?);
    document.push_str("\ncreated_at: ");
    document.push_str(&json_scalar(&session.created_at)?);
    document.push_str("\nupdated_at: ");
    document.push_str(&json_scalar(&updated_at)?);
    document.push_str("\njournal_sequence: ");
    document.push_str(&replay.last_sequence.to_string());
    document.push_str("\njournal_checksum: ");
    document.push_str(&json_scalar(&replay.last_checksum)?);
    document.push_str("\n---\n");

    for segment in &replay.finalized_segments {
        document.push('\n');
        document.push_str("## ");
        document.push_str(&format_timestamp(segment.start_ms));
        document.push_str(" — ");
        document.push_str(match segment.source {
            AudioSource::Microphone => "Microphone",
            AudioSource::SystemOutput => "System output",
        });
        document.push_str("\n\n");
        document.push_str(&segment.text);
        if !segment.text.ends_with('\n') {
            document.push('\n');
        }
        document.push_str("\n<!--\nsegment_id: ");
        document.push_str(&segment.id.to_string());
        document.push_str("\nsource: ");
        document.push_str(match segment.source {
            AudioSource::Microphone => "microphone",
            AudioSource::SystemOutput => "system_output",
        });
        document.push_str("\nstart_ms: ");
        document.push_str(&segment.start_ms.to_string());
        document.push_str("\nend_ms: ");
        document.push_str(&segment.end_ms.to_string());
        document.push_str("\nstatus: final\nlanguage: ");
        document.push_str(&json_scalar(&segment.language)?);
        document.push_str("\n-->\n");
    }
    if document.len() as u64 > MAX_TRANSCRIPT_BYTES {
        return Err(TranscriptStoreError::new("transcript_snapshot_too_large"));
    }
    Ok(document.into_bytes())
}

fn json_scalar<T: serde::Serialize>(value: &T) -> Result<String, TranscriptStoreError> {
    serde_json::to_string(value)
        .map_err(|_| TranscriptStoreError::new("transcript_snapshot_render_failed"))
}

fn parse_checkpoint(bytes: &[u8]) -> Result<(u64, Option<String>), TranscriptStoreError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| TranscriptStoreError::new("transcript_snapshot_invalid"))?;
    if text.contains('\r') {
        return Err(TranscriptStoreError::new("transcript_snapshot_invalid"));
    }
    let lines = text.lines().take(10).collect::<Vec<_>>();
    if lines.len() != 10
        || lines[0] != "---"
        || lines[1] != "schema_version: 1"
        || lines[2] != "document_type: \"transcript\""
        || !lines[3].starts_with("project_id: ")
        || !lines[4].starts_with("session_id: ")
        || !lines[5].starts_with("created_at: ")
        || !lines[6].starts_with("updated_at: ")
        || !lines[7].starts_with("journal_sequence: ")
        || !lines[8].starts_with("journal_checksum: ")
        || lines[9] != "---"
    {
        return Err(TranscriptStoreError::new("transcript_snapshot_invalid"));
    }
    let sequence = lines[7]["journal_sequence: ".len()..]
        .parse::<u64>()
        .map_err(|_| TranscriptStoreError::new("transcript_checkpoint_invalid"))?;
    let checksum = serde_json::from_str::<Option<String>>(&lines[8]["journal_checksum: ".len()..])
        .map_err(|_| TranscriptStoreError::new("transcript_checkpoint_invalid"))?;
    if checksum.as_ref().is_some_and(|value| {
        value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) || (sequence == 0) != checksum.is_none()
    {
        return Err(TranscriptStoreError::new("transcript_checkpoint_invalid"));
    }
    Ok((sequence, checksum))
}

fn format_timestamp(milliseconds: u64) -> String {
    let hours = milliseconds / 3_600_000;
    let minutes = (milliseconds / 60_000) % 60;
    let seconds = (milliseconds / 1_000) % 60;
    let millis = milliseconds % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn safe_entry_name(project_folder: &str, session_folder: &str) -> Option<String> {
    let combined = format!("{project_folder}/{session_folder}");
    (!combined.is_empty() && combined.len() <= 257 && !combined.chars().any(char::is_control))
        .then_some(combined)
}

fn push_discovery_issue(
    report: &mut TranscriptDiscoveryReport,
    entry_name: Option<String>,
    code: &'static str,
) {
    if report.issues.len() < MAX_TRANSCRIPT_DISCOVERY_ISSUES {
        report
            .issues
            .push(TranscriptDiscoveryIssue { entry_name, code });
    } else {
        report.issues_truncated = true;
    }
}

fn write_synced_temporary(path: &Path, bytes: &[u8]) -> Result<(), TranscriptStoreError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| TranscriptStoreError::new("transcript_snapshot_write_failed"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| TranscriptStoreError::new("transcript_snapshot_write_failed"))
}

fn remove_if_file(path: &Path) -> Result<(), TranscriptStoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(TranscriptStoreError::new(
            "transcript_snapshot_write_failed",
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaterializeFault {
    TemporarySynced,
    Replaced,
}

fn inject_fault(
    configured: Option<MaterializeFault>,
    point: MaterializeFault,
) -> Result<(), TranscriptStoreError> {
    if configured == Some(point) {
        Err(TranscriptStoreError::new(
            "transcript_snapshot_fault_injected",
        ))
    } else {
        Ok(())
    }
}

fn map_open_error(code: &'static str) -> TranscriptStoreError {
    match code {
        "project_snapshot_missing" => TranscriptStoreError::new("transcript_snapshot_missing"),
        "project_path_unsafe" => TranscriptStoreError::new("transcript_path_unsafe"),
        _ => TranscriptStoreError::new("transcript_snapshot_read_failed"),
    }
}

fn map_session_error(error: SessionStoreError) -> TranscriptStoreError {
    match error.code {
        "session_path_unsafe" | "session_path_identity_changed" => {
            TranscriptStoreError::new("transcript_path_unsafe")
        }
        "session_snapshot_missing" => TranscriptStoreError::new("transcript_session_missing"),
        _ => TranscriptStoreError::new("transcript_session_unavailable"),
    }
}

fn map_journal_error(_error: SessionJournalError) -> TranscriptStoreError {
    TranscriptStoreError::new("transcript_journal_unavailable")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use uuid::Uuid;

    use crate::{
        domain::{Project, Session},
        persistence::{
            FinalizedTranscriptSegment, JournalAppend, JournalMutation, ProjectStore,
            SessionJournal, SessionStore,
        },
    };

    use super::*;

    fn workspace() -> (
        tempfile::TempDir,
        Project,
        Session,
        SessionLocator,
        TranscriptStore,
    ) {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        let project = serde_json::from_value(fixture["project"].clone()).unwrap();
        let session = serde_json::from_value(fixture["session"].clone()).unwrap();
        let root = tempfile::tempdir().unwrap();
        ProjectStore::open(root.path())
            .unwrap()
            .create_project(&project)
            .unwrap();
        SessionStore::open(root.path())
            .unwrap()
            .create_session(&project, &session)
            .unwrap();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let store = TranscriptStore::open(root.path()).unwrap();
        (root, project, session, locator, store)
    }

    fn paths(root: &Path, project: &Project, session: &Session) -> TranscriptPaths {
        TranscriptPaths::new(
            &root
                .join("projects")
                .join(&project.folder_name)
                .join("sessions")
                .join(&session.folder_name),
        )
    }

    fn append_segment(root: &Path, locator: &SessionLocator, text: &str, start_ms: u64) {
        append_segment_with_ids(
            root,
            locator,
            text,
            start_ms,
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
    }

    fn append_segment_with_ids(
        root: &Path,
        locator: &SessionLocator,
        text: &str,
        start_ms: u64,
        event_id: Uuid,
        segment_id: Uuid,
    ) {
        SessionJournal::open(root)
            .unwrap()
            .append(
                locator,
                JournalAppend {
                    event_id,
                    recorded_at: "2026-08-11T10:00:01Z".to_owned(),
                    mutation: JournalMutation::FinalizedTranscriptSegment(
                        FinalizedTranscriptSegment {
                            id: segment_id,
                            source: AudioSource::Microphone,
                            start_ms,
                            end_ms: start_ms + 1_000,
                            text: text.to_owned(),
                            language: "en-GB".to_owned(),
                        },
                    ),
                },
            )
            .unwrap();
    }

    #[test]
    fn materialization_records_and_verifies_the_exact_journal_checkpoint() {
        let (root, project, session, locator, store) = workspace();
        SessionJournal::open(root.path())
            .unwrap()
            .append(
                &locator,
                JournalAppend {
                    event_id: Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap(),
                    recorded_at: "2026-08-11T10:00:01Z".to_owned(),
                    mutation: JournalMutation::FinalizedTranscriptSegment(
                        FinalizedTranscriptSegment {
                            id: Uuid::parse_str("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee").unwrap(),
                            source: AudioSource::Microphone,
                            start_ms: 120,
                            end_ms: 1_420,
                            text: "A bounded finalized segment.".to_owned(),
                            language: "en".to_owned(),
                        },
                    ),
                },
            )
            .unwrap();
        let transcript_paths = paths(root.path(), &project, &session);
        fs::write(&transcript_paths.temporary, b"torn temporary").unwrap();
        let snapshot = store.materialize(&locator, None).unwrap();
        let document = fs::read(transcript_paths.document).unwrap();
        assert_eq!(
            document,
            include_bytes!("../../../fixtures/persistence/transcript-v1.md")
        );
        assert_eq!(snapshot.checkpoint_sequence, 1);
        assert_eq!(snapshot.segment_count, 1);
        assert!(snapshot.checkpoint_checksum.is_some());
        assert_eq!(parse_checkpoint(&document).unwrap().0, 1);
        assert_eq!(store.read_transcript(&locator).unwrap(), snapshot);
    }

    #[test]
    fn replay_rebuilds_identical_canonical_markdown() {
        let (root, project, session, locator, store) = workspace();
        append_segment(
            root.path(),
            &locator,
            "First.\n\nSecond paragraph.",
            194_000,
        );
        let first = store.materialize(&locator, None).unwrap();
        let transcript_paths = paths(root.path(), &project, &session);
        let expected = fs::read(&transcript_paths.document).unwrap();
        fs::remove_file(&transcript_paths.document).unwrap();
        let rebuilt = store.materialize(&locator, None).unwrap();
        assert_eq!(fs::read(&transcript_paths.document).unwrap(), expected);
        assert_eq!(rebuilt.fingerprint, first.fingerprint);
    }

    #[test]
    fn exact_byte_conflicts_preserve_external_edits() {
        let (root, project, session, locator, store) = workspace();
        append_segment(root.path(), &locator, "Original.", 10);
        let snapshot = store.materialize(&locator, None).unwrap();
        let transcript_paths = paths(root.path(), &project, &session);
        let mut external = fs::read(&transcript_paths.document).unwrap();
        external.extend_from_slice(b"\nexternal edit\n");
        fs::write(&transcript_paths.document, &external).unwrap();
        assert_eq!(
            store
                .materialize(&locator, Some(snapshot.fingerprint))
                .unwrap_err()
                .code,
            "transcript_snapshot_conflict"
        );
        assert_eq!(fs::read(&transcript_paths.document).unwrap(), external);
    }

    #[test]
    fn replacement_retains_backup_and_read_recovers_it() {
        let (root, project, session, locator, store) = workspace();
        append_segment(root.path(), &locator, "First.", 10);
        let first = store.materialize(&locator, None).unwrap();
        let transcript_paths = paths(root.path(), &project, &session);
        let first_bytes = fs::read(&transcript_paths.document).unwrap();
        append_segment(root.path(), &locator, "Second.", 20);
        let second = store
            .materialize(&locator, Some(first.fingerprint))
            .unwrap();
        assert_eq!(fs::read(&transcript_paths.backup).unwrap(), first_bytes);
        fs::write(&transcript_paths.document, b"malformed").unwrap();
        let recovered = store.read_transcript(&locator).unwrap();
        assert!(recovered.recovered_from_backup);
        assert_eq!(recovered.checkpoint_sequence, 1);
        assert_ne!(recovered.fingerprint, second.fingerprint);
        assert_eq!(fs::read(&transcript_paths.document).unwrap(), first_bytes);
        assert_eq!(fs::read(&transcript_paths.backup).unwrap(), first_bytes);
    }

    #[test]
    fn injected_failures_never_lose_the_last_acknowledged_snapshot() {
        for fault in [
            MaterializeFault::TemporarySynced,
            MaterializeFault::Replaced,
        ] {
            let (root, project, session, locator, store) = workspace();
            append_segment(root.path(), &locator, "First.", 10);
            let first = store.materialize(&locator, None).unwrap();
            let transcript_paths = paths(root.path(), &project, &session);
            let acknowledged = fs::read(&transcript_paths.document).unwrap();
            append_segment(root.path(), &locator, "Second.", 20);
            assert!(
                store
                    .materialize_with_fault(&locator, Some(first.fingerprint), Some(fault))
                    .is_err()
            );
            assert_eq!(fs::read(&transcript_paths.document).unwrap(), acknowledged);
            assert_eq!(
                store.read_transcript(&locator).unwrap().checkpoint_sequence,
                1
            );
        }
    }
}
