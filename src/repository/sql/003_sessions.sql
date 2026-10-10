-- RP-23: revocable sessions and per-user token_version.
ALTER TABLE rp_users ADD COLUMN token_version INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    jti_hash TEXT NOT NULL UNIQUE,
    user_id INTEGER NOT NULL,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    ua_hash TEXT,
    ip TEXT,
    FOREIGN KEY (user_id) REFERENCES rp_users(id)
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
