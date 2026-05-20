-- Schema v0.2.0
-- Adds artifact tracking.

BEGIN;

CREATE TABLE artifacts (
    id                  TEXT PRIMARY KEY,
    prompt_id           TEXT NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    filename            TEXT NOT NULL,
    original_path       TEXT NOT NULL,
    project_id          TEXT REFERENCES projects(id) ON DELETE SET NULL,
    storage_mode        TEXT NOT NULL CHECK(storage_mode IN ('reference','snapshot')),
    snapshot_blob       BLOB,
    snapshot_size       INTEGER,
    mime_type           TEXT,
    detection_mode      TEXT NOT NULL CHECK(detection_mode IN ('auto_watch','manual')),
    is_broken           INTEGER NOT NULL DEFAULT 0 CHECK(is_broken IN (0,1)),
    last_verified_at    TEXT,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX idx_artifacts_prompt_id     ON artifacts(prompt_id);
CREATE INDEX idx_artifacts_project_id    ON artifacts(project_id);
CREATE INDEX idx_artifacts_is_broken     ON artifacts(is_broken) WHERE is_broken = 1;
CREATE INDEX idx_artifacts_storage_mode  ON artifacts(storage_mode);

COMMIT;
