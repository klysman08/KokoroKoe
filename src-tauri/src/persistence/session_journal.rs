#![allow(dead_code)]

use std::{
    collections::HashMap,
    fmt,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
};

use crate::{
    audio::AudioSource,
    domain::{SessionId, SessionState},
};

use super::session_store::{SessionLocator, SessionStore, SessionStoreError};

const JOURNAL_FILE: &str = "recovery.journal";
const JOURNAL_SCHEMA_VERSION: u8 = 1;
const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_RECORDS: usize = 100_000;
const MAX_SEGMENT_TEXT_BYTES: usize = 32 * 1024;
const MAX_LANGUAGE_BYTES: usize = 64;
const JSON_SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionJournalError {
    pub(crate) code: &'static str,
}

impl SessionJournalError {
    fn new(code: &'static str) -> Self {
        Self { code }
    }
}

impl fmt::Display for SessionJournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for SessionJournalError {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FinalizedTranscriptSegment {
    pub(crate) id: Uuid,
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) text: String,
    pub(crate) language: String,
}

impl FinalizedTranscriptSegment {
    fn validate(&self) -> Result<(), SessionJournalError> {
        if self.id.is_nil()
            || self.end_ms <= self.start_ms
            || self.end_ms > JSON_SAFE_INTEGER_MAX
            || self.text.is_empty()
            || self.text.len() > MAX_SEGMENT_TEXT_BYTES
            || self.text.chars().any(|value| {
                value == '\r' || (value.is_control() && value != '\n' && value != '\t')
            })
            || self.language.is_empty()
            || self.language.len() > MAX_LANGUAGE_BYTES
            || self.language.chars().any(char::is_control)
        {
            return Err(SessionJournalError::new("session_journal_mutation_invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LifecycleChange {
    pub(crate) previous: SessionState,
    pub(crate) current: SessionState,
    pub(crate) occurred_at: String,
}

impl LifecycleChange {
    fn validate(&self) -> Result<(), SessionJournalError> {
        if OffsetDateTime::parse(&self.occurred_at, &Rfc3339).is_err()
            || !valid_transition(self.previous, self.current)
        {
            return Err(SessionJournalError::new("session_journal_mutation_invalid"));
        }
        Ok(())
    }
}

/// A user's rewrite of one finalized segment.
///
/// This is an event, not an edit. The journal is append-only, so the original
/// `FinalizedTranscriptSegment` record it refers to is never touched and the
/// transcription as it was produced always stays recoverable.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SegmentCorrection {
    pub(crate) segment_id: Uuid,
    pub(crate) text: String,
}

impl SegmentCorrection {
    fn validate(&self) -> Result<(), SessionJournalError> {
        if self.segment_id.is_nil() || !valid_segment_text(&self.text) {
            return Err(SessionJournalError::new("session_journal_mutation_invalid"));
        }
        Ok(())
    }
}

/// Whether the user marked one finalized segment as important.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SegmentImportance {
    pub(crate) segment_id: Uuid,
    pub(crate) important: bool,
}

impl SegmentImportance {
    fn validate(&self) -> Result<(), SessionJournalError> {
        if self.segment_id.is_nil() {
            return Err(SessionJournalError::new("session_journal_mutation_invalid"));
        }
        Ok(())
    }
}

/// The same bounds a transcribed segment must satisfy. A correction is read
/// back and rendered exactly where the original was, so it cannot be allowed to
/// carry anything the original could not.
fn valid_segment_text(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_SEGMENT_TEXT_BYTES
        && !text
            .chars()
            .any(|value| value == '\r' || (value.is_control() && value != '\n' && value != '\t'))
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub(crate) enum JournalMutation {
    FinalizedTranscriptSegment(FinalizedTranscriptSegment),
    LifecycleChange(LifecycleChange),
    SegmentCorrection(SegmentCorrection),
    SegmentImportance(SegmentImportance),
}

impl JournalMutation {
    fn validate(&self) -> Result<(), SessionJournalError> {
        match self {
            Self::FinalizedTranscriptSegment(segment) => segment.validate(),
            Self::LifecycleChange(change) => change.validate(),
            Self::SegmentCorrection(correction) => correction.validate(),
            Self::SegmentImportance(importance) => importance.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JournalAppend {
    pub(crate) event_id: Uuid,
    pub(crate) recorded_at: String,
    pub(crate) mutation: JournalMutation,
}

impl JournalAppend {
    fn validate(&self) -> Result<(), SessionJournalError> {
        if self.event_id.is_nil() || OffsetDateTime::parse(&self.recorded_at, &Rfc3339).is_err() {
            return Err(SessionJournalError::new("session_journal_append_invalid"));
        }
        self.mutation.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JournalReplay {
    pub(crate) last_sequence: u64,
    pub(crate) last_checksum: Option<String>,
    pub(crate) last_recorded_at: Option<String>,
    pub(crate) applied_events: usize,
    pub(crate) ignored_duplicate_events: usize,
    pub(crate) discarded_torn_tail: bool,
    pub(crate) state: SessionState,
    pub(crate) finalized_segments: Vec<ReplayedSegment>,
}

/// One finalized segment with whatever the user later said about it.
///
/// The journaled segment is kept verbatim: `text` on it is always the
/// transcription as produced, and a correction sits beside it rather than over
/// it, so both readings survive every replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplayedSegment {
    pub(crate) segment: FinalizedTranscriptSegment,
    pub(crate) corrected_text: Option<String>,
    pub(crate) important: bool,
}

impl ReplayedSegment {
    /// What the transcript reads as now.
    pub(crate) fn effective_text(&self) -> &str {
        self.corrected_text.as_deref().unwrap_or(&self.segment.text)
    }

    /// The transcription as it was produced. Never overwritten.
    pub(crate) fn original_text(&self) -> &str {
        &self.segment.text
    }

    pub(crate) const fn corrected(&self) -> bool {
        self.corrected_text.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JournalReceipt {
    pub(crate) sequence: u64,
    pub(crate) checksum: String,
    pub(crate) already_committed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnsignedRecord {
    schema_version: u8,
    session_id: SessionId,
    sequence: u64,
    event_id: Uuid,
    recorded_at: String,
    mutation: JournalMutation,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JournalRecord {
    schema_version: u8,
    session_id: SessionId,
    sequence: u64,
    event_id: Uuid,
    recorded_at: String,
    mutation: JournalMutation,
    checksum: String,
}

impl JournalRecord {
    fn unsigned(&self) -> UnsignedRecord {
        UnsignedRecord {
            schema_version: self.schema_version,
            session_id: self.session_id,
            sequence: self.sequence,
            event_id: self.event_id,
            recorded_at: self.recorded_at.clone(),
            mutation: self.mutation.clone(),
        }
    }

    fn validate(&self, session_id: SessionId, sequence: u64) -> Result<(), SessionJournalError> {
        if self.schema_version != JOURNAL_SCHEMA_VERSION
            || self.session_id != session_id
            || self.sequence != sequence
            || self.sequence == 0
            || self.sequence > JSON_SAFE_INTEGER_MAX
            || self.event_id.is_nil()
            || OffsetDateTime::parse(&self.recorded_at, &Rfc3339).is_err()
            || self.checksum.len() != 64
            || !self
                .checksum
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
        {
            return Err(SessionJournalError::new("session_journal_record_invalid"));
        }
        self.mutation.validate()?;
        if checksum(&self.unsigned())? != self.checksum {
            return Err(SessionJournalError::new(
                "session_journal_checksum_mismatch",
            ));
        }
        Ok(())
    }
}

pub(crate) struct SessionJournal {
    sessions: SessionStore,
}

impl SessionJournal {
    pub(crate) fn open(workspace_path: &Path) -> Result<Self, SessionJournalError> {
        Ok(Self {
            sessions: SessionStore::open(workspace_path).map_err(map_store_error)?,
        })
    }

    pub(crate) fn append(
        &self,
        locator: &SessionLocator,
        append: JournalAppend,
    ) -> Result<JournalReceipt, SessionJournalError> {
        append.validate()?;
        let session_id = locator.session_id();
        let directory = self
            .sessions
            .open_existing_session(locator)
            .map_err(map_store_error)?;
        directory.revalidate().map_err(map_store_error)?;
        let path = directory.path().join(JOURNAL_FILE);
        let mut file = open_journal(&path)?;
        file.lock_exclusive()
            .map_err(|_| SessionJournalError::new("session_journal_lock_failed"))?;
        let result = (|| {
            let bytes = read_bounded(&mut file)?;
            let parsed = parse_records(&bytes, session_id)?;
            replay_records(&parsed.records, parsed.discarded_torn_tail)?;
            if let Some(record) = parsed
                .records
                .iter()
                .find(|record| record.event_id == append.event_id)
            {
                if record.recorded_at != append.recorded_at || record.mutation != append.mutation {
                    return Err(SessionJournalError::new("session_journal_event_conflict"));
                }
                if parsed.discarded_torn_tail {
                    file.set_len(parsed.valid_bytes as u64)
                        .and_then(|()| file.sync_all())
                        .map_err(|_| SessionJournalError::new("session_journal_write_failed"))?;
                }
                return Ok(JournalReceipt {
                    sequence: record.sequence,
                    checksum: record.checksum.clone(),
                    already_committed: true,
                });
            }
            let sequence = u64::try_from(parsed.records.len())
                .ok()
                .and_then(|value| value.checked_add(1))
                .filter(|value| *value <= JSON_SAFE_INTEGER_MAX)
                .ok_or_else(|| SessionJournalError::new("session_journal_limit_exceeded"))?;
            let unsigned = UnsignedRecord {
                schema_version: JOURNAL_SCHEMA_VERSION,
                session_id,
                sequence,
                event_id: append.event_id,
                recorded_at: append.recorded_at,
                mutation: append.mutation,
            };
            let record = JournalRecord {
                schema_version: unsigned.schema_version,
                session_id: unsigned.session_id,
                sequence: unsigned.sequence,
                event_id: unsigned.event_id,
                recorded_at: unsigned.recorded_at.clone(),
                mutation: unsigned.mutation.clone(),
                checksum: checksum(&unsigned)?,
            };
            let mut candidate_records = parsed.records.clone();
            candidate_records.push(record.clone());
            replay_records(&candidate_records, false)?;
            let mut encoded = serde_json::to_vec(&record)
                .map_err(|_| SessionJournalError::new("session_journal_record_invalid"))?;
            encoded.push(b'\n');
            if encoded.len() > MAX_RECORD_BYTES
                || (parsed.valid_bytes as u64)
                    .checked_add(encoded.len() as u64)
                    .is_none_or(|length| length > MAX_JOURNAL_BYTES)
            {
                return Err(SessionJournalError::new("session_journal_limit_exceeded"));
            }
            directory.revalidate().map_err(map_store_error)?;
            if parsed.discarded_torn_tail {
                file.set_len(parsed.valid_bytes as u64)
                    .map_err(|_| SessionJournalError::new("session_journal_write_failed"))?;
            }
            file.seek(SeekFrom::End(0))
                .and_then(|_| file.write_all(&encoded))
                .and_then(|_| file.sync_all())
                .map_err(|_| SessionJournalError::new("session_journal_write_failed"))?;
            directory.revalidate().map_err(map_store_error)?;
            Ok(JournalReceipt {
                sequence,
                checksum: record.checksum,
                already_committed: false,
            })
        })();
        let _ = FileExt::unlock(&file);
        result
    }

    pub(crate) fn replay(
        &self,
        locator: &SessionLocator,
    ) -> Result<JournalReplay, SessionJournalError> {
        let session_id = locator.session_id();
        let directory = self
            .sessions
            .open_existing_session(locator)
            .map_err(map_store_error)?;
        directory.revalidate().map_err(map_store_error)?;
        let path = directory.path().join(JOURNAL_FILE);
        if !path.exists() {
            return Ok(empty_replay());
        }
        let mut file = open_existing_journal(&path)?;
        FileExt::lock_shared(&file)
            .map_err(|_| SessionJournalError::new("session_journal_lock_failed"))?;
        let result = (|| {
            let bytes = read_bounded(&mut file)?;
            let parsed = parse_records(&bytes, session_id)?;
            directory.revalidate().map_err(map_store_error)?;
            replay_records(&parsed.records, parsed.discarded_torn_tail)
        })();
        let _ = FileExt::unlock(&file);
        result
    }

    pub(crate) fn replay_through(
        &self,
        locator: &SessionLocator,
        sequence: u64,
    ) -> Result<JournalReplay, SessionJournalError> {
        let session_id = locator.session_id();
        let directory = self
            .sessions
            .open_existing_session(locator)
            .map_err(map_store_error)?;
        directory.revalidate().map_err(map_store_error)?;
        let path = directory.path().join(JOURNAL_FILE);
        if !path.exists() {
            return if sequence == 0 {
                Ok(empty_replay())
            } else {
                Err(SessionJournalError::new(
                    "session_journal_checkpoint_invalid",
                ))
            };
        }
        let mut file = open_existing_journal(&path)?;
        FileExt::lock_shared(&file)
            .map_err(|_| SessionJournalError::new("session_journal_lock_failed"))?;
        let result = (|| {
            let bytes = read_bounded(&mut file)?;
            let parsed = parse_records(&bytes, session_id)?;
            let length = usize::try_from(sequence)
                .ok()
                .filter(|length| *length <= parsed.records.len())
                .ok_or_else(|| SessionJournalError::new("session_journal_checkpoint_invalid"))?;
            directory.revalidate().map_err(map_store_error)?;
            replay_records(&parsed.records[..length], false)
        })();
        let _ = FileExt::unlock(&file);
        result
    }
}

struct ParsedRecords {
    records: Vec<JournalRecord>,
    valid_bytes: usize,
    discarded_torn_tail: bool,
}

fn parse_records(
    bytes: &[u8],
    session_id: SessionId,
) -> Result<ParsedRecords, SessionJournalError> {
    let has_torn_tail = !bytes.is_empty() && !bytes.ends_with(b"\n");
    let valid_bytes = if has_torn_tail {
        bytes
            .iter()
            .rposition(|value| *value == b'\n')
            .map_or(0, |index| index + 1)
    } else {
        bytes.len()
    };
    let mut records = Vec::new();
    for line in bytes[..valid_bytes].split(|value| *value == b'\n') {
        if line.is_empty() {
            continue;
        }
        if line.len() + 1 > MAX_RECORD_BYTES || records.len() >= MAX_RECORDS {
            return Err(SessionJournalError::new("session_journal_limit_exceeded"));
        }
        let record: JournalRecord = serde_json::from_slice(line)
            .map_err(|_| SessionJournalError::new("session_journal_record_invalid"))?;
        let sequence = u64::try_from(records.len()).unwrap_or(u64::MAX) + 1;
        record.validate(session_id, sequence)?;
        records.push(record);
    }
    Ok(ParsedRecords {
        records,
        valid_bytes,
        discarded_torn_tail: has_torn_tail,
    })
}

fn annotated(
    positions: &HashMap<Uuid, usize>,
    segment_id: Uuid,
) -> Result<usize, SessionJournalError> {
    positions
        .get(&segment_id)
        .copied()
        .ok_or_else(|| SessionJournalError::new("session_journal_annotation_orphaned"))
}

fn replay_records(
    records: &[JournalRecord],
    discarded_torn_tail: bool,
) -> Result<JournalReplay, SessionJournalError> {
    let mut state = SessionState::Idle;
    let mut segments: Vec<ReplayedSegment> = Vec::new();
    let mut positions: HashMap<Uuid, usize> = HashMap::new();
    let mut events: HashMap<Uuid, (&str, &JournalMutation)> = HashMap::new();
    let mut ignored_duplicate_events = 0;
    for record in records {
        if let Some((recorded_at, mutation)) = events.get(&record.event_id) {
            if *recorded_at != record.recorded_at || *mutation != &record.mutation {
                return Err(SessionJournalError::new("session_journal_event_conflict"));
            }
            ignored_duplicate_events += 1;
            continue;
        }
        events.insert(record.event_id, (&record.recorded_at, &record.mutation));
        match &record.mutation {
            JournalMutation::FinalizedTranscriptSegment(segment) => {
                if positions.insert(segment.id, segments.len()).is_some() {
                    return Err(SessionJournalError::new("session_journal_segment_conflict"));
                }
                segments.push(ReplayedSegment {
                    segment: segment.clone(),
                    corrected_text: None,
                    important: false,
                });
            }
            JournalMutation::LifecycleChange(change) => {
                if change.previous != state {
                    return Err(SessionJournalError::new(
                        "session_journal_lifecycle_conflict",
                    ));
                }
                state = change.current;
            }
            // An annotation is always written after the segment it names, so a
            // reference to an unknown segment is corruption rather than an
            // ordering the replay should tolerate. A truncated prefix can only
            // lose annotations, never orphan one.
            JournalMutation::SegmentCorrection(correction) => {
                let position = annotated(&positions, correction.segment_id)?;
                // A later correction supersedes an earlier one; the journaled
                // original underneath is still untouched.
                segments[position].corrected_text = Some(correction.text.clone());
            }
            JournalMutation::SegmentImportance(importance) => {
                let position = annotated(&positions, importance.segment_id)?;
                segments[position].important = importance.important;
            }
        }
    }
    Ok(JournalReplay {
        last_sequence: records.last().map_or(0, |record| record.sequence),
        last_checksum: records.last().map(|record| record.checksum.clone()),
        last_recorded_at: records.last().map(|record| record.recorded_at.clone()),
        applied_events: events.len(),
        ignored_duplicate_events,
        discarded_torn_tail,
        state,
        finalized_segments: segments,
    })
}

fn empty_replay() -> JournalReplay {
    JournalReplay {
        last_sequence: 0,
        last_checksum: None,
        last_recorded_at: None,
        applied_events: 0,
        ignored_duplicate_events: 0,
        discarded_torn_tail: false,
        state: SessionState::Idle,
        finalized_segments: Vec::new(),
    }
}

fn checksum(record: &UnsignedRecord) -> Result<String, SessionJournalError> {
    let bytes = serde_json::to_vec(record)
        .map_err(|_| SessionJournalError::new("session_journal_record_invalid"))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect())
}

fn read_bounded(file: &mut File) -> Result<Vec<u8>, SessionJournalError> {
    let length = file
        .metadata()
        .map_err(|_| SessionJournalError::new("session_journal_read_failed"))?
        .len();
    if length > MAX_JOURNAL_BYTES {
        return Err(SessionJournalError::new("session_journal_limit_exceeded"));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| SessionJournalError::new("session_journal_read_failed"))?;
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(MAX_JOURNAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SessionJournalError::new("session_journal_read_failed"))?;
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(SessionJournalError::new("session_journal_limit_exceeded"));
    }
    Ok(bytes)
}

#[cfg(windows)]
fn open_journal(path: &Path) -> Result<File, SessionJournalError> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| SessionJournalError::new("session_journal_open_failed"))?;
    reject_reparse_file(&file)?;
    Ok(file)
}

#[cfg(windows)]
fn open_existing_journal(path: &Path) -> Result<File, SessionJournalError> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| SessionJournalError::new("session_journal_open_failed"))?;
    reject_reparse_file(&file)?;
    Ok(file)
}

#[cfg(windows)]
fn reject_reparse_file(file: &File) -> Result<(), SessionJournalError> {
    if file
        .metadata()
        .map_err(|_| SessionJournalError::new("session_journal_open_failed"))?
        .file_attributes()
        & FILE_ATTRIBUTE_REPARSE_POINT
        != 0
    {
        return Err(SessionJournalError::new("session_journal_path_unsafe"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn open_journal(_path: &Path) -> Result<File, SessionJournalError> {
    Err(SessionJournalError::new("session_journal_windows_only"))
}

#[cfg(not(windows))]
fn open_existing_journal(_path: &Path) -> Result<File, SessionJournalError> {
    Err(SessionJournalError::new("session_journal_windows_only"))
}

fn valid_transition(previous: SessionState, current: SessionState) -> bool {
    use SessionState::{
        Capturing, Completed, Failed, Idle, Paused, Preparing, ProcessingSummary, Stopping,
        Transcribing,
    };
    matches!(
        (previous, current),
        (Idle, Preparing)
            | (Preparing, Capturing)
            | (Capturing, Transcribing)
            | (Transcribing, Paused)
            | (Paused, Preparing)
            | (Capturing | Transcribing | Paused, Stopping)
            | (Stopping, ProcessingSummary | Completed)
            | (ProcessingSummary, Completed)
            | (
                Preparing | Capturing | Transcribing | Paused | Stopping | ProcessingSummary,
                Failed
            )
            | (Failed, Preparing)
    )
}

fn map_store_error(error: SessionStoreError) -> SessionJournalError {
    match error.code {
        "session_path_unsafe" | "session_path_identity_changed" => {
            SessionJournalError::new("session_journal_path_unsafe")
        }
        "session_snapshot_missing" => SessionJournalError::new("session_journal_session_missing"),
        _ => SessionJournalError::new("session_journal_session_unavailable"),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        domain::{Project, Session},
        persistence::{ProjectStore, SessionStore},
    };

    use super::*;

    fn records() -> (Project, Session) {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/project-session-v1.json"
        ))
        .unwrap();
        (
            serde_json::from_value(fixture["project"].clone()).unwrap(),
            serde_json::from_value(fixture["session"].clone()).unwrap(),
        )
    }

    fn workspace_with_session() -> (
        tempfile::TempDir,
        Project,
        Session,
        SessionLocator,
        SessionJournal,
    ) {
        let workspace = tempfile::tempdir().unwrap();
        let (project, session) = records();
        ProjectStore::open(workspace.path())
            .unwrap()
            .create_project(&project)
            .unwrap();
        SessionStore::open(workspace.path())
            .unwrap()
            .create_session(&project, &session)
            .unwrap();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        let journal = SessionJournal::open(workspace.path()).unwrap();
        (workspace, project, session, locator, journal)
    }

    fn journal_path(workspace: &Path, project: &Project, session: &Session) -> std::path::PathBuf {
        workspace
            .join("projects")
            .join(&project.folder_name)
            .join("sessions")
            .join(&session.folder_name)
            .join(JOURNAL_FILE)
    }

    fn lifecycle(event_id: Uuid, previous: SessionState, current: SessionState) -> JournalAppend {
        JournalAppend {
            event_id,
            recorded_at: "2026-08-11T10:00:00Z".to_owned(),
            mutation: JournalMutation::LifecycleChange(LifecycleChange {
                previous,
                current,
                occurred_at: "2026-08-11T10:00:00Z".to_owned(),
            }),
        }
    }

    fn segment(event_id: Uuid, segment_id: Uuid) -> JournalAppend {
        JournalAppend {
            event_id,
            recorded_at: "2026-08-11T10:00:01Z".to_owned(),
            mutation: JournalMutation::FinalizedTranscriptSegment(FinalizedTranscriptSegment {
                id: segment_id,
                source: AudioSource::Microphone,
                start_ms: 120,
                end_ms: 1_420,
                text: "A bounded finalized segment.".to_owned(),
                language: "en".to_owned(),
            }),
        }
    }

    fn correction(event_id: Uuid, segment_id: Uuid, text: &str) -> JournalAppend {
        JournalAppend {
            event_id,
            recorded_at: "2026-08-11T10:00:03Z".to_owned(),
            mutation: JournalMutation::SegmentCorrection(SegmentCorrection {
                segment_id,
                text: text.to_owned(),
            }),
        }
    }

    fn importance(event_id: Uuid, segment_id: Uuid, important: bool) -> JournalAppend {
        JournalAppend {
            event_id,
            recorded_at: "2026-08-11T10:00:04Z".to_owned(),
            mutation: JournalMutation::SegmentImportance(SegmentImportance {
                segment_id,
                important,
            }),
        }
    }

    /// The whole point of correcting through an append-only journal: the
    /// transcription as it was produced is never overwritten, so it survives
    /// every replay and stays recoverable.
    #[test]
    fn a_correction_never_overwrites_the_original() {
        let (workspace, project, session_record, locator, journal) = workspace_with_session();
        let segment_id = Uuid::new_v4();
        journal
            .append(&locator, segment(Uuid::new_v4(), segment_id))
            .unwrap();

        journal
            .append(
                &locator,
                correction(Uuid::new_v4(), segment_id, "A corrected segment."),
            )
            .unwrap();

        let replay = journal.replay(&locator).unwrap();
        let replayed = &replay.finalized_segments[0];
        assert_eq!(replayed.effective_text(), "A corrected segment.");
        assert_eq!(replayed.original_text(), "A bounded finalized segment.");
        assert_eq!(replayed.segment.text, "A bounded finalized segment.");
        assert!(replayed.corrected());
        // The original is still on disk verbatim, not just in memory.
        let bytes =
            fs::read_to_string(journal_path(workspace.path(), &project, &session_record)).unwrap();
        assert!(bytes.contains("A bounded finalized segment."));
        assert!(bytes.contains("A corrected segment."));
    }

    /// Correcting twice is two events, so the first correction and the original
    /// both remain in the log even though only the last one reads.
    #[test]
    fn the_latest_correction_wins_and_the_earlier_ones_remain() {
        let (workspace, project, session_record, locator, journal) = workspace_with_session();
        let segment_id = Uuid::new_v4();
        journal
            .append(&locator, segment(Uuid::new_v4(), segment_id))
            .unwrap();
        journal
            .append(
                &locator,
                correction(Uuid::new_v4(), segment_id, "First try."),
            )
            .unwrap();

        journal
            .append(
                &locator,
                correction(Uuid::new_v4(), segment_id, "Second try."),
            )
            .unwrap();

        let replay = journal.replay(&locator).unwrap();
        assert_eq!(replay.finalized_segments[0].effective_text(), "Second try.");
        assert_eq!(
            replay.finalized_segments[0].original_text(),
            "A bounded finalized segment."
        );
        let bytes =
            fs::read_to_string(journal_path(workspace.path(), &project, &session_record)).unwrap();
        assert!(bytes.contains("First try."));
    }

    #[test]
    fn importance_is_recorded_and_can_be_taken_back() {
        let (_workspace, _project, _session, locator, journal) = workspace_with_session();
        let segment_id = Uuid::new_v4();
        journal
            .append(&locator, segment(Uuid::new_v4(), segment_id))
            .unwrap();
        assert!(!journal.replay(&locator).unwrap().finalized_segments[0].important);

        journal
            .append(&locator, importance(Uuid::new_v4(), segment_id, true))
            .unwrap();
        assert!(journal.replay(&locator).unwrap().finalized_segments[0].important);

        journal
            .append(&locator, importance(Uuid::new_v4(), segment_id, false))
            .unwrap();
        assert!(!journal.replay(&locator).unwrap().finalized_segments[0].important);
    }

    /// An annotation is always written after the segment it names, so a
    /// reference to an unknown segment means the log is damaged rather than
    /// merely out of order. The append refuses it outright, so the orphan never
    /// reaches the file and cannot make the journal unreadable later.
    #[test]
    fn an_annotation_for_an_unknown_segment_is_refused() {
        let (workspace, project, session_record, locator, journal) = workspace_with_session();
        journal
            .append(&locator, segment(Uuid::new_v4(), Uuid::new_v4()))
            .unwrap();

        let refused = journal.append(
            &locator,
            correction(Uuid::new_v4(), Uuid::new_v4(), "Orphaned."),
        );

        assert_eq!(
            refused.unwrap_err().code,
            "session_journal_annotation_orphaned"
        );
        let bytes =
            fs::read_to_string(journal_path(workspace.path(), &project, &session_record)).unwrap();
        assert!(!bytes.contains("Orphaned."));
        assert_eq!(bytes.lines().count(), 1);
        // The journal still replays cleanly, so a refused annotation costs
        // nothing.
        assert_eq!(
            journal.replay(&locator).unwrap().finalized_segments.len(),
            1
        );
    }

    /// A correction is rendered where the original was, so it cannot carry
    /// anything the original could not.
    #[test]
    fn a_correction_obeys_the_same_text_bounds_as_a_transcription() {
        let (_workspace, _project, _session, locator, journal) = workspace_with_session();
        let segment_id = Uuid::new_v4();
        journal
            .append(&locator, segment(Uuid::new_v4(), segment_id))
            .unwrap();

        for rejected in ["", "carriage\rreturn", "bell\u{7}"] {
            assert_eq!(
                journal
                    .append(&locator, correction(Uuid::new_v4(), segment_id, rejected))
                    .unwrap_err()
                    .code,
                "session_journal_mutation_invalid"
            );
        }
        let oversized = "x".repeat(MAX_SEGMENT_TEXT_BYTES + 1);
        assert_eq!(
            journal
                .append(&locator, correction(Uuid::new_v4(), segment_id, &oversized))
                .unwrap_err()
                .code,
            "session_journal_mutation_invalid"
        );
        // Newlines and tabs stay allowed, exactly as in a transcription.
        journal
            .append(
                &locator,
                correction(Uuid::new_v4(), segment_id, "line\nbreak\tkept"),
            )
            .unwrap();
    }

    #[test]
    fn canonical_record_matches_the_frozen_golden_bytes() {
        let (workspace, project, session_record, locator, journal) = workspace_with_session();
        journal
            .append(
                &locator,
                segment(
                    Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap(),
                    Uuid::parse_str("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee").unwrap(),
                ),
            )
            .unwrap();

        assert_eq!(
            fs::read_to_string(journal_path(workspace.path(), &project, &session_record)).unwrap(),
            include_str!("../../../fixtures/persistence/session-journal-v1.jsonl")
        );
    }

    #[test]
    fn append_synchronizes_versioned_checksummed_records_and_replays_deterministically() {
        let (workspace, project, session_record, locator, journal) = workspace_with_session();
        let lifecycle_id = Uuid::new_v4();
        let segment_id = Uuid::new_v4();
        let first = journal
            .append(
                &locator,
                lifecycle(lifecycle_id, SessionState::Idle, SessionState::Preparing),
            )
            .unwrap();
        let second = journal
            .append(&locator, segment(Uuid::new_v4(), segment_id))
            .unwrap();

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        assert_eq!(first.checksum.len(), 64);
        assert!(!first.already_committed);
        let bytes = fs::read(journal_path(workspace.path(), &project, &session_record)).unwrap();
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(bytes.split(|value| *value == b'\n').count(), 3);

        let replay = journal.replay(&locator).unwrap();
        assert_eq!(replay.last_sequence, 2);
        assert_eq!(replay.applied_events, 2);
        assert_eq!(replay.state, SessionState::Preparing);
        assert_eq!(replay.finalized_segments.len(), 1);
        assert_eq!(replay.finalized_segments[0].segment.id, segment_id);
        assert_eq!(journal.replay(&locator).unwrap(), replay);
    }

    #[test]
    fn retrying_the_same_event_is_idempotent_and_conflicting_reuse_fails() {
        let (workspace, project, session_record, locator, journal) = workspace_with_session();
        let event_id = Uuid::new_v4();
        let append = lifecycle(event_id, SessionState::Idle, SessionState::Preparing);

        let first = journal.append(&locator, append.clone()).unwrap();
        let retry = journal.append(&locator, append).unwrap();

        assert_eq!(retry.sequence, first.sequence);
        assert_eq!(retry.checksum, first.checksum);
        assert!(retry.already_committed);
        let path = journal_path(workspace.path(), &project, &session_record);
        assert_eq!(fs::read_to_string(path).unwrap().lines().count(), 1);
        let mut conflicting = lifecycle(event_id, SessionState::Idle, SessionState::Preparing);
        conflicting.recorded_at = "2026-08-11T10:00:02Z".to_owned();
        assert_eq!(
            journal.append(&locator, conflicting).unwrap_err().code,
            "session_journal_event_conflict"
        );
    }

    #[test]
    fn replay_discards_only_a_torn_final_record_and_next_append_repairs_the_tail() {
        let (workspace, project, session_record, locator, journal) = workspace_with_session();
        journal
            .append(
                &locator,
                lifecycle(Uuid::new_v4(), SessionState::Idle, SessionState::Preparing),
            )
            .unwrap();
        let path = journal_path(workspace.path(), &project, &session_record);
        let valid_length = fs::metadata(&path).unwrap().len();
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"schemaVersion\":1").unwrap();
        file.sync_all().unwrap();
        drop(file);

        let replay = journal.replay(&locator).unwrap();
        assert!(replay.discarded_torn_tail);
        assert_eq!(replay.last_sequence, 1);
        let torn_length = fs::metadata(&path).unwrap().len();
        assert_eq!(
            journal
                .append(
                    &locator,
                    lifecycle(
                        Uuid::new_v4(),
                        SessionState::Capturing,
                        SessionState::Stopping,
                    ),
                )
                .unwrap_err()
                .code,
            "session_journal_lifecycle_conflict"
        );
        assert_eq!(fs::metadata(&path).unwrap().len(), torn_length);

        let receipt = journal
            .append(&locator, segment(Uuid::new_v4(), Uuid::new_v4()))
            .unwrap();
        assert_eq!(receipt.sequence, 2);
        assert!(fs::metadata(path).unwrap().len() > valid_length);
        let repaired = journal.replay(&locator).unwrap();
        assert!(!repaired.discarded_torn_tail);
        assert_eq!(repaired.last_sequence, 2);
    }

    #[test]
    fn complete_corruption_and_invalid_lifecycle_replay_fail_closed() {
        let (workspace, project, session_record, locator, journal) = workspace_with_session();
        journal
            .append(
                &locator,
                lifecycle(Uuid::new_v4(), SessionState::Idle, SessionState::Preparing),
            )
            .unwrap();
        let path = journal_path(workspace.path(), &project, &session_record);
        let mut bytes = fs::read(&path).unwrap();
        let checksum = bytes
            .windows(b"\"checksum\":\"".len())
            .position(|window| window == b"\"checksum\":\"")
            .unwrap()
            + b"\"checksum\":\"".len();
        bytes[checksum] = if bytes[checksum] == b'a' { b'b' } else { b'a' };
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            journal.replay(&locator).unwrap_err().code,
            "session_journal_checksum_mismatch"
        );

        fs::remove_file(&path).unwrap();
        journal
            .append(
                &locator,
                lifecycle(Uuid::new_v4(), SessionState::Idle, SessionState::Preparing),
            )
            .unwrap();
        assert_eq!(
            journal
                .append(
                    &locator,
                    lifecycle(
                        Uuid::new_v4(),
                        SessionState::Capturing,
                        SessionState::Stopping,
                    ),
                )
                .unwrap_err()
                .code,
            "session_journal_lifecycle_conflict"
        );
        assert_eq!(journal.replay(&locator).unwrap().last_sequence, 1);
    }
}
