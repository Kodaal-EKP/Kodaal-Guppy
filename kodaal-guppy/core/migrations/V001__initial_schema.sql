-- Schema v0.1.0
-- Created 2026-05-05
-- Initial schema: prompts, projects, tags, prompt_tags, FTS5 index.

BEGIN;

CREATE TABLE projects (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    path            TEXT UNIQUE,
    color           TEXT NOT NULL DEFAULT '#3b82f6',
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX idx_projects_path ON projects(path);

CREATE TABLE prompts (
    id                  TEXT PRIMARY KEY,
    text                TEXT NOT NULL,
    text_hash           TEXT NOT NULL,
    source              TEXT NOT NULL CHECK(source IN ('browser','ide','cli','mcp')),
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

CREATE INDEX idx_prompts_created_at         ON prompts(created_at DESC);
CREATE INDEX idx_prompts_project_id         ON prompts(project_id);
CREATE INDEX idx_prompts_source             ON prompts(source);
CREATE INDEX idx_prompts_source_app         ON prompts(source_app);
CREATE INDEX idx_prompts_favorite           ON prompts(favorite) WHERE favorite = 1;
CREATE INDEX idx_prompts_use_count          ON prompts(use_count DESC);
CREATE INDEX idx_prompts_last_used_at       ON prompts(last_used_at DESC);
CREATE INDEX idx_prompts_text_hash_recent   ON prompts(text_hash, created_at);
CREATE INDEX idx_prompts_conversation_id    ON prompts(conversation_id);

CREATE TABLE tags (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE prompt_tags (
    prompt_id   TEXT NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    tag_id      TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (prompt_id, tag_id)
);

CREATE INDEX idx_prompt_tags_tag_id ON prompt_tags(tag_id);

CREATE VIRTUAL TABLE prompts_fts USING fts5(
    text,
    content='prompts',
    content_rowid='rowid',
    tokenize='porter unicode61 remove_diacritics 2'
);

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
