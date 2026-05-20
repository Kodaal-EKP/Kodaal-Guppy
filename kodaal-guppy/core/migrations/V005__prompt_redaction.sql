-- Schema v0.5.0
-- Adds prompt-level sensitive-content redaction metadata.

BEGIN;

ALTER TABLE prompts
    ADD COLUMN redacted INTEGER NOT NULL DEFAULT 0 CHECK(redacted IN (0,1));

ALTER TABLE prompts
    ADD COLUMN redaction_reason TEXT;

CREATE INDEX idx_prompts_redacted ON prompts(redacted) WHERE redacted = 1;

COMMIT;
