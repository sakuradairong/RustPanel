//! Privileged runner HTTP regressions: shell-injection rule args are rejected.
use actix_web::{http::StatusCode, test as actix_test, web, App};
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

const TEST_SIGNING_KEY: &str = "privileged-runner-test-signing-key-32b!!";

fn test_keys() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("RP_SECRET_KEY", TEST_SIGNING_KEY);
        std::env::set_var(
            "RP_SESSION_KEY",
            "privileged-runner-test-session-key-at-least-64-bytes-independent",
        );
        std::env::set_var(
            "RP_MASTER_KEY",
            "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f91",
        );
    });
}

fn pool_with_admin() -> (NamedTempFile, DBPool, String) {
    test_keys();
    let file = NamedTempFile::new().unwrap();
    let pool = DBPool::builder()
        .max_size(2)
        .connection_timeout(Duration::from_millis(200))
        .build(ConnectionManager::<SqliteConnection>::new(
            file.path().to_str().unwrap(),
        ))
        .unwrap();
    migrations::migrate(&pool).unwrap();
    pool.get()
        .unwrap()
        .batch_execute(
            "INSERT INTO rp_users (
            username, password, salt, authority, error_count, status,
            created_at, updated_at, password_algo, must_change_password, token_version
        ) VALUES (
            'admin', 'hash', '', 'admin', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', 0, 0
        );
        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT id, 1 FROM rp_users WHERE username = 'admin';",
        )
        .unwrap();
    let mut conn = pool.get().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let session = sessions::create_session(&mut conn, 1, 3600, None, None).unwrap();
    drop(conn);
    let token = generate_jwt(1, &session.jti, 0, now + 3600).unwrap();
    (file, pool, token)
}

#[actix_web::test]
async fn firewall_rule_injection_is_forbidden_without_spawning_shell() {
    let (_file, pool, bearer) = pool_with_admin();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;

    let payloads = [
        serde_json::json!({"chain":"INPUT","rule":"-p tcp; id"}),
        serde_json::json!({"chain":"INPUT","rule":"-p tcp $(reboot)"}),
        serde_json::json!({"chain":"INPUT","rule":"-p tcp `id`"}),
        serde_json::json!({"chain":"INPUT;id","rule":"-p tcp -j ACCEPT"}),
    ];

    for body in payloads {
        let request = actix_test::TestRequest::post()
            .uri("/api/v1/firewall/rules")
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .set_json(&body)
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "{body}"
        );
    }
}

#[actix_web::test]
async fn service_unit_injection_is_forbidden() {
    let (_file, pool, bearer) = pool_with_admin();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;

    let request = actix_test::TestRequest::post()
        .uri("/api/v1/service/nginx.service;reboot/start")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    // Path segment may be rejected by routing or by runner; either is fail-closed.
    let status = actix_test::call_service(&app, request).await.status();
    assert!(
        status == StatusCode::FORBIDDEN
            || status == StatusCode::NOT_FOUND
            || status == StatusCode::BAD_REQUEST,
        "{status}"
    );

    let request = actix_test::TestRequest::post()
        .uri("/api/v1/service/evil%3Breboot/start")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::FORBIDDEN
    );
}
