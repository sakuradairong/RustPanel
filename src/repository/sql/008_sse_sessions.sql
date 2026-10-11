-- Existing tickets have no trustworthy session identity; revoke them on upgrade.
DELETE FROM sse_tickets;
ALTER TABLE sse_tickets ADD COLUMN session_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE sse_tickets ADD COLUMN token_version INTEGER NOT NULL DEFAULT -1;
ALTER TABLE sse_tickets ADD COLUMN access_expires_at_unix BIGINT NOT NULL DEFAULT 0;
