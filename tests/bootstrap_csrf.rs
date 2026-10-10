//! RP-10 bind policy is unit-tested; CSRF + must_change_password HTTP checks.
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

const TEST_SIGNING_KEY: &str = "bootstrap-csrf-test-signing-key-32byt!";

fn test_keys() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("RP_SECRET_KEY", TEST_SIGNING_KEY);
        std::env::set_var(
            "RP_SESSION_KEY",
            "bootstrap-csrf-test-session-key-at-least-64-bytes-and-independent",
        );
        std::env::set_var(
            "RP_MASTER_KEY",
            "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f91",
        );
    });
}

fn pool_user(must_change: bool) -> (NamedTempFile, DBPool, String) {
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
    let flag = if must_change { 1 } else { 0 };
    pool.get()
        .unwrap()
        .batch_execute(&format!(
            "INSERT INTO rp_users (
            username, password, salt, authority, error_count, status,
            created_at, updated_at, password_algo, must_change_password, token_version
        ) VALUES (
            'u', 'hash', '', 'user', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', {flag}, 0
        );
        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT id, 3 FROM rp_users WHERE username = 'u';"
        ))
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
async fn must_change_password_blocks_privileged_routes_but_allows_me() {
    let (_f, pool, bearer) = pool_user(true);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .wrap(api::csrf::CsrfProtect)
            .service(api::route::v1()),
    )
    .await;

    let me = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, me).await.status(),
        StatusCode::OK
    );

    let blocked = actix_test::TestRequest::get()
        .uri("/api/v1/user/menus")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, blocked).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[actix_web::test]
async fn cookie_mutating_request_without_origin_is_forbidden() {
    let (_f, pool, bearer) = pool_user(false);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .wrap(api::csrf::CsrfProtect)
            .service(api::route::v1()),
    )
    .await;

    // Cookie present + POST without Origin => CSRF reject.
    let request = actix_test::TestRequest::post()
        .uri("/api/v1/login/logout")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .insert_header(("Cookie", "token=session-cookie-value"))
        .insert_header(("Host", "127.0.0.1:8080"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::FORBIDDEN
    );

    // Matching Origin allows the cookie-authenticated mutate through CSRF.
    let request = actix_test::TestRequest::post()
        .uri("/api/v1/login/logout")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .insert_header(("Cookie", "token=session-cookie-value"))
        .insert_header(("Host", "127.0.0.1:8080"))
        .insert_header(("Origin", "http://127.0.0.1:8080"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK
    );
}
