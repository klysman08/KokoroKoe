CREATE TABLE session_index_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    generation INTEGER NOT NULL CHECK (generation >= 1),
    workspace_key TEXT NOT NULL CHECK (length(workspace_key) = 64)
);

CREATE TABLE session_index_entries (
    session_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    project_folder TEXT NOT NULL,
    session_folder TEXT NOT NULL,
    sort_rank INTEGER NOT NULL UNIQUE CHECK (sort_rank >= 0),
    session_json TEXT NOT NULL,
    snapshot_sha256 BLOB NOT NULL CHECK (length(snapshot_sha256) = 32)
);
