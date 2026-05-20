-- Schema v0.4.0
-- Adds desktop as a first-class capture source.

PRAGMA foreign_keys=OFF;
PRAGMA legacy_alter_table=ON;

BEGIN;

DROP TRIGGER IF EXISTS prompts_ai;
DROP TRIGGER IF EXISTS prompts_ad;
DROP TRIGGER IF EXISTS prompts_au;
DROP TABLE IF EXISTS prompts_fts;

ALTER TABLE prompts RENAME TO prompts_old;

CREATE TABLE prompts (
    id                  TEXT PRIMARY KEY,
    text                TEXT NOT NULL,
    text_hash           TEXT NOT NULL,
    source              TEXT NOT NULL CHECK(source IN ('browser','desktop','ide','cli','mcp')),
    source_app          TEXT NOT NULL,
    project_id          TEXT REFERENCES projects(id) ON DELETE SET NULL,
    conversation_id     TEXT,
    conversation_title  TEXT,
    use_count           INTEGER NOT NULL DEFAULT 1 CHECK(use_count >= 1),
    favorite            INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1)),
    metadata            TEXT NOT NULL DEFAULT '{}',
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    last_used_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

INSERT INTO prompts (
    id, text, text_hash, source, source_app, project_id, conversation_id, conversation_title,
    use_count, favorite, metadata, created_at, last_used_at
)
SELECT
    id, text, text_hash, source, source_app, project_id, conversation_id, conversation_title,
    use_count, favorite, metadata, created_at, last_used_at
FROM prompts_old;

DROP TABLE prompts_old;

CREATE INDEX idx_prompts_created_at         ON prompts(created_at DESC);
CREATE INDEX idx_prompts_project_id         ON prompts(project_id);
CREATE INDEX idx_prompts_source             ON prompts(source);
CREATE INDEX idx_prompts_source_app         ON prompts(source_app);
CREATE INDEX idx_prompts_favorite           ON prompts(favorite) WHERE favorite = 1;
CREATE INDEX idx_prompts_use_count          ON prompts(use_count DESC);
CREATE INDEX idx_prompts_last_used_at       ON prompts(last_used_at DESC);
CREATE INDEX idx_prompts_text_hash_recent   ON prompts(text_hash, source, source_app, created_at);
CREATE INDEX idx_prompts_conversation_id    ON prompts(conversation_id);

CREATE VIRTUAL TABLE prompts_fts USING fts5(
    text,
    content='prompts',
    content_rowid='rowid',
    tokenize='porter unicode61 remove_diacritics 2'
);

INSERT INTO prompts_fts(rowid, text)
SELECT rowid, text FROM prompts;

CREATE TRIGGER prompts_ai AFTER INSERT ON prompts BEGIN
    INSERT INTO prompts_fts(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TRIGGER prompts_ad AFTER DELETE ON prompts BEGIN
    INSERT INTO prompts_fts(prompts_fts, rowid, text) VALUES('delete', old.rowid, old.text);
END;

CREATE TRIGGER prompts_au AFTER UPDATE OF text ON prompts BEGIN
    INSERT INTO prompts_fts(prompts_fts, rowid, text) VALUES('delete', old.rowid, old.text);
    INSERT INTO prompts_fts(rowid, text) VALUES (new.rowid, new.text);
END;

COMMIT;

PRAGMA legacy_alter_table=OFF;
PRAGMA foreign_keys=ON;
