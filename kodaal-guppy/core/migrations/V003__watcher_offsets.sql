-- Schema v0.3.0
-- Tracks file capture offsets for CLI and IDE watcher surfaces.

BEGIN;

CREATE TABLE watcher_offsets (
    source_app      TEXT NOT NULL,
    path            TEXT NOT NULL,
    offset_bytes    INTEGER NOT NULL DEFAULT 0 CHECK(offset_bytes >= 0),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (source_app, path)
);

COMMIT;
