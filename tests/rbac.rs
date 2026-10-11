//! RBAC regressions: default viewers cannot call high-risk write APIs.
use actix_web::{http::StatusCode, test as actix_test, web, App};
use diesel::{connection::SimpleConnection, r2d2::ConnectionManager, RunQueryDsl, SqliteConnection};
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

const TEST_SIGNING_KEY: &str = "rbac-test-signing-key-with-at-least-32-bytes!!";

fn test_keys() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("RP_SECRET_KEY", TEST_SIGNING_KEY);
        std::env::set_var(
            "RP_SESSION_KEY",
            "rbac-test-session-key-at-least-64-bytes-and-independent-value-xx",
        );
        std::env::set_var(
            "RP_MASTER_KEY",
            "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f91",
        );
    });
}

fn pool_with_role(role_sql: &str) -> (NamedTempFile, DBPool, String) {
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
        .batch_execute(&format!(
            "INSERT INTO rp_users (
            username, password, salt, authority, error_count, status,
            created_at, updated_at, password_algo, must_change_password, token_version
        ) VALUES (
            'viewer', 'hash', '', 'user', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', 0, 0
        );
        {role_sql}",
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
async fn identity_and_menus_reflect_current_database_capabilities() {
    let (_file, pool, bearer) = pool_with_viewer();
    let app = actix_test::init_service(App::new().app_data(web::Data::new(pool.clone())).service(api::route::v1())).await;
    let response = actix_test::call_service(&app, actix_test::TestRequest::get().uri("/api/v1/user/me")
        .insert_header(("Authorization", format!("Bearer {bearer}"))).to_request()).await;
    let body: serde_json::Value = actix_test::read_body_json(response).await;
    assert_eq!(body["roles"], serde_json::json!(["viewer"]));
    assert!(body["permissions"].as_array().unwrap().iter().any(|p| p == "file:read"));
    assert!(!body["permissions"].as_array().unwrap().iter().any(|p| p == "docker:write"));
    pool.get().unwrap().batch_execute("DELETE FROM user_roles; INSERT INTO user_roles VALUES (1, 2)").unwrap();
    let response = actix_test::call_service(&app, actix_test::TestRequest::get().uri("/api/v1/user/menus")
        .insert_header(("Authorization", format!("Bearer {bearer}"))).to_request()).await;
    let body: serde_json::Value = actix_test::read_body_json(response).await;
    assert_eq!(body["data"]["info"]["username"], "viewer");
    assert_eq!(body["data"]["roles"], serde_json::json!(["operator"]));
    assert!(body["data"]["access"].as_array().unwrap().iter().any(|p| p == "docker:write"));
    assert!(!body["data"]["menus"].as_array().unwrap().iter().any(|m| m["path"] == "/admin/users"));
}

#[actix_web::test]
async fn viewer_is_forbidden_from_installer_site_firewall_and_file_writes() {
    let (_file, pool, bearer) = pool_with_viewer();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;

    let forbidden = [
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/installer/install")
                .set_json(serde_json::json!({"software":"nginx","version":"latest"})),
            "installer",
        ),
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/webserver/sites")
                .set_json(serde_json::json!({"server_name":"example.invalid","root":"/var/www"})),
            "sites",
        ),
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/firewall/rules")
                .set_json(serde_json::json!({"chain":"INPUT","rule":"-p tcp --dport 1 -j ACCEPT"})),
            "firewall",
        ),
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/file/save")
                .set_json(serde_json::json!({"path":"/tmp/x","content":"nope"})),
            "file",
        ),
    ];

    for (builder, label) in forbidden {
        let request = builder
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "{label}"
        );
    }
}

#[actix_web::test]
async fn viewer_is_forbidden_from_docker_and_process_writes() {
    let (_file, pool, bearer) = pool_with_viewer();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;

    let forbidden = [
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/docker/containers")
                .set_json(serde_json::json!({"name":"evil","image":"alpine"})),
            "docker create container",
        ),
        (
            actix_test::TestRequest::post().uri("/api/v1/docker/containers/abc123/start"),
            "docker start container",
        ),
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/docker/images/pull")
                .set_json(serde_json::json!({"image":"alpine"})),
            "docker pull image",
        ),
        (
            actix_test::TestRequest::post().uri("/api/v1/docker/images/abc123/remove"),
            "docker remove image",
        ),
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/docker/networks")
                .set_json(serde_json::json!({"name":"evil"})),
            "docker create network",
        ),
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/docker/volumes")
                .set_json(serde_json::json!({"name":"evil"})),
            "docker create volume",
        ),
        (
            actix_test::TestRequest::post().uri("/api/v1/process/kill/99999"),
            "process kill",
        ),
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/docker/containers/abc123/exec")
                .set_json(serde_json::json!({"cmd":"id"})),
            "docker exec",
        ),
        (
            actix_test::TestRequest::post()
                .uri("/api/v1/docker/images/prune")
                .set_json(serde_json::json!({"dangling_only":true})),
            "docker image prune",
        ),
        (
            actix_test::TestRequest::post().uri("/api/v1/docker/volumes/prune"),
            "docker volume prune",
        ),
    ];

    for (builder, label) in forbidden {
        let request = builder
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "{label}"
        );
    }
}

#[actix_web::test]
async fn viewer_retains_read_access_to_docker_and_log_lists() {
    let (_file, pool, bearer) = pool_with_viewer();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;

    // Docker reads pass RBAC for viewers; the backend call itself may fail
    // with 5xx when no Docker socket exists, but must never be a 403.
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/docker/containers")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    let status = actix_test::call_service(&app, request).await.status();
    assert_ne!(status, StatusCode::FORBIDDEN, "docker list containers");

    // Log listing is a pure read guarded by log:read, which viewers hold.
    let request = actix_test::TestRequest::get()
        .uri("/api/v1/log/list")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, request).await.status(),
        StatusCode::OK,
        "log list"
    );
}

fn pool_with_viewer() -> (NamedTempFile, DBPool, String) {
    pool_with_role(
        "INSERT OR IGNORE INTO user_roles (user_id, role_id)
         SELECT id, 3 FROM rp_users WHERE username = 'viewer';",
    )
}

/// Role rows are the single source of truth: the legacy `authority` column
/// no longer grants anything by itself.
#[actix_web::test]
async fn legacy_admin_without_role_rows_holds_nothing() {
    let (_file, pool, bearer) = pool_with_role("-- no role rows");
    let mut conn = pool.get().unwrap();
    diesel::sql_query("UPDATE rp_users SET authority = 'admin' WHERE id = 1")
        .execute(&mut conn)
        .unwrap();
    drop(conn);
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;

    for (method, path) in [
        (actix_web::http::Method::GET, "/api/v1/admin/users"),
        (actix_web::http::Method::POST, "/api/v1/ssl/issue"),
        (actix_web::http::Method::POST, "/api/v1/docker/containers/abc/start"),
    ] {
        let request = actix_test::TestRequest::default()
            .method(method)
            .uri(path)
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .set_json(serde_json::json!({"domains": ["example.invalid"]}))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "authority='admin' without role rows must not grant {path}"
        );
    }
}

/// The seeded operator role actually reaches the cert endpoints it holds
/// permissions for (previously gated behind user:admin, unreachable).
#[actix_web::test]
async fn operator_passes_rbac_on_certificate_mutations() {
    let (_file, pool, bearer) = pool_with_role(
        "UPDATE rp_users SET authority = 'user' WHERE id = 1;
         INSERT OR IGNORE INTO user_roles (user_id, role_id)
         SELECT id, 2 FROM rp_users WHERE username = 'viewer';",
    );
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .service(api::route::v1()),
    )
    .await;

    // RBAC passes; the certbot call itself may fail (500) in the test
    // environment — the assertion is that it is never a permission denial.
    for path in ["/api/v1/ssl/issue", "/api/v1/ssl/setup"] {
        let request = actix_test::TestRequest::post()
            .uri(path)
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .set_json(serde_json::json!({"domains": ["example.invalid"], "email": "t@e.invalid"}))
            .to_request();
        assert_ne!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "operator must reach {path}"
        );
    }
}
