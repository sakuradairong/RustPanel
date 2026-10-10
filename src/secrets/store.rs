//! Persist sealed secrets in SQLite; never write plaintext columns.
use diesel::{
    sql_query,
    sql_types::{Blob, Integer, Text},
    QueryableByName, RunQueryDsl, SqliteConnection,
};

use super::{
    envelope::{self, SealedSecret},
    kek::MasterKey,
};

#[derive(Debug, Clone)]
pub struct SecretRecord {
    pub id: i32,
    pub name: String,
    pub purpose: String,
    pub sealed: SealedSecret,
}

#[derive(QueryableByName)]
struct SecretRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    purpose: String,
    #[diesel(sql_type = Blob)]
    ciphertext: Vec<u8>,
    #[diesel(sql_type = Blob)]
    nonce: Vec<u8>,
    #[diesel(sql_type = Blob)]
    wrapped_dek: Vec<u8>,
    #[diesel(sql_type = Blob)]
    wrap_nonce: Vec<u8>,
    #[diesel(sql_type = Text)]
    key_id: String,
}

fn now_utc() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string()
}

/// Ensure `key_id` exists in `secret_keys` (idempotent).
pub fn register_key(conn: &mut SqliteConnection, key_id: &str) -> Result<(), String> {
    sql_query(
        "INSERT OR IGNORE INTO secret_keys (key_id, created_at, retired_at)
         VALUES (?1, ?2, NULL)",
    )
    .bind::<Text, _>(key_id)
    .bind::<Text, _>(now_utc())
    .execute(conn)
    .map_err(|err| format!("Failed to register secret key: {err}"))?;
    Ok(())
}

/// Encrypt and upsert a named secret.
pub fn put_secret(
    conn: &mut SqliteConnection,
    kek: &MasterKey,
    purpose: &str,
    name: &str,
    plaintext: &[u8],
) -> Result<i32, String> {
    if purpose.trim().is_empty() || name.trim().is_empty() {
        return Err("Secret purpose and name are required".into());
    }
    register_key(conn, kek.key_id())?;
    let sealed = envelope::encrypt(kek, purpose, name, plaintext)?;
    let ts = now_utc();
    sql_query(
        "INSERT INTO secrets (
            name, purpose, ciphertext, nonce, wrapped_dek, wrap_nonce, key_id, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(name) DO UPDATE SET
            purpose = excluded.purpose,
            ciphertext = excluded.ciphertext,
            nonce = excluded.nonce,
            wrapped_dek = excluded.wrapped_dek,
            wrap_nonce = excluded.wrap_nonce,
            key_id = excluded.key_id,
            updated_at = excluded.updated_at",
    )
    .bind::<Text, _>(name)
    .bind::<Text, _>(purpose)
    .bind::<Blob, _>(&sealed.ciphertext)
    .bind::<Blob, _>(&sealed.nonce)
    .bind::<Blob, _>(&sealed.wrapped_dek)
    .bind::<Blob, _>(&sealed.wrap_nonce)
    .bind::<Text, _>(kek.key_id())
    .bind::<Text, _>(&ts)
    .execute(conn)
    .map_err(|err| format!("Failed to store secret: {err}"))?;

    let rows: Vec<SecretRow> = sql_query(
        "SELECT id, name, purpose, ciphertext, nonce, wrapped_dek, wrap_nonce, key_id
         FROM secrets WHERE name = ?1",
    )
    .bind::<Text, _>(name)
    .load(conn)
    .map_err(|err| format!("Failed to read stored secret id: {err}"))?;
    rows.first()
        .map(|r| r.id)
        .ok_or_else(|| "Stored secret missing after upsert".into())
}

/// Load and decrypt a secret by name. Plaintext is returned only to the caller.
pub fn get_secret(
    conn: &mut SqliteConnection,
    kek: &MasterKey,
    name: &str,
) -> Result<Vec<u8>, String> {
    let record = load_record(conn, name)?;
    envelope::decrypt(kek, &record.purpose, &record.name, &record.sealed)
}

pub fn delete_secret(conn: &mut SqliteConnection, name: &str) -> Result<bool, String> {
    let updated = sql_query("DELETE FROM secrets WHERE name = ?1")
        .bind::<Text, _>(name)
        .execute(conn)
        .map_err(|err| format!("Failed to delete secret: {err}"))?;
    Ok(updated > 0)
}

fn load_record(conn: &mut SqliteConnection, name: &str) -> Result<SecretRecord, String> {
    let rows: Vec<SecretRow> = sql_query(
        "SELECT id, name, purpose, ciphertext, nonce, wrapped_dek, wrap_nonce, key_id
         FROM secrets WHERE name = ?1",
    )
    .bind::<Text, _>(name)
    .load(conn)
    .map_err(|err| format!("Failed to load secret: {err}"))?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| format!("Secret '{name}' not found"))?;
    Ok(SecretRecord {
        id: row.id,
        name: row.name.clone(),
        purpose: row.purpose.clone(),
        sealed: SealedSecret {
            ciphertext: row.ciphertext,
            nonce: row.nonce,
            wrapped_dek: row.wrapped_dek,
            wrap_nonce: row.wrap_nonce,
            key_id: row.key_id,
        },
    })
}

/// Re-wrap every secret currently under `old_kek` onto `new_kek`.
pub fn rewrap_all(
    conn: &mut SqliteConnection,
    old_kek: &MasterKey,
    new_kek: &MasterKey,
) -> Result<usize, String> {
    register_key(conn, new_kek.key_id())?;
    let rows: Vec<SecretRow> = sql_query(
        "SELECT id, name, purpose, ciphertext, nonce, wrapped_dek, wrap_nonce, key_id
         FROM secrets WHERE key_id = ?1",
    )
    .bind::<Text, _>(old_kek.key_id())
    .load(conn)
    .map_err(|err| format!("Failed to list secrets for rotation: {err}"))?;

    let mut count = 0usize;
    let ts = now_utc();
    for row in rows {
        let sealed = SealedSecret {
            ciphertext: row.ciphertext.clone(),
            nonce: row.nonce.clone(),
            wrapped_dek: row.wrapped_dek.clone(),
            wrap_nonce: row.wrap_nonce.clone(),
            key_id: row.key_id.clone(),
        };
        let rotated = envelope::rewrap(old_kek, new_kek, &sealed)?;
        sql_query(
            "UPDATE secrets SET wrapped_dek = ?1, wrap_nonce = ?2, key_id = ?3, updated_at = ?4
             WHERE id = ?5",
        )
        .bind::<Blob, _>(&rotated.wrapped_dek)
        .bind::<Blob, _>(&rotated.wrap_nonce)
        .bind::<Text, _>(new_kek.key_id())
        .bind::<Text, _>(&ts)
        .bind::<Integer, _>(row.id)
        .execute(conn)
        .map_err(|err| format!("Failed to update rotated secret: {err}"))?;
        count += 1;
    }

    sql_query("UPDATE secret_keys SET retired_at = ?1 WHERE key_id = ?2 AND key_id != ?3")
        .bind::<Text, _>(&ts)
        .bind::<Text, _>(old_kek.key_id())
        .bind::<Text, _>(new_kek.key_id())
        .execute(conn)
        .map_err(|err| format!("Failed to retire old secret key: {err}"))?;

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::migrations::{self, EXPECTED_SCHEMA_VERSION};
    use crate::service::db::DBPool;
    use diesel::r2d2::ConnectionManager;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    fn sample_kek(tag: u8, id: &str) -> MasterKey {
        let mut bytes = [0x22u8; 32];
        bytes[0] = tag;
        MasterKey::from_bytes(bytes, id)
    }

    fn pool() -> (NamedTempFile, DBPool) {
        let file = NamedTempFile::new().unwrap();
        let pool = DBPool::builder()
            .max_size(1)
            .connection_timeout(Duration::from_millis(500))
            .build(ConnectionManager::<SqliteConnection>::new(
                file.path().to_str().unwrap(),
            ))
            .unwrap();
        assert_eq!(migrations::migrate(&pool).unwrap(), EXPECTED_SCHEMA_VERSION);
        (file, pool)
    }

    #[test]
    fn put_get_round_trip_leaves_no_plaintext_in_db_bytes() {
        let (file, pool) = pool();
        let kek = sample_kek(1, "kek-a");
        let mut conn = pool.get().unwrap();
        let secret = b"offline-search-must-miss-this";
        put_secret(&mut conn, &kek, "db", "mysql.root", secret).unwrap();

        let plain = get_secret(&mut conn, &kek, "mysql.root").unwrap();
        assert_eq!(plain, secret);

        // Offline file scan: plaintext must not appear in the SQLite file.
        drop(conn);
        drop(pool);
        let raw = std::fs::read(file.path()).unwrap();
        assert!(
            !raw.windows(secret.len()).any(|w| w == secret),
            "plaintext leaked into database file"
        );
    }

    #[test]
    fn rewrap_all_allows_decrypt_with_new_kek_only() {
        let (_file, pool) = pool();
        let old = sample_kek(1, "kek-old");
        let new = sample_kek(2, "kek-new");
        let mut conn = pool.get().unwrap();
        put_secret(&mut conn, &old, "s3", "backup.access", b"akid").unwrap();
        put_secret(&mut conn, &old, "s3", "backup.secret", b"secretkey").unwrap();
        assert_eq!(rewrap_all(&mut conn, &old, &new).unwrap(), 2);
        assert_eq!(get_secret(&mut conn, &new, "backup.access").unwrap(), b"akid");
        assert!(get_secret(&mut conn, &old, "backup.access").is_err());
    }
}
