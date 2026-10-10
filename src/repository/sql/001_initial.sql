CREATE TABLE IF NOT EXISTS rp_users (
    id INTEGER PRIMARY KEY,
    username TEXT(16) UNIQUE NOT NULL,
    password TEXT(40) NOT NULL,
    salt TEXT(16) NOT NULL,
    initial_password TEXT(18) NOT NULL,
    authority TEXT(20) NOT NULL,
    error_count INTEGER(5) DEFAULT 0,
    status INTEGER(5) DEFAULT 0 NOT NULL,
    api_key TEXT(40) NULL,
    created_at datetime NOT NULL,
    updated_at datetime NOT NULL,
    deleted_at datetime
);
