-- RP-08: envelope-encrypted secrets (DEK wrapped by KEK; no plaintext at rest).
CREATE TABLE IF NOT EXISTS secret_keys (
    key_id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL,
    retired_at TEXT
);

CREATE TABLE IF NOT EXISTS secrets (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name TEXT NOT NULL UNIQUE,
    purpose TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    wrapped_dek BLOB NOT NULL,
    wrap_nonce BLOB NOT NULL,
    key_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(key_id) REFERENCES secret_keys(key_id)
);

CREATE INDEX IF NOT EXISTS idx_secrets_purpose ON secrets(purpose);
CREATE INDEX IF NOT EXISTS idx_secrets_key_id ON secrets(key_id);
