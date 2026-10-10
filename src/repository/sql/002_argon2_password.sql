-- RP-01: Argon2id support, clear historical plaintext initial passwords, drop the column.
ALTER TABLE rp_users ADD COLUMN password_algo TEXT NOT NULL DEFAULT 'sha1';
ALTER TABLE rp_users ADD COLUMN must_change_password INTEGER NOT NULL DEFAULT 0;
UPDATE rp_users
   SET must_change_password = CASE
         WHEN length(trim(COALESCE(initial_password, ''))) > 0 THEN 1
         ELSE 0
       END;
ALTER TABLE rp_users DROP COLUMN initial_password;
