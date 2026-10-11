//! SSE one-time ticket regressions (RP-05 / C.4).
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

const TEST_SIGNING_KEY: &str = "sse-ticket-test-signing-key-32-bytes!!";

fn test_keys() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("RP_SECRET_KEY", TEST_SIGNING_KEY);
        std::env::set_var(
            "RP_SESSION_KEY",
            "sse-ticket-test-session-key-at-least-64-bytes-and-independent-xx",
        );
        std::env::set_var(
            "RP_MASTER_KEY",
            "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f91",
        );
    });
}

fn pool_with_user() -> (NamedTempFile, DBPool, String) {
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
            'op', 'hash', '', 'user', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', 0, 0
        );
        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT id, 2 FROM rp_users WHERE username = 'op';",
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
async fn ticket_connects_once_and_rejects_reuse_wrong_stream_and_query_jwt() {
    let (_file, pool, bearer) = pool_with_user();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;

    let issue = actix_test::TestRequest::post()
        .uri("/api/v1/monitor/tickets")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .set_json(serde_json::json!({"stream":"monitor"}))
        .to_request();
    let resp = actix_test::call_service(&app, issue).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = actix_test::read_body_json(resp).await;
    let ticket = body["data"]["ticket"].as_str().unwrap().to_string();

    let ok = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/monitor?ticket={ticket}"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, ok).await.status(),
        StatusCode::OK
    );

    let reuse = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/monitor?ticket={ticket}"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, reuse).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let bad_stream = actix_test::TestRequest::post()
        .uri("/api/v1/monitor/tickets")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .set_json(serde_json::json!({"stream":"jobs"}))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, bad_stream).await.status(),
        StatusCode::BAD_REQUEST
    );

    let query_jwt = actix_test::TestRequest::get()
        .uri(&format!("/api/v1/monitor?token={bearer}"))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, query_jwt).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
async fn logout_invalidates_unused_tickets_and_ends_an_open_stream() {
    use actix_web::body::MessageBody;
    let (_file, pool, bearer) = pool_with_user();
    let app = actix_test::init_service(App::new().app_data(web::Data::new(pool.clone())).service(api::route::v1())).await;
    let mut tickets = Vec::new();
    for _ in 0..2 {
        let response = actix_test::call_service(&app, actix_test::TestRequest::post()
            .uri("/api/v1/monitor/tickets").insert_header(("Authorization", format!("Bearer {bearer}")))
            .set_json(serde_json::json!({"stream":"monitor"})).to_request()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = actix_test::read_body_json(response).await;
        tickets.push(body["data"]["ticket"].as_str().unwrap().to_owned());
    }
    let response = actix_test::call_service(&app, actix_test::TestRequest::get()
        .uri(&format!("/api/v1/monitor?ticket={}", tickets[0])).to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = std::pin::pin!(response.into_body());
    let first = tokio::time::timeout(Duration::from_secs(5), futures_util::future::poll_fn(|cx| body.as_mut().poll_next(cx))).await.unwrap().unwrap().unwrap();
    assert!(String::from_utf8_lossy(&first).contains("data:"));
    let logout = actix_test::call_service(&app, actix_test::TestRequest::post()
        .uri("/api/v1/login/logout").insert_header(("Authorization", format!("Bearer {bearer}"))).to_request()).await;
    assert_eq!(logout.status(), StatusCode::OK);
    let rejected = actix_test::call_service(&app, actix_test::TestRequest::get()
        .uri(&format!("/api/v1/monitor?ticket={}", tickets[1])).to_request()).await;
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let chunk = futures_util::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await.unwrap().unwrap();
            if String::from_utf8_lossy(&chunk).contains("event: revoked") { break; }
        }
    }).await.unwrap();
}
