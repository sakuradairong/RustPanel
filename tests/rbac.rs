//! RBAC regressions: default viewers cannot call high-risk write APIs.
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

fn pool_with_viewer() -> (NamedTempFile, DBPool, String) {
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
            'viewer', 'hash', '', 'user', 0, 0,
            '2026-10-10 00:00:00', '2026-10-10 00:00:00', 'argon2id', 0, 0
        );
        INSERT OR IGNORE INTO user_roles (user_id, role_id)
        SELECT id, 3 FROM rp_users WHERE username = 'viewer';",
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
