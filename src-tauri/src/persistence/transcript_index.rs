// Canonical transcript Markdown remains authoritative; this FTS5 database is rebuildable.
#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    audio::AudioSource,
    domain::{ProjectId, SessionId},
};

use super::transcript_store::{TranscriptDiscoveryReport, TranscriptStore, TranscriptStoreError};

const MIGRATION: &str = include_str!("../../migrations/transcript_index_v1.sql");
const DATABASE_NAME: &str = "transcript-index.sqlite3";
const SCHEMA_VERSION: u32 = 1;
const CURSOR_PREFIX: &str = "t1:";
const MAX_QUERY_BYTES: usize = 256;
const MAX_QUERY_TERMS: usize = 16;
const MAX_RESULT_LIMIT: u16 = 100;
const MAX_SNIPPET_CHARS: usize = 240;
const MAX_CURSOR_OFFSET: u32 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptSearchRequest {
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: Option<SessionId>,
    pub(crate) query: String,
    pub(crate) cursor: Option<String>,
    pub(crate) limit: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptSearchResult {
    pub(crate) project_id: ProjectId,
    pub(crate) session_id: SessionId,
    pub(crate) segment_id: Uuid,
    pub(crate) source: AudioSource,
    pub(crate) start_ms: u64,
    pub(crate) end_ms: u64,
    pub(crate) language: String,
    pub(crate) snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptSearchPage {
    pub(crate) items: Vec<TranscriptSearchResult>,
    pub(crate) next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptIndexRebuildReport {
    pub(crate) generation: u64,
    pub(crate) indexed_transcripts: u32,
    pub(crate) indexed_segments: u32,
    pub(crate) discovery: TranscriptDiscoveryReport,
    pub(crate) quarantined_corrupt_index: bool,
}

pub(crate) struct TranscriptSearchCatalog {
    store: TranscriptStore,
    app_data_directory: PathBuf,
    database_path: PathBuf,
    workspace_key: String,
    operation_lock: Mutex<()>,
}

impl TranscriptSearchCatalog {
    pub(crate) fn open(
        workspace_path: &Path,
        app_data_directory: PathBuf,
    ) -> Result<Self, TranscriptStoreError> {
        let store = TranscriptStore::open(workspace_path)?;
        let canonical = fs::canonicalize(workspace_path)
            .map_err(|_| TranscriptStoreError::new("transcript_search_workspace_invalid"))?;
        let normalized = canonical.to_string_lossy().to_lowercase();
        let workspace_key = hex_sha256(normalized.as_bytes());
        Ok(Self {
            database_path: app_data_directory.join(DATABASE_NAME),
            app_data_directory,
            store,
            workspace_key,
            operation_lock: Mutex::new(()),
        })
    }

    pub(crate) fn rebuild_index(
        &self,
    ) -> Result<TranscriptIndexRebuildReport, TranscriptStoreError> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| TranscriptStoreError::new("transcript_search_unavailable"))?;
        self.rebuild_index_with_fault(None)
    }

    pub(crate) fn search(
        &self,
        request: &TranscriptSearchRequest,
    ) -> Result<TranscriptSearchPage, TranscriptStoreError> {
        let validated = ValidatedSearch::new(request)?;
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| TranscriptStoreError::new("transcript_search_unavailable"))?;
        match self.search_locked(&validated) {
            Err(error) if error.code == "transcript_search_corrupt" => {
                self.quarantine_corrupt_index()?;
                self.rebuild_index_with_fault(None)?;
                self.search_locked(&validated)
            }
            Err(error) if error.code == "transcript_search_index_invalid" => {
                self.rebuild_index_with_fault(None)?;
                self.search_locked(&validated)
            }
            result => result,
        }
    }

    fn search_locked(
        &self,
        search: &ValidatedSearch,
    ) -> Result<TranscriptSearchPage, TranscriptStoreError> {
        let connection = self.open_connection_without_recovery()?;
        if !index_has_projection(&connection, &self.workspace_key)? {
            drop(connection);
            self.rebuild_index_with_fault(None)?;
        }
        let connection = self.open_connection_without_recovery()?;
        read_search_page(&connection, search)
    }

    fn rebuild_index_with_fault(
        &self,
        fault: Option<RebuildFault>,
    ) -> Result<TranscriptIndexRebuildReport, TranscriptStoreError> {
        let discovery = self.store.discover_transcripts()?;
        let indexed_segments = discovery
            .transcripts
            .iter()
            .try_fold(0_u32, |total, transcript| {
                total.checked_add(transcript.snapshot.segment_count as u32)
            })
            .ok_or_else(|| TranscriptStoreError::new("transcript_search_limit_exceeded"))?;
        let (mut connection, mut quarantined) = match self.open_connection_without_recovery() {
            Ok(connection) => (connection, false),
            Err(error) if error.code == "transcript_search_corrupt" => {
                self.quarantine_corrupt_index()?;
                (self.open_connection_without_recovery()?, true)
            }
            Err(error) => return Err(error),
        };
        let generation =
            match rebuild_on_connection(&mut connection, &discovery, &self.workspace_key, fault) {
                Err(error) if error.code == "transcript_search_corrupt" && !quarantined => {
                    drop(connection);
                    self.quarantine_corrupt_index()?;
                    quarantined = true;
                    let mut recovered = self.open_connection_without_recovery()?;
                    rebuild_on_connection(&mut recovered, &discovery, &self.workspace_key, fault)?
                }
                result => result?,
            };
        Ok(TranscriptIndexRebuildReport {
            generation,
            indexed_transcripts: discovery.transcripts.len() as u32,
            indexed_segments,
            discovery,
            quarantined_corrupt_index: quarantined,
        })
    }

    fn open_connection_without_recovery(&self) -> Result<Connection, TranscriptStoreError> {
        fs::create_dir_all(&self.app_data_directory)
            .map_err(|_| TranscriptStoreError::new("transcript_search_unavailable"))?;
        let mut connection = Connection::open(&self.database_path)
            .map_err(|_| TranscriptStoreError::new("transcript_search_unavailable"))?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|_| TranscriptStoreError::new("transcript_search_unavailable"))?;
        let version = raw_schema_version(&connection).map_err(map_read_error)?;
        ensure_schema(&mut connection, version)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
            )
            .map_err(map_read_error)?;
        Ok(connection)
    }

    fn quarantine_corrupt_index(&self) -> Result<(), TranscriptStoreError> {
        let suffix = format!(".corrupt-{}", Uuid::new_v4());
        for source in database_family_paths(&self.database_path) {
            if source.exists() {
                let mut destination = source.as_os_str().to_owned();
                destination.push(&suffix);
                fs::rename(&source, PathBuf::from(destination))
                    .map_err(|_| TranscriptStoreError::new("transcript_search_unavailable"))?;
            }
        }
        tracing::warn!("physically corrupt rebuildable transcript search index quarantined");
        Ok(())
    }
}

struct ValidatedSearch {
    project_id: String,
    session_id: Option<String>,
    fts_query: String,
    query_terms: Vec<String>,
    query_key: String,
    cursor: DecodedCursor,
    limit: u16,
}

impl ValidatedSearch {
    fn new(request: &TranscriptSearchRequest) -> Result<Self, TranscriptStoreError> {
        if request.query.is_empty()
            || request.query.len() > MAX_QUERY_BYTES
            || request.query.chars().any(|value| value.is_control())
            || !(1..=MAX_RESULT_LIMIT).contains(&request.limit)
        {
            return Err(TranscriptStoreError::new(
                "transcript_search_request_invalid",
            ));
        }
        let query_terms = request
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>();
        if query_terms.is_empty() || query_terms.len() > MAX_QUERY_TERMS {
            return Err(TranscriptStoreError::new(
                "transcript_search_request_invalid",
            ));
        }
        let fts_query = query_terms
            .iter()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let project_id = id_text(request.project_id)?;
        let session_id = request.session_id.map(id_text).transpose()?;
        let query_key = hex_sha256(request.query.to_lowercase().as_bytes())[..16].to_owned();
        let cursor = decode_cursor(
            request.cursor.as_deref(),
            &project_id,
            session_id.as_deref(),
            &query_key,
        )?;
        Ok(Self {
            project_id,
            session_id,
            fts_query,
            query_terms,
            query_key,
            cursor,
            limit: request.limit,
        })
    }
}

#[derive(Clone, Copy)]
struct DecodedCursor {
    generation: Option<u64>,
    offset: u32,
}

fn ensure_schema(connection: &mut Connection, version: u32) -> Result<(), TranscriptStoreError> {
    if version > SCHEMA_VERSION {
        return Err(TranscriptStoreError::new("transcript_search_future_schema"));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_write_error)?;
    transaction
        .execute_batch(MIGRATION)
        .and_then(|()| transaction.pragma_update(None, "user_version", SCHEMA_VERSION))
        .map_err(map_write_error)?;
    transaction.commit().map_err(map_write_error)
}

fn raw_schema_version(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn index_has_projection(
    connection: &Connection,
    workspace_key: &str,
) -> Result<bool, TranscriptStoreError> {
    let meta = connection
        .query_row(
            "SELECT generation, workspace_key, content_sha256 FROM transcript_index_meta
             WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_read_error)?;
    let Some((generation, key, expected_digest)) = meta else {
        return Ok(false);
    };
    if generation < 1 || key != workspace_key {
        return Ok(false);
    }
    let entries: i64 = connection
        .query_row(
            "SELECT count(*) FROM transcript_index_segments",
            [],
            |row| row.get(0),
        )
        .map_err(map_read_error)?;
    let fts: i64 = connection
        .query_row("SELECT count(*) FROM transcript_index_fts", [], |row| {
            row.get(0)
        })
        .map_err(map_read_error)?;
    if entries != fts || entries < 0 {
        return Err(TranscriptStoreError::new("transcript_search_index_invalid"));
    }
    if expected_digest != projection_digest(connection)? {
        return Err(TranscriptStoreError::new("transcript_search_index_invalid"));
    }
    Ok(true)
}

fn rebuild_on_connection(
    connection: &mut Connection,
    discovery: &TranscriptDiscoveryReport,
    workspace_key: &str,
    fault: Option<RebuildFault>,
) -> Result<u64, TranscriptStoreError> {
    let current = connection
        .query_row(
            "SELECT generation FROM transcript_index_meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(map_read_error)?
        .unwrap_or(0);
    let generation = current
        .checked_add(1)
        .ok_or_else(|| TranscriptStoreError::new("transcript_search_generation_exhausted"))?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_write_error)?;
    transaction
        .execute("DELETE FROM transcript_index_segments", [])
        .map_err(map_write_error)?;
    inject_fault(fault, RebuildFault::AfterDelete)?;
    let mut row_id = 0_i64;
    for transcript in &discovery.transcripts {
        let project_id = id_text(transcript.session.project_id)?;
        let session_id = id_text(transcript.session.id)?;
        for segment in &transcript.snapshot.segments {
            let checksum = transcript
                .snapshot
                .checkpoint_checksum
                .as_deref()
                .ok_or_else(|| TranscriptStoreError::new("transcript_search_index_invalid"))?;
            row_id = row_id
                .checked_add(1)
                .ok_or_else(|| TranscriptStoreError::new("transcript_search_limit_exceeded"))?;
            let source = match segment.source {
                AudioSource::Microphone => "microphone",
                AudioSource::SystemOutput => "system_output",
            };
            transaction
                .execute(
                    "INSERT INTO transcript_index_segments (
                        row_id, project_id, session_id, segment_id, source, start_ms, end_ms,
                        language, segment_text, checkpoint_sequence, checkpoint_checksum,
                        snapshot_sha256
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        row_id,
                        project_id,
                        session_id,
                        segment.id.to_string(),
                        source,
                        i64::try_from(segment.start_ms).map_err(|_| {
                            TranscriptStoreError::new("transcript_search_index_invalid")
                        })?,
                        i64::try_from(segment.end_ms).map_err(|_| {
                            TranscriptStoreError::new("transcript_search_index_invalid")
                        })?,
                        segment.language,
                        segment.text,
                        i64::try_from(transcript.snapshot.checkpoint_sequence).map_err(|_| {
                            TranscriptStoreError::new("transcript_search_index_invalid")
                        })?,
                        checksum,
                        transcript.snapshot.fingerprint.as_bytes().as_slice(),
                    ],
                )
                .map_err(map_write_error)?;
        }
    }
    transaction
        .execute(
            "INSERT INTO transcript_index_fts(transcript_index_fts) VALUES('rebuild')",
            [],
        )
        .map_err(map_write_error)?;
    inject_fault(fault, RebuildFault::AfterInsert)?;
    let content_sha256 = projection_digest(&transaction)?;
    transaction
        .execute(
            "INSERT INTO transcript_index_meta (
                singleton, generation, workspace_key, content_sha256
             ) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(singleton) DO UPDATE SET generation = excluded.generation,
                 workspace_key = excluded.workspace_key,
                 content_sha256 = excluded.content_sha256",
            params![generation, workspace_key, content_sha256],
        )
        .map_err(map_write_error)?;
    transaction.commit().map_err(map_write_error)?;
    u64::try_from(generation)
        .map_err(|_| TranscriptStoreError::new("transcript_search_index_invalid"))
}

fn projection_digest(connection: &Connection) -> Result<String, TranscriptStoreError> {
    let mut statement = connection
        .prepare(
            "SELECT project_id, session_id, segment_id, source, start_ms, end_ms, language,
                    segment_text, checkpoint_sequence, checkpoint_checksum, snapshot_sha256
             FROM transcript_index_segments ORDER BY row_id",
        )
        .map_err(map_read_error)?;
    let mut rows = statement.query([]).map_err(map_read_error)?;
    let mut digest = Sha256::new();
    while let Some(row) = rows.next().map_err(map_read_error)? {
        for column in 0..10 {
            let value = row.get_ref(column).map_err(map_read_error)?;
            let bytes = match value {
                rusqlite::types::ValueRef::Integer(value) => value.to_le_bytes().to_vec(),
                rusqlite::types::ValueRef::Text(value) => value.to_vec(),
                _ => {
                    return Err(TranscriptStoreError::new("transcript_search_index_invalid"));
                }
            };
            digest.update((bytes.len() as u64).to_le_bytes());
            digest.update(bytes);
        }
        let fingerprint = row.get_ref(10).map_err(map_read_error)?;
        let rusqlite::types::ValueRef::Blob(fingerprint) = fingerprint else {
            return Err(TranscriptStoreError::new("transcript_search_index_invalid"));
        };
        digest.update((fingerprint.len() as u64).to_le_bytes());
        digest.update(fingerprint);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn read_search_page(
    connection: &Connection,
    search: &ValidatedSearch,
) -> Result<TranscriptSearchPage, TranscriptStoreError> {
    let generation = connection
        .query_row(
            "SELECT generation FROM transcript_index_meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_read_error)?;
    let generation = u64::try_from(generation)
        .map_err(|_| TranscriptStoreError::new("transcript_search_index_invalid"))?;
    if search
        .cursor
        .generation
        .is_some_and(|cursor| cursor != generation)
    {
        return Err(TranscriptStoreError::new("transcript_search_page_stale"));
    }
    let (sql, session_parameter) = if search.session_id.is_some() {
        (
            "SELECT s.project_id, s.session_id, s.segment_id, s.source, s.start_ms,
                    s.end_ms, s.language, s.segment_text, s.checkpoint_sequence,
                    s.checkpoint_checksum, s.snapshot_sha256
             FROM transcript_index_fts
             JOIN transcript_index_segments s ON s.row_id = transcript_index_fts.rowid
             WHERE transcript_index_fts MATCH ?1 AND s.project_id = ?2 AND s.session_id = ?3
             ORDER BY bm25(transcript_index_fts), s.start_ms, s.segment_id LIMIT ?4 OFFSET ?5",
            true,
        )
    } else {
        (
            "SELECT s.project_id, s.session_id, s.segment_id, s.source, s.start_ms,
                    s.end_ms, s.language, s.segment_text, s.checkpoint_sequence,
                    s.checkpoint_checksum, s.snapshot_sha256
             FROM transcript_index_fts
             JOIN transcript_index_segments s ON s.row_id = transcript_index_fts.rowid
             WHERE transcript_index_fts MATCH ?1 AND s.project_id = ?2
             ORDER BY bm25(transcript_index_fts), s.start_ms, s.segment_id LIMIT ?3 OFFSET ?4",
            false,
        )
    };
    let mut statement = connection.prepare(sql).map_err(map_read_error)?;
    let fetch_limit = u32::from(search.limit) + 1;
    let mut rows = if session_parameter {
        statement
            .query(params![
                search.fts_query,
                search.project_id,
                search.session_id.as_deref().unwrap_or_default(),
                fetch_limit,
                search.cursor.offset
            ])
            .map_err(map_read_error)?
    } else {
        statement
            .query(params![
                search.fts_query,
                search.project_id,
                fetch_limit,
                search.cursor.offset
            ])
            .map_err(map_read_error)?
    };
    let mut items = Vec::with_capacity(usize::from(search.limit) + 1);
    while let Some(row) = rows.next().map_err(map_read_error)? {
        let project_id: String = row.get(0).map_err(map_read_error)?;
        let session_id: String = row.get(1).map_err(map_read_error)?;
        let segment_id: String = row.get(2).map_err(map_read_error)?;
        let source: String = row.get(3).map_err(map_read_error)?;
        let start_ms: i64 = row.get(4).map_err(map_read_error)?;
        let end_ms: i64 = row.get(5).map_err(map_read_error)?;
        let language: String = row.get(6).map_err(map_read_error)?;
        let text: String = row.get(7).map_err(map_read_error)?;
        let checkpoint_sequence: i64 = row.get(8).map_err(map_read_error)?;
        let checkpoint_checksum: String = row.get(9).map_err(map_read_error)?;
        let snapshot_sha256: Vec<u8> = row.get(10).map_err(map_read_error)?;
        if project_id != search.project_id
            || search
                .session_id
                .as_deref()
                .is_some_and(|expected| expected != session_id)
            || checkpoint_sequence < 1
            || checkpoint_checksum.len() != 64
            || snapshot_sha256.len() != 32
            || language.is_empty()
            || language.len() > 64
            || text.is_empty()
            || text.len() > 32 * 1024
            || start_ms < 0
            || end_ms <= start_ms
        {
            return Err(TranscriptStoreError::new("transcript_search_index_invalid"));
        }
        items.push(TranscriptSearchResult {
            project_id: parse_id(&project_id)?,
            session_id: parse_id(&session_id)?,
            segment_id: Uuid::parse_str(&segment_id)
                .map_err(|_| TranscriptStoreError::new("transcript_search_index_invalid"))?,
            source: match source.as_str() {
                "microphone" => AudioSource::Microphone,
                "system_output" => AudioSource::SystemOutput,
                _ => {
                    return Err(TranscriptStoreError::new("transcript_search_index_invalid"));
                }
            },
            start_ms: start_ms as u64,
            end_ms: end_ms as u64,
            language,
            snippet: stable_snippet(&text, &search.query_terms),
        });
    }
    let has_more = items.len() > usize::from(search.limit);
    if has_more {
        items.pop();
    }
    let scope = search.session_id.as_deref().unwrap_or("*");
    Ok(TranscriptSearchPage {
        next_cursor: has_more.then(|| {
            format!(
                "{CURSOR_PREFIX}{generation}:{}:{scope}:{}:{}",
                search.project_id,
                search.query_key,
                search
                    .cursor
                    .offset
                    .checked_add(u32::from(search.limit))
                    .unwrap_or(MAX_CURSOR_OFFSET)
            )
        }),
        items,
    })
}

fn decode_cursor(
    cursor: Option<&str>,
    project_id: &str,
    session_id: Option<&str>,
    query_key: &str,
) -> Result<DecodedCursor, TranscriptStoreError> {
    let Some(cursor) = cursor else {
        return Ok(DecodedCursor {
            generation: None,
            offset: 0,
        });
    };
    let encoded = cursor
        .strip_prefix(CURSOR_PREFIX)
        .ok_or_else(|| TranscriptStoreError::new("transcript_search_cursor_invalid"))?;
    let parts = encoded.split(':').collect::<Vec<_>>();
    let expected_scope = session_id.unwrap_or("*");
    if parts.len() != 5
        || parts[1] != project_id
        || parts[2] != expected_scope
        || parts[3] != query_key
    {
        return Err(TranscriptStoreError::new(
            "transcript_search_cursor_invalid",
        ));
    }
    let generation = parts[0]
        .parse::<u64>()
        .ok()
        .filter(|value| *value >= 1)
        .ok_or_else(|| TranscriptStoreError::new("transcript_search_cursor_invalid"))?;
    let offset = parts[4]
        .parse::<u32>()
        .ok()
        .filter(|value| *value <= MAX_CURSOR_OFFSET)
        .ok_or_else(|| TranscriptStoreError::new("transcript_search_cursor_invalid"))?;
    Ok(DecodedCursor {
        generation: Some(generation),
        offset,
    })
}

fn stable_snippet(text: &str, terms: &[String]) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut previous_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            if !previous_space {
                normalized.push(' ');
            }
            previous_space = true;
        } else {
            normalized.push(character);
            previous_space = false;
        }
    }
    let normalized = normalized.trim();
    let lower = normalized.to_lowercase();
    let byte_start = terms
        .iter()
        .filter_map(|term| lower.find(term))
        .min()
        .unwrap_or(0);
    let char_start = normalized[..byte_start].chars().count().saturating_sub(48);
    let chars = normalized.chars().collect::<Vec<_>>();
    let end = (char_start + MAX_SNIPPET_CHARS).min(chars.len());
    let mut snippet = chars[char_start..end].iter().collect::<String>();
    if char_start > 0 {
        snippet.insert(0, '…');
    }
    if end < chars.len() {
        snippet.push('…');
    }
    snippet
}

fn id_text<T: serde::Serialize>(id: T) -> Result<String, TranscriptStoreError> {
    serde_json::to_value(id)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| TranscriptStoreError::new("transcript_search_index_invalid"))
}

fn parse_id<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, TranscriptStoreError> {
    serde_json::from_value(serde_json::Value::String(text.to_owned()))
        .map_err(|_| TranscriptStoreError::new("transcript_search_index_invalid"))
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_physical_corruption(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if matches!(
                sqlite_error.code,
                rusqlite::ffi::ErrorCode::DatabaseCorrupt
                    | rusqlite::ffi::ErrorCode::NotADatabase
            )
    )
}

fn map_read_error(error: rusqlite::Error) -> TranscriptStoreError {
    if is_physical_corruption(&error) {
        TranscriptStoreError::new("transcript_search_corrupt")
    } else {
        TranscriptStoreError::new("transcript_search_unavailable")
    }
}

fn map_write_error(error: rusqlite::Error) -> TranscriptStoreError {
    map_read_error(error)
}

fn database_family_paths(path: &Path) -> [PathBuf; 3] {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let mut shm = path.as_os_str().to_owned();
    shm.push("-shm");
    [path.to_path_buf(), wal.into(), shm.into()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebuildFault {
    AfterDelete,
    AfterInsert,
}

fn inject_fault(
    configured: Option<RebuildFault>,
    current: RebuildFault,
) -> Result<(), TranscriptStoreError> {
    if configured == Some(current) {
        Err(TranscriptStoreError::new(
            "transcript_search_fault_injected",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;

    use crate::{
        domain::{Project, Session},
        persistence::{
            FinalizedTranscriptSegment, JournalAppend, JournalMutation, ProjectStore,
            SessionJournal, SessionLocator, SessionStore, TranscriptStore,
        },
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

    fn session(base: &Session, id: &str, title: &str) -> Session {
        let mut value = serde_json::to_value(base).unwrap();
        value["id"] = serde_json::json!(id);
        value["title"] = serde_json::json!(title);
        value["folderName"] = serde_json::json!(format!(
            "2026-08-11-{}--{}",
            title.to_ascii_lowercase().replace(' ', "-"),
            &id.replace('-', "")[..8]
        ));
        serde_json::from_value(value).unwrap()
    }

    fn initialize_workspace(root: &Path) -> (Project, Session) {
        let (project, base) = records();
        ProjectStore::open(root)
            .unwrap()
            .create_project(&project)
            .unwrap();
        (project, base)
    }

    fn add_transcript(root: &Path, project: &Project, session: &Session, texts: &[&str]) {
        SessionStore::open(root)
            .unwrap()
            .create_session(project, session)
            .unwrap();
        let locator = SessionLocator::from_records(project, session).unwrap();
        let journal = SessionJournal::open(root).unwrap();
        for (index, text) in texts.iter().enumerate() {
            journal
                .append(
                    &locator,
                    JournalAppend {
                        event_id: Uuid::new_v4(),
                        recorded_at: format!("2026-08-11T10:00:{index:02}Z"),
                        mutation: JournalMutation::FinalizedTranscriptSegment(
                            FinalizedTranscriptSegment {
                                id: Uuid::new_v4(),
                                source: if index % 2 == 0 {
                                    AudioSource::Microphone
                                } else {
                                    AudioSource::SystemOutput
                                },
                                start_ms: index as u64 * 2_000 + 100,
                                end_ms: index as u64 * 2_000 + 1_100,
                                text: (*text).to_owned(),
                                language: "en-GB".to_owned(),
                            },
                        ),
                    },
                )
                .unwrap();
        }
        TranscriptStore::open(root)
            .unwrap()
            .materialize(&locator, None)
            .unwrap();
    }

    fn request(
        project: &Project,
        session: Option<&Session>,
        query: &str,
        limit: u16,
    ) -> TranscriptSearchRequest {
        TranscriptSearchRequest {
            project_id: project.id,
            session_id: session.map(|value| value.id),
            query: query.to_owned(),
            cursor: None,
            limit,
        }
    }

    #[test]
    fn rebuild_indexes_only_valid_markdown_and_searches_project_or_session_scope() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (project, base) = initialize_workspace(workspace.path());
        let first = session(
            &base,
            "11111111-1111-4111-8111-111111111111",
            "First session",
        );
        let second = session(
            &base,
            "22222222-2222-4222-8222-222222222222",
            "Second session",
        );
        let missing = session(
            &base,
            "33333333-3333-4333-8333-333333333333",
            "Missing transcript",
        );
        add_transcript(
            workspace.path(),
            &project,
            &first,
            &["Alpha decision belongs to the first session."],
        );
        add_transcript(
            workspace.path(),
            &project,
            &second,
            &["Alpha risk belongs to the second session."],
        );
        SessionStore::open(workspace.path())
            .unwrap()
            .create_session(&project, &missing)
            .unwrap();

        let catalog =
            TranscriptSearchCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        let report = catalog.rebuild_index().unwrap();
        assert_eq!(report.indexed_transcripts, 2);
        assert_eq!(report.indexed_segments, 2);
        assert_eq!(report.discovery.issues.len(), 1);

        let project_results = catalog
            .search(&request(&project, None, "alpha", 10))
            .unwrap();
        assert_eq!(project_results.items.len(), 2);
        let session_results = catalog
            .search(&request(&project, Some(&first), "alpha", 10))
            .unwrap();
        assert_eq!(session_results.items.len(), 1);
        assert_eq!(session_results.items[0].session_id, first.id);
        assert!(session_results.items[0].snippet.contains("Alpha decision"));
    }

    #[test]
    fn cursors_bind_generation_query_project_and_session() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (project, base) = initialize_workspace(workspace.path());
        add_transcript(
            workspace.path(),
            &project,
            &base,
            &[
                "Bounded search one",
                "Bounded search two",
                "Bounded search three",
            ],
        );
        let catalog =
            TranscriptSearchCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        let first = catalog
            .search(&request(&project, None, "bounded", 2))
            .unwrap();
        assert_eq!(first.items.len(), 2);
        let cursor = first.next_cursor.unwrap();

        let mut wrong_query = request(&project, None, "search", 2);
        wrong_query.cursor = Some(cursor.clone());
        assert_eq!(
            catalog.search(&wrong_query).unwrap_err().code,
            "transcript_search_cursor_invalid"
        );
        catalog.rebuild_index().unwrap();
        let mut stale = request(&project, None, "bounded", 2);
        stale.cursor = Some(cursor);
        assert_eq!(
            catalog.search(&stale).unwrap_err().code,
            "transcript_search_page_stale"
        );
    }

    #[test]
    fn physical_and_semantic_corruption_rebuild_from_markdown() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (project, base) = initialize_workspace(workspace.path());
        add_transcript(
            workspace.path(),
            &project,
            &base,
            &["Canonical searchable transcript"],
        );
        fs::write(app_data.path().join(DATABASE_NAME), b"not a database").unwrap();
        let catalog =
            TranscriptSearchCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        let report = catalog.rebuild_index().unwrap();
        assert!(report.quarantined_corrupt_index);

        let connection = Connection::open(&catalog.database_path).unwrap();
        connection
            .execute(
                "UPDATE transcript_index_segments SET segment_text = 'tampered but valid'",
                [],
            )
            .unwrap();
        drop(connection);
        let results = catalog
            .search(&request(&project, None, "canonical", 10))
            .unwrap();
        assert_eq!(results.items.len(), 1);
        assert!(results.items[0].snippet.contains("Canonical"));
    }

    #[test]
    fn failed_rebuild_preserves_the_previous_generation() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (project, base) = initialize_workspace(workspace.path());
        add_transcript(
            workspace.path(),
            &project,
            &base,
            &["Previously acknowledged content"],
        );
        let catalog =
            TranscriptSearchCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        catalog.rebuild_index().unwrap();
        assert!(
            catalog
                .rebuild_index_with_fault(Some(RebuildFault::AfterDelete))
                .is_err()
        );
        let results = catalog
            .search(&request(&project, None, "acknowledged", 10))
            .unwrap();
        assert_eq!(results.items.len(), 1);
    }

    #[test]
    fn workspace_rebinding_never_leaks_the_path_or_prior_workspace_rows() {
        let first_workspace = tempfile::tempdir().unwrap();
        let second_workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (first_project, first_session) = initialize_workspace(first_workspace.path());
        let (second_project, second_session) = initialize_workspace(second_workspace.path());
        add_transcript(
            first_workspace.path(),
            &first_project,
            &first_session,
            &["Only first workspace token"],
        );
        add_transcript(
            second_workspace.path(),
            &second_project,
            &second_session,
            &["Only second workspace token"],
        );
        TranscriptSearchCatalog::open(first_workspace.path(), app_data.path().to_path_buf())
            .unwrap()
            .rebuild_index()
            .unwrap();
        let second =
            TranscriptSearchCatalog::open(second_workspace.path(), app_data.path().to_path_buf())
                .unwrap();
        assert_eq!(
            second
                .search(&request(&second_project, None, "second", 10))
                .unwrap()
                .items
                .len(),
            1
        );
        let bytes = fs::read(&second.database_path).unwrap();
        assert!(
            !String::from_utf8_lossy(&bytes)
                .contains(&second_workspace.path().display().to_string())
        );
    }

    #[test]
    fn invalid_requests_fail_before_database_access() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (project, _) = initialize_workspace(workspace.path());
        let catalog =
            TranscriptSearchCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        for invalid in [
            request(&project, None, "", 10),
            request(&project, None, "query", 0),
            request(&project, None, "query", 101),
            request(&project, None, "bad\nquery", 10),
        ] {
            assert_eq!(
                catalog.search(&invalid).unwrap_err().code,
                "transcript_search_request_invalid"
            );
        }
        assert!(!catalog.database_path.exists());
    }

    #[test]
    fn empty_transcripts_are_valid_and_future_schemas_are_preserved() {
        let workspace = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let (project, session) = initialize_workspace(workspace.path());
        SessionStore::open(workspace.path())
            .unwrap()
            .create_session(&project, &session)
            .unwrap();
        let locator = SessionLocator::from_records(&project, &session).unwrap();
        TranscriptStore::open(workspace.path())
            .unwrap()
            .materialize(&locator, None)
            .unwrap();
        let catalog =
            TranscriptSearchCatalog::open(workspace.path(), app_data.path().to_path_buf()).unwrap();
        let report = catalog.rebuild_index().unwrap();
        assert_eq!(report.indexed_transcripts, 1);
        assert_eq!(report.indexed_segments, 0);

        let connection = Connection::open(&catalog.database_path).unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();
        drop(connection);
        assert_eq!(
            catalog.rebuild_index().unwrap_err().code,
            "transcript_search_future_schema"
        );
        let version: u32 = Connection::open(&catalog.database_path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
    }
}
