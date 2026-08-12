CREATE TABLE transcript_index_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    generation INTEGER NOT NULL CHECK (generation >= 1),
    workspace_key TEXT NOT NULL CHECK (length(workspace_key) = 64),
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64)
);

CREATE TABLE transcript_index_segments (
    row_id INTEGER PRIMARY KEY,
    project_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    segment_id TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL CHECK (source IN ('microphone', 'system_output')),
    start_ms INTEGER NOT NULL CHECK (start_ms >= 0),
    end_ms INTEGER NOT NULL CHECK (end_ms > start_ms),
    language TEXT NOT NULL CHECK (length(language) BETWEEN 1 AND 64),
    segment_text TEXT NOT NULL CHECK (length(segment_text) BETWEEN 1 AND 32768),
    checkpoint_sequence INTEGER NOT NULL CHECK (checkpoint_sequence >= 1),
    checkpoint_checksum TEXT NOT NULL CHECK (length(checkpoint_checksum) = 64),
    snapshot_sha256 BLOB NOT NULL CHECK (length(snapshot_sha256) = 32)
);

CREATE INDEX transcript_index_scope
    ON transcript_index_segments(project_id, session_id, start_ms, segment_id);

CREATE VIRTUAL TABLE transcript_index_fts USING fts5(
    segment_text,
    content = 'transcript_index_segments',
    content_rowid = 'row_id',
    tokenize = 'unicode61 remove_diacritics 2'
);
