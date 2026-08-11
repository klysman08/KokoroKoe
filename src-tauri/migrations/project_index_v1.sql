CREATE TABLE project_index_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    generation INTEGER NOT NULL CHECK (generation >= 1)
);

CREATE TABLE project_index_entries (
    project_id TEXT PRIMARY KEY,
    folder_name TEXT NOT NULL UNIQUE,
    sort_rank INTEGER NOT NULL UNIQUE CHECK (sort_rank >= 0),
    project_json TEXT NOT NULL,
    snapshot_sha256 BLOB NOT NULL CHECK (length(snapshot_sha256) = 32)
);
