CREATE TABLE window_state (
  window_label TEXT PRIMARY KEY NOT NULL,
  state_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;
