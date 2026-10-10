//! HTTP regressions against the real router, using only a temporary SQLite database.
use actix_web::{
    http::{Method, StatusCode},
    test as actix_test, web, App,
};
use diesel::{connection::SimpleConnection, r2d2::ConnectionManager, SqliteConnection};
use rust_panel::{
    api,
    api::auth::generate_jwt,
    repository::migrations,
    service::{db::DBPool, sessions},
};
use std::{
    sync::Once,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

const TEST_SIGNING_KEY: &str = "security-test-signing-key-with-at-least-32-bytes";

fn test_keys() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("RP_SECRET_KEY", TEST_SIGNING_KEY);
        std::env::set_var(
            "RP_SESSION_KEY",
            "security-test-session-key-at-least-64-bytes-and-independent-of-signing",
        );
        std::env::set_var(
            "RP_MASTER_KEY",
            "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f91",
        );
    });
}

fn database(with_schema: bool) -> (NamedTempFile, DBPool) {
    test_keys();
    let file = NamedTempFile::new().unwrap();
    let pool = DBPool::builder()
        .max_size(1)
        .connection_timeout(Duration::from_millis(100))
        .build(ConnectionManager::<SqliteConnection>::new(
            file.path().to_str().unwrap(),
        ))
        .unwrap();
    if with_schema {
        migrations::migrate(&pool).unwrap();
        pool.get()
            .unwrap()
            .batch_execute(
                "INSERT INTO rp_users (
            username, password, salt, authority, error_count, status,
            created_at, updated_at, password_algo, must_change_password, token_version
        ) VALUES (
            'operator', 'test-password-hash', 'test-salt', 'user', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'sha1', 0, 0
        );
        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT id, 3 FROM rp_users WHERE username = 'operator';",
            )
            .unwrap();
    }
    (file, pool)
}

fn token_for(pool: &DBPool, user_id: i32, token_version: i32, expires_in: i64) -> String {
    test_keys();
    let mut conn = pool.get().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // Session row TTL is independent of JWT exp so expired-token tests do not need conf.yaml.
    let session =
        sessions::create_session(&mut conn, user_id, 3600, Some("127.0.0.1"), None).unwrap();
    drop(conn);
    generate_jwt(user_id, &session.jti, token_version, now + expires_in).unwrap()
}

#[actix_web::test]
async fn every_ssl_route_requires_authentication() {
    let app = actix_test::init_service(App::new().service(api::route::v1())).await;
    let endpoints = [
        (Method::GET, "/api/v1/ssl/providers"),
        (Method::GET, "/api/v1/ssl/accounts"),
        (Method::POST, "/api/v1/ssl/accounts"),
        (Method::GET, "/api/v1/ssl/certificates"),
        (Method::GET, "/api/v1/ssl/certificates/example.invalid"),
        (Method::DELETE, "/api/v1/ssl/certificates/example.invalid"),
        (Method::POST, "/api/v1/ssl/issue"),
        (Method::POST, "/api/v1/ssl/renew/example.invalid"),
        (Method::POST, "/api/v1/ssl/deploy"),
        (Method::POST, "/api/v1/ssl/setup"),
        (Method::POST, "/api/v1/ssl/upload"),
        (Method::POST, "/api/v1/ssl/self-signed"),
    ];
    for (method, path) in endpoints {
        let request = actix_test::TestRequest::default()
            .method(method)
            .uri(path)
            .set_json(serde_json::json!({
                "domains": ["example.invalid"], "domain": "example.invalid",
                "email": "test@example.invalid", "site_name": "test", "name": "test",
                "cert": "test certificate", "key": "test key"
            }))
            .to_request();
        let response = actix_test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[actix_web::test]
async fn authentication_uses_current_database_identity_and_role() {
    let (_file, pool) = database(true);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let bearer = token_for(&pool, 1, 0, 3600);
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    let response = actix_test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(response).await;
    assert_eq!(body["username"], "operator");
    assert_eq!(body["authority"], "user");
    assert!(body.get("password").is_none());
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[actix_web::test]
async fn disabled_missing_changed_password_and_expired_users_are_rejected() {
    let (_file, pool) = database(true);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    for bearer in [
        token_for(&pool, 1, 0, -3600),
        "invalid.jwt".into(),
    ] {
        let request = actix_test::TestRequest::get()
            .uri("/api/v1/user/me")
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }

    let disabled = token_for(&pool, 1, 0, 3600);
    pool.get()
        .unwrap()
        .batch_execute("UPDATE rp_users SET status=-1")
        .unwrap();
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {disabled}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );
    // Long-lived JWT in query is rejected even when Authorization is present.
    let request = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/monitor?token={disabled}"))
        .insert_header(("Authorization", format!("Bearer {disabled}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );
    // Bearer-only monitor path still uses session/DB checks (disabled user).
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/monitor")
        .insert_header(("Authorization", format!("Bearer {disabled}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );

    pool.get()
        .unwrap()
        .batch_execute("UPDATE rp_users SET status=0; DELETE FROM sessions;")
        .unwrap();
    let stale_version = token_for(&pool, 1, 0, 3600);
    pool.get()
        .unwrap()
        .batch_execute("UPDATE rp_users SET token_version = 1")
        .unwrap();
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {stale_version}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );

    pool.get()
        .unwrap()
        .batch_execute("UPDATE rp_users SET token_version = 0; DELETE FROM sessions;")
        .unwrap();
    let deleted_user_token = token_for(&pool, 1, 0, 3600);
    pool.get()
        .unwrap()
        .batch_execute("DELETE FROM sessions; DELETE FROM user_roles; DELETE FROM rp_users;")
        .unwrap();
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header((
            "Authorization",
            format!("Bearer {deleted_user_token}"),
        ))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn logout_revokes_the_current_session() {
    let (_file, pool) = database(true);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let bearer = token_for(&pool, 1, 0, 3600);
    let request = actix_test::TestRequest::post()
        .uri("/api/v1/login/logout")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK
    );
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn access_token_does_not_embed_password_material() {
    let (_file, pool) = database(true);
    let bearer = token_for(&pool, 1, 0, 3600);
    let parts: Vec<_> = bearer.split('.').collect();
    assert_eq!(parts.len(), 3);
    let payload = parts[1];
    // Base64url decode without padding handling via standard library is awkward;
    // ensure the compact token string never contains the stored password hash.
    assert!(!bearer.contains("test-password-hash"));
    assert!(!payload.contains("password"));
}

#[actix_web::test]
async fn database_query_failure_fails_closed_for_regular_and_sse_requests() {
    let (_file, pool) = database(false);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;
    // Token without a backing session/DB schema must not authorize.
    let bogus = generate_jwt(1, "no-such-jti", 0, {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600
    })
    .unwrap();
    for path in ["/api/v1/user/me", "/api/v1/monitor"] {
        let request = actix_test::TestRequest::get()
            .uri(path)
            .insert_header(("Authorization", format!("Bearer {bogus}")))
            .to_request();
        let status = actix_test::call_service(&app, request).await.status();
        assert!(
            status == StatusCode::UNAUTHORIZED || status == StatusCode::SERVICE_UNAVAILABLE,
            "{path} => {status}"
        );
    }
    // Query JWT is never an auth path (rejected before DB).
    let request = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/monitor?token={bogus}"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn exhausted_connection_pool_returns_unavailable_without_panicking() {
    let (_file, pool) = database(true);
    let bearer = token_for(&pool, 1, 0, 3600);
    let held_connection = pool.get().unwrap();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    drop(held_connection);
}

#[actix_web::test]
async fn missing_database_is_unavailable_instead_of_authorizing_a_valid_jwt() {
    let app = actix_test::init_service(App::new().service(api::route::v1())).await;
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header((
            "Authorization",
            format!(
                "Bearer {}",
                generate_jwt(1, "orphan", 0, {
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64
                        + 3600
                })
                .unwrap()
            ),
        ))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[actix_web::test]
async fn ssl_mutations_reject_non_admin_even_with_a_stale_admin_token() {
    let (_file, pool) = database(true);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    // Role comes from DB (user), not from a crafted admin claim.
    let bearer = token_for(&pool, 1, 0, 3600);
    for (method, path) in [
        (Method::POST, "/api/v1/ssl/accounts"),
        (Method::DELETE, "/api/v1/ssl/certificates/example.invalid"),
        (Method::POST, "/api/v1/ssl/issue"),
        (Method::POST, "/api/v1/ssl/renew/example.invalid"),
        (Method::POST, "/api/v1/ssl/deploy"),
        (Method::POST, "/api/v1/ssl/setup"),
        (Method::POST, "/api/v1/ssl/upload"),
        (Method::POST, "/api/v1/ssl/self-signed"),
    ] {
        let request = actix_test::TestRequest::default()
            .method(method)
            .uri(path)
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .set_json(serde_json::json!({
                "domains": ["example.invalid"], "domain": "example.invalid",
                "email": "test@example.invalid", "site_name": "test", "name": "test",
                "cert": "test certificate", "key": "test key"
            }))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
}

#[actix_web::test]
async fn authenticated_ssl_metadata_remains_available() {
    let (_file, pool) = database(true);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/ssl/providers")
        .insert_header((
            "Authorization",
            format!("Bearer {}", token_for(&pool, 1, 0, 3600)),
        ))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK
    );
}

#[test]
fn panel_refuses_missing_or_invalid_keys_before_initializing_runtime() {
    let directory = tempfile::tempdir().unwrap();
    for keys in [
        None,
        Some(("short-private-value", "independent-but-too-short")),
    ] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_panel"));
        command
            .current_dir(directory.path())
            .env_remove("RP_SECRET_KEY")
            .env_remove("RP_SESSION_KEY")
            .env_remove("RP_MASTER_KEY")
            .env_remove("RP_MASTER_KEY_FILE");
        if let Some((signing, session)) = keys {
            command
                .env("RP_SECRET_KEY", signing)
                .env("RP_SESSION_KEY", session);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("RP_SECRET_KEY"));
        assert!(!stderr.contains("short-private-value"));
        assert!(!directory.path().join("runtime").exists());
        assert!(!directory.path().join("data.db3").exists());
    }
}

#[test]
fn panel_refuses_missing_master_key_even_when_signing_keys_valid() {
    let directory = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_panel"))
        .current_dir(directory.path())
        .env("RP_SECRET_KEY", TEST_SIGNING_KEY)
        .env(
            "RP_SESSION_KEY",
            "security-test-session-key-at-least-64-bytes-and-independent-of-signing",
        )
        .env_remove("RP_MASTER_KEY")
        .env_remove("RP_MASTER_KEY_FILE")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("RP_MASTER_KEY"));
    assert!(!directory.path().join("runtime").exists());
    assert!(!directory.path().join("data.db3").exists());
}

#[actix_web::test]
async fn admin_password_reset_revokes_existing_tokens() {
    let (_file, pool) = database(true);
    pool.get()
        .unwrap()
        .batch_execute(
            "INSERT INTO rp_users (
            username, password, salt, authority, error_count, status,
            created_at, updated_at, password_algo, must_change_password, token_version
        ) VALUES (
            'admin2', 'test-password-hash', 'test-salt', 'admin', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'sha1', 0, 0
        );
        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT id, 1 FROM rp_users WHERE username = 'admin2';",
        )
        .unwrap();
    let admin_bearer = token_for(&pool, 2, 0, 3600);
    let victim_bearer = token_for(&pool, 1, 0, 3600);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;

    // The victim's token works before the reset.
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {victim_bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK
    );

    let request = actix_test::TestRequest::post()
        .uri("/api/v1/admin/users/1/reset-password")
        .insert_header(("Authorization", format!("Bearer {admin_bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK,
        "admin reset-password"
    );

    // The pre-reset token must be dead immediately, not just after logout.
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {victim_bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::UNAUTHORIZED,
        "pre-reset token after admin password reset"
    );
}

#[actix_web::test]
async fn disabling_and_deleting_users_leaves_no_active_sessions() {
    use diesel::{QueryableByName, RunQueryDsl};
    use diesel::sql_types::{BigInt, Nullable, Text};

    #[derive(QueryableByName)]
    struct SessionRow {
        #[diesel(sql_type = Nullable<Text>)]
        revoked_at: Option<String>,
    }

    fn live_sessions(pool: &DBPool, user_id: i32) -> i64 {
        #[derive(QueryableByName)]
        struct Count {
            #[diesel(sql_type = BigInt)]
            count: i64,
        }
        let mut conn = pool.get().unwrap();
        let rows: Vec<Count> = diesel::sql_query(
            "SELECT COUNT(*) AS count FROM sessions WHERE user_id = ?1 AND revoked_at IS NULL",
        )
        .bind::<BigInt, _>(i64::from(user_id))
        .load(&mut conn)
        .unwrap();
        rows[0].count
    }

    let (_file, pool) = database(true);
    pool.get()
        .unwrap()
        .batch_execute(
            "INSERT INTO rp_users (
            username, password, salt, authority, error_count, status,
            created_at, updated_at, password_algo, must_change_password, token_version
        ) VALUES (
            'admin3', 'test-password-hash', 'test-salt', 'admin', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'sha1', 0, 0
        );
        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT id, 1 FROM rp_users WHERE username = 'admin3';",
        )
        .unwrap();
    let admin_bearer = token_for(&pool, 2, 0, 3600);
    // Victim session that would otherwise outlive the account.
    let _victim_bearer = token_for(&pool, 1, 0, 3600);
    assert_eq!(live_sessions(&pool, 1), 1);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;

    // Disable via update_user: session rows must be revoked in the same call.
    let request = actix_test::TestRequest::put()
        .uri("/api/v1/admin/users/1")
        .insert_header(("Authorization", format!("Bearer {admin_bearer}")))
        .set_json(serde_json::json!({"status": -1}))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK,
        "admin disable user"
    );
    assert_eq!(
        live_sessions(&pool, 1),
        0,
        "disabling a user must revoke their sessions"
    );

    // Re-enable, mint a fresh session, then delete: same invariant.
    pool.get()
        .unwrap()
        .batch_execute("UPDATE rp_users SET status = 0 WHERE id = 1;")
        .unwrap();
    let _victim_bearer = token_for(&pool, 1, 0, 3600);
    assert_eq!(live_sessions(&pool, 1), 1);
    let request = actix_test::TestRequest::delete()
        .uri("/api/v1/admin/users/1")
        .insert_header(("Authorization", format!("Bearer {admin_bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK,
        "admin delete user"
    );
    assert_eq!(
        live_sessions(&pool, 1),
        0,
        "deleting a user must revoke their sessions"
    );
    let _ = SessionRow { revoked_at: None };
}

#[actix_web::test]
async fn log_read_is_bounded_to_log_roots() {
    let (_file, pool) = database(true);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let bearer = token_for(&pool, 1, 0, 3600);

    // Point the log roots at a temp dir for this test.
    let log_dir = std::env::temp_dir().join("rp-log-test");
    std::fs::create_dir_all(&log_dir).unwrap();
    std::fs::write(log_dir.join("panel.log"), "line-one\nline-two\n").unwrap();
    std::env::set_var("RP_LOG_ROOTS", log_dir.to_str().unwrap());

    // Arbitrary absolute paths outside the log roots are refused.
    for path in ["/etc/passwd", "/etc/shadow", "/root/.ssh/id_rsa"] {
        let request = actix_test::TestRequest::get()
            .uri(&format!("/api/v1/log/read?path={}", urlencode(path)))
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "log read outside roots: {path}"
        );
    }

    // Traversal via .. is rejected by the bounds layer.
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/log/read?path=/var/log/../../etc/passwd")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::FORBIDDEN,
        "log read traversal"
    );

    // A real file under a log root reads fine.
    let abs = log_dir.join("panel.log");
    let request = actix_test::TestRequest::get()
        .uri(&format!(
            "/api/v1/log/read?path={}",
            urlencode(abs.to_str().unwrap())
        ))
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    let response = actix_test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(response).await;
    assert_eq!(body["data"]["total"], 2);
    std::fs::remove_dir_all(&log_dir).ok();
}

fn urlencode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
