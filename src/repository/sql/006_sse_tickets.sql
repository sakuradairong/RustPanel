-- RP-05: one-time short-lived SSE tickets (not long-lived JWTs in URLs).
CREATE TABLE IF NOT EXISTS sse_tickets (
    jti TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    stream TEXT NOT NULL,
    expires_at_unix INTEGER NOT NULL,
    consumed_at_unix INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY(user_id) REFERENCES rp_users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sse_tickets_expiry ON sse_tickets(expires_at_unix);
CREATE INDEX IF NOT EXISTS idx_sse_tickets_user ON sse_tickets(user_id);
