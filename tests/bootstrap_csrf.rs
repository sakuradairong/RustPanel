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
async fn password_change_is_atomic_and_incorrect_password_keeps_the_session() {
    use diesel::{sql_query, sql_types::Text, RunQueryDsl};
    let (_f, pool, bearer) = pool_user(true);
    let original = rust_panel::service::password::hash_argon2id("old-password").unwrap();
    sql_query("UPDATE rp_users SET password = ?1").bind::<Text, _>(&original).execute(&mut *pool.get().unwrap()).unwrap();
    let app = actix_test::init_service(App::new().app_data(web::Data::new(pool.clone())).service(api::route::v1())).await;
    let request = |current: &str| actix_test::TestRequest::post().uri("/api/v1/user/change-password")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .set_json(serde_json::json!({"current_password":current,"new_password":"new-password"})).to_request();
    assert_eq!(actix_test::call_service(&app, request("incorrect")).await.status(), StatusCode::BAD_REQUEST);
    pool.get().unwrap().batch_execute("CREATE TRIGGER fail_version BEFORE UPDATE OF token_version ON rp_users BEGIN SELECT RAISE(ABORT, 'version failed'); END;").unwrap();
    assert_eq!(actix_test::call_service(&app, request("old-password")).await.status(), StatusCode::INTERNAL_SERVER_ERROR);
    #[derive(diesel::QueryableByName)]
    struct Password { #[diesel(sql_type = Text)] password: String }
    let row: Password = sql_query("SELECT password FROM rp_users").get_result(&mut *pool.get().unwrap()).unwrap();
    assert_eq!(row.password, original);
    let me = || actix_test::TestRequest::get().uri("/api/v1/user/me").insert_header(("Authorization", format!("Bearer {bearer}"))).to_request();
    assert_eq!(actix_test::call_service(&app, me()).await.status(), StatusCode::OK);
    pool.get().unwrap().batch_execute("DROP TRIGGER fail_version").unwrap();
    assert_eq!(actix_test::call_service(&app, request("old-password")).await.status(), StatusCode::OK);
    assert_eq!(actix_test::call_service(&app, me()).await.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn actual_login_response_sets_a_protected_cookie_and_logout_removes_it() {
    use diesel::{sql_query, sql_types::Text, RunQueryDsl};
    use rust_panel::{common::fun::md5, service::global::CONF};
    let (_f, pool, _) = pool_user(false);
    let hash = rust_panel::service::password::hash_argon2id("login-password").unwrap();
    sql_query("UPDATE rp_users SET password = ?1").bind::<Text, _>(&hash).execute(&mut *pool.get().unwrap()).unwrap();
    let app = actix_test::init_service(App::new().app_data(web::Data::new(pool)).service(api::route::v1())).await;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let iv = md5(CONF.app.security_dir.clone());
    let key = md5(format!("{}{timestamp}{iv}", CONF.app.security_dir));
    let encrypt = |value: &str| gmsm::sm4::sm4_cbc_encrypt_hex(value, &key, &iv);
    let response = actix_test::call_service(&app, actix_test::TestRequest::post()
        .uri(&format!("/api/v1/login/sign/{}", CONF.app.security_dir))
        .set_json(serde_json::json!({"username":encrypt("u"),"password":encrypt("login-password"),"timestamp":timestamp})).to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.response().cookies().find(|cookie| cookie.name() == "token").unwrap();
    assert_eq!(cookie.path(), Some("/"));
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(cookie.same_site(), Some(actix_web::cookie::SameSite::Lax));
    assert_eq!(cookie.secure().unwrap_or(false), rust_panel::service::bind_policy::cookie_secure_recommended(&CONF.app.host));
    let body: serde_json::Value = actix_test::read_body_json(response).await;
    let response = actix_test::call_service(&app, actix_test::TestRequest::post().uri("/api/v1/login/logout")
        .insert_header(("Authorization", format!("Bearer {}", body["data"]["token"].as_str().unwrap()))).to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.response().cookies().find(|cookie| cookie.name() == "token").unwrap();
    assert_eq!(cookie.path(), Some("/"));
    assert_eq!(cookie.max_age().unwrap().whole_seconds(), 0);
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

#[actix_web::test]
async fn cookie_mutating_request_with_wrong_origin_is_forbidden() {
    let (_f, pool, bearer) = pool_user(false);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .wrap(api::csrf::CsrfProtect)
            .service(api::route::v1()),
    )
    .await;

    // Cross-origin cookie-authenticated mutates must fail closed: exact
    // host:port match required, no suffix tricks.
    for origin in [
        "https://evil.example",
        "http://127.0.0.1:8080.attacker.invalid",
        "http://not127.0.0.1:8080",
    ] {
        let request = actix_test::TestRequest::post()
            .uri("/api/v1/login/logout")
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .insert_header(("Cookie", "token=session-cookie-value"))
            .insert_header(("Host", "127.0.0.1:8080"))
            .insert_header(("Origin", origin))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "origin {origin} must be rejected"
        );
    }
}
