//! HTTP regressions against the real router, using only a temporary SQLite database.
use actix_web::{
    http::{Method, StatusCode},
    test as actix_test, web, App,
};
use diesel::{connection::SimpleConnection, r2d2::ConnectionManager, SqliteConnection};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use rust_panel::{api, service::db::DBPool};
use serde::Serialize;
use std::{
    sync::Once,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

const TEST_SIGNING_KEY: &str = "security-test-signing-key-with-at-least-32-bytes";

fn test_keys() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // This integration-test binary owns its environment; no production process is started.
        std::env::set_var("RP_SECRET_KEY", TEST_SIGNING_KEY);
        std::env::set_var(
            "RP_SESSION_KEY",
            "security-test-session-key-at-least-64-bytes-and-independent-of-signing",
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
        pool.get()
            .unwrap()
            .batch_execute(
                "CREATE TABLE rp_users (
            id INTEGER PRIMARY KEY, username TEXT NOT NULL, password TEXT NOT NULL,
            salt TEXT NOT NULL, initial_password TEXT NOT NULL, authority TEXT NOT NULL,
            error_count INTEGER NOT NULL, status INTEGER NOT NULL,
            created_at TIMESTAMP NOT NULL, updated_at TIMESTAMP NOT NULL
        ); INSERT INTO rp_users VALUES (
            1, 'operator', 'test-password-hash', 'test-salt', '', 'user', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00'
        );",
            )
            .unwrap();
    }
    (file, pool)
}

fn token(password: &str, authority: &str, expires_in: i64) -> String {
    test_keys();
    #[derive(Serialize)]
    struct Claims {
        sub: String,
        exp: usize,
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let sub = serde_json::json!({
        "id": 1, "username": "outdated-name", "password": password,
        "authority": authority, "created_at": "2026-10-10T00:00:00"
    })
    .to_string();
    encode(
        &Header::new(Algorithm::HS256),
        &Claims {
            sub,
            exp: (now + expires_in) as usize,
        },
        &EncodingKey::from_secret(TEST_SIGNING_KEY.as_bytes()),
    )
    .unwrap()
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
    let bearer = token("test-password-hash", "admin", 3600);
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
        token("wrong-hash", "user", 3600),
        token("test-password-hash", "user", -3600),
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
    let bearer = token("test-password-hash", "user", 3600);
    for update in ["UPDATE rp_users SET status=-1", "DELETE FROM rp_users"] {
        pool.get().unwrap().batch_execute(update).unwrap();
        let request = actix_test::TestRequest::get()
            .uri("/api/v1/user/me")
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let request = actix_test::TestRequest::get()
            .uri(&format!("/api/v1/monitor?token={bearer}"))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
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
    let bearer = token("test-password-hash", "user", 3600);
    for path in [
        "/api/v1/user/me".to_owned(),
        format!("/api/v1/monitor?token={bearer}"),
    ] {
        let request = actix_test::TestRequest::get()
            .uri(&path)
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}

#[actix_web::test]
async fn exhausted_connection_pool_returns_unavailable_without_panicking() {
    let (_file, pool) = database(true);
    let held_connection = pool.get().unwrap();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/user/me")
        .insert_header((
            "Authorization",
            format!("Bearer {}", token("test-password-hash", "user", 3600)),
        ))
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
            format!("Bearer {}", token("test-password-hash", "user", 3600)),
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
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;
    let bearer = token("test-password-hash", "admin", 3600);
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
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/ssl/providers")
        .insert_header((
            "Authorization",
            format!("Bearer {}", token("test-password-hash", "user", 3600)),
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
            .env_remove("RP_SESSION_KEY");
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
