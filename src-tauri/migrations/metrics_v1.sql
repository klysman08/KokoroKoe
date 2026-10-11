CREATE TABLE metrics_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0)
) STRICT;

CREATE TABLE usage_entries (
    request_id TEXT PRIMARY KEY NOT NULL CHECK (length(request_id) = 36),
    project_id TEXT NOT NULL CHECK (length(project_id) = 36),
    session_id TEXT NOT NULL CHECK (length(session_id) = 36),
    purpose TEXT NOT NULL CHECK (purpose IN ('insight', 'summary', 'manual_question')),
    model_id TEXT NOT NULL CHECK (length(model_id) BETWEEN 1 AND 256),
    input_tokens INTEGER NOT NULL CHECK (input_tokens >= 0),
    output_tokens INTEGER NOT NULL CHECK (output_tokens >= 0),
    actual_cost_picousd INTEGER NOT NULL CHECK (actual_cost_picousd >= 0),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE INDEX usage_entries_created ON usage_entries (created_at_ms);

CREATE TABLE insight_batches (
    request_id TEXT PRIMARY KEY NOT NULL CHECK (length(request_id) = 36),
    project_id TEXT NOT NULL CHECK (length(project_id) = 36),
    session_id TEXT NOT NULL CHECK (length(session_id) = 36),
    insight_count INTEGER NOT NULL CHECK (insight_count BETWEEN 0 AND 64),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE INDEX insight_batches_created ON insight_batches (created_at_ms);

CREATE TABLE recent_errors (
    row_id INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT NOT NULL CHECK (length(code) BETWEEN 1 AND 128),
    user_message TEXT NOT NULL CHECK (length(user_message) BETWEEN 1 AND 512),
    severity TEXT NOT NULL CHECK (severity IN ('warning', 'error', 'critical')),
    retryable INTEGER NOT NULL CHECK (retryable IN (0, 1)),
    correlation_id TEXT NOT NULL CHECK (length(correlation_id) = 36),
    occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0)
) STRICT;

CREATE INDEX recent_errors_occurred ON recent_errors (occurred_at_ms);
