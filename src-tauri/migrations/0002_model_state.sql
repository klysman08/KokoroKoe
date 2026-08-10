CREATE TABLE model_download_jobs (
  request_id TEXT PRIMARY KEY NOT NULL,
  model_id TEXT NOT NULL,
  job_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX model_download_jobs_model_updated
  ON model_download_jobs (model_id, updated_at DESC);

CREATE TABLE model_installations (
  model_id TEXT PRIMARY KEY NOT NULL,
  installed_at TEXT NOT NULL
) STRICT;
