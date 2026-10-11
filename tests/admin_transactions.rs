//! Regression tests for admin user/password transactions.
//!
//! These hit the real Actix router and a real temporary SQLite database. A
//! SQLite trigger injects a `RAISE(ABORT)` failure after the row update, so the
//! only way the assertions can hold is if the whole transaction rolls back.
use actix_web::{http::StatusCode, test as actix_test, web, App};
use diesel::{
    connection::SimpleConnection,
    r2d2::ConnectionManager,
    sql_query,
    sql_types::{BigInt, Integer, Text},
    QueryableByName, RunQueryDsl, SqliteConnection,
};
use rust_panel::{
    api,
    api::auth::generate_jwt,
    repository::migrations,
    service::{db::DBPool, rbac, sessions},
};
use std::{
    sync::Once,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

const TEST_SIGNING_KEY: &str = "admin-transactions-test-signing-key-at-least-32-bytes";

fn test_keys() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("RP_SECRET_KEY", TEST_SIGNING_KEY);
        std::env::set_var(
            "RP_SESSION_KEY",
            "admin-transactions-session-key-at-least-64-bytes-and-independent",
        );
        std::env::set_var(
            "RP_MASTER_KEY",
            "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f91",
        );
    });
}

/// One admin user (id 1) with the `user:admin` role from the real migrations.
fn database() -> (NamedTempFile, DBPool) {
    test_keys();
    let file = NamedTempFile::new().unwrap();
    let pool = DBPool::builder()
        .max_size(1)
        .connection_timeout(Duration::from_millis(100))
        .build(ConnectionManager::<SqliteConnection>::new(
            file.path().to_str().unwrap(),
        ))
        .unwrap();
    migrations::migrate(&pool).unwrap();
    let mut conn = pool.get().unwrap();
    conn.batch_execute(
        "INSERT INTO rp_users (
            username, password, salt, authority, error_count, status,
            created_at, updated_at, password_algo, must_change_password, token_version
        ) VALUES (
            'root', 'test-password-hash', 'test-salt', 'admin', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'sha1', 0, 0
        );",
    )
    .unwrap();
    rbac::assign_role_for_authority(&mut conn, 1, "admin").unwrap();
    drop(conn);
    (file, pool)
}

fn token_for(pool: &DBPool, user_id: i32, token_version: i32, expires_in: i64) -> String {
    let mut conn = pool.get().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let session =
        sessions::create_session(&mut conn, user_id, 3600, Some("127.0.0.1"), None).unwrap();
    drop(conn);
    generate_jwt(user_id, &session.jti, token_version, now + expires_in).unwrap()
}

#[derive(QueryableByName)]
struct UserState {
    #[diesel(sql_type = Text)]
    username: String,
    #[diesel(sql_type = Text)]
    password: String,
    #[diesel(sql_type = Text)]
    authority: String,
    #[diesel(sql_type = Integer)]
    status: i32,
    #[diesel(sql_type = Integer)]
    token_version: i32,
    #[diesel(sql_type = Integer)]
    must_change_password: i32,
}

fn user_state(pool: &DBPool, id: i32) -> UserState {
    let mut conn = pool.get().unwrap();
    sql_query(
        "SELECT username, password, authority, status, token_version, must_change_password
         FROM rp_users WHERE id = ?1",
    )
    .bind::<BigInt, _>(i64::from(id))
    .get_result(&mut conn)
    .unwrap()
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

fn live_sessions(pool: &DBPool, id: i32) -> i64 {
    let mut conn = pool.get().unwrap();
    sql_query("SELECT COUNT(*) AS count FROM sessions WHERE user_id = ?1 AND revoked_at IS NULL")
        .bind::<BigInt, _>(i64::from(id))
        .get_result::<CountRow>(&mut conn)
        .unwrap()
        .count
}

fn abort_token_bump(pool: &DBPool) {
    pool.get()
        .unwrap()
        .batch_execute(
            "CREATE TRIGGER abort_token_bump BEFORE UPDATE OF token_version ON rp_users
             BEGIN SELECT RAISE(ABORT, 'injected token_version failure'); END;",
        )
        .unwrap();
}

fn abort_session_revoke(pool: &DBPool) {
    pool.get()
        .unwrap()
        .batch_execute(
            "CREATE TRIGGER abort_session_revoke BEFORE UPDATE OF revoked_at ON sessions
             BEGIN SELECT RAISE(ABORT, 'injected session revoke failure'); END;",
        )
        .unwrap();
}

fn bearer(pool: &DBPool) -> String {
    format!("Bearer {}", token_for(pool, 1, 0, 3600))
}

#[actix_web::test]
async fn reset_password_failure_rolls_back_password_and_sessions() {
    let (_file, pool) = database();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let auth = bearer(&pool);
    assert_eq!(live_sessions(&pool, 1), 1);
    let before = user_state(&pool, 1);
    abort_token_bump(&pool);

    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::post()
            .uri("/api/v1/admin/users/1/reset-password")
            .insert_header(("Authorization", auth))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let after = user_state(&pool, 1);
    assert_eq!(after.password, before.password);
    assert_eq!(after.authority, before.authority);
    assert_eq!(after.status, before.status);
    assert_eq!(after.token_version, before.token_version);
    assert_eq!(after.must_change_password, before.must_change_password);
    assert_eq!(live_sessions(&pool, 1), 1);
}

#[actix_web::test]
async fn update_user_failure_rolls_back_user_and_role() {
    let (_file, pool) = database();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let auth = bearer(&pool);
    let before = user_state(&pool, 1);
    abort_token_bump(&pool);

    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::put()
            .uri("/api/v1/admin/users/1")
            .insert_header(("Authorization", auth))
            .set_json(serde_json::json!({
                "username": "renamed-admin",
                "password": "new-secret-password",
                "authority": "user",
                "status": 1
            }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let after = user_state(&pool, 1);
    assert_eq!(after.username, before.username);
    assert_eq!(after.password, before.password);
    assert_eq!(after.authority, before.authority);
    assert_eq!(after.status, before.status);
    assert_eq!(after.token_version, before.token_version);
    assert!(rbac::user_has_permission(&mut pool.get().unwrap(), 1, rbac::PERM_USER_ADMIN).unwrap());
    assert_eq!(live_sessions(&pool, 1), 1);
}

#[actix_web::test]
async fn update_user_failure_rolls_back_when_session_revoke_fails() {
    let (_file, pool) = database();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let auth = bearer(&pool);
    let before = user_state(&pool, 1);
    abort_session_revoke(&pool);

    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::put()
            .uri("/api/v1/admin/users/1")
            .insert_header(("Authorization", auth))
            .set_json(serde_json::json!({ "status": 1 }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let after = user_state(&pool, 1);
    assert_eq!(after.status, before.status);
    assert_eq!(after.token_version, before.token_version);
    assert_eq!(live_sessions(&pool, 1), 1);
}

#[actix_web::test]
async fn reset_password_success_rotates_and_revokes() {
    let (_file, pool) = database();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let auth = bearer(&pool);
    let before = user_state(&pool, 1);

    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::post()
            .uri("/api/v1/admin/users/1/reset-password")
            .insert_header(("Authorization", auth))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let after = user_state(&pool, 1);
    assert_ne!(after.password, before.password);
    assert_eq!(after.token_version, before.token_version + 1);
    assert_eq!(after.must_change_password, 1);
    assert_eq!(live_sessions(&pool, 1), 0);
}

#[actix_web::test]
async fn update_user_success_rotates_and_revokes() {
    let (_file, pool) = database();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let auth = bearer(&pool);
    let before = user_state(&pool, 1);

    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::put()
            .uri("/api/v1/admin/users/1")
            .insert_header(("Authorization", auth))
            .set_json(serde_json::json!({ "password": "new-secret-password" }))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let after = user_state(&pool, 1);
    assert_ne!(after.password, before.password);
    assert_eq!(after.token_version, before.token_version + 1);
    assert_eq!(after.must_change_password, 0);
    assert_eq!(live_sessions(&pool, 1), 0);
}

#[actix_web::test]
async fn delete_user_success_removes_account() {
    let (_file, pool) = database();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    let auth = bearer(&pool);

    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::delete()
            .uri("/api/v1/admin/users/1")
            .insert_header(("Authorization", auth))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let remaining: i64 = {
        let mut conn = pool.get().unwrap();
        sql_query("SELECT COUNT(*) AS count FROM rp_users WHERE id = 1")
            .get_result::<CountRow>(&mut conn)
            .unwrap()
            .count
    };
    assert_eq!(remaining, 0);
    assert_eq!(live_sessions(&pool, 1), 0);
}

#[actix_web::test]
async fn create_user_role_failure_rolls_back_account_and_duplicate_remains_bad_request() {
    let (_file, pool) = database();
    let auth = bearer(&pool);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .service(api::route::v1()),
    )
    .await;
    pool.get().unwrap().batch_execute("CREATE TRIGGER abort_role BEFORE INSERT ON user_roles BEGIN SELECT RAISE(ABORT, 'role failed'); END;").unwrap();
    let response = actix_test::call_service(&app, actix_test::TestRequest::post().uri("/api/v1/admin/users")
        .insert_header(("Authorization", auth.clone())).set_json(serde_json::json!({"username":"new-user","password":"new-password","authority":"operator"})).to_request()).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let count: CountRow = sql_query("SELECT COUNT(*) AS count FROM rp_users")
        .get_result(&mut *pool.get().unwrap())
        .unwrap();
    assert_eq!(count.count, 1);
    pool.get()
        .unwrap()
        .batch_execute("DROP TRIGGER abort_role")
        .unwrap();
    let response = actix_test::call_service(
        &app,
        actix_test::TestRequest::post()
            .uri("/api/v1/admin/users")
            .insert_header(("Authorization", auth))
            .set_json(
                serde_json::json!({"username":"root","password":"new-password","authority":"user"}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
