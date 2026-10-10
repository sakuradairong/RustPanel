//! File API path-boundary regressions (RP-06 / C.5).
use actix_web::{http::StatusCode, test as actix_test, web, App};
use diesel::{connection::SimpleConnection, r2d2::ConnectionManager, SqliteConnection};
use rust_panel::{
    api,
    api::auth::generate_jwt,
    repository::migrations,
    service::{
        db::DBPool,
        path_bounds::{self, FileRootConfig},
        sessions,
    },
};
use std::{
    fs,
    os::unix::fs::symlink,
    sync::Once,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::{tempdir, NamedTempFile};

const TEST_SIGNING_KEY: &str = "file-bounds-test-signing-key-32bytes-min!";

fn test_keys() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_var("RP_SECRET_KEY", TEST_SIGNING_KEY);
        std::env::set_var(
            "RP_SESSION_KEY",
            "file-bounds-test-session-key-at-least-64-bytes-and-independent-xx",
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
async fn escape_paths_and_symlink_return_forbidden() {
    let root_dir = tempdir().unwrap();
    let roots = FileRootConfig::from_roots(vec![root_dir.path().to_path_buf()]);
    let inside = root_dir.path().join("ok.txt");
    fs::write(&inside, b"ok").unwrap();
    let link = root_dir.path().join("to-passwd");
    symlink("/etc/passwd", &link).unwrap();

    let (_file, pool, bearer) = pool_with_admin();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(roots))
            .service(api::route::v1()),
    )
    .await;

    let cases = [
        (
            format!(
                "/api/v1/file/list?path={}/../etc&current=1&pageSize=10",
                root_dir.path().display()
            ),
            "dotdot-list",
        ),
        (
            "/api/v1/file/list?path=/etc&current=1&pageSize=10".to_string(),
            "outside-list",
        ),
        (
            format!("/api/v1/file/content?path={}", link.display()),
            "symlink-content",
        ),
    ];

    for (uri, label) in cases {
        let request = actix_test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {bearer}")))
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, request).await.status(),
            StatusCode::FORBIDDEN,
            "{label}"
        );
    }

    let save_escape = actix_test::TestRequest::post()
        .uri("/api/v1/file/save")
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .set_json(serde_json::json!({"path":"/etc/passwd","content":"x"}))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, save_escape).await.status(),
        StatusCode::FORBIDDEN,
        "outside-save"
    );
}

#[actix_web::test]
async fn in_root_list_and_atomic_write_succeed() {
    let root_dir = tempdir().unwrap();
    let roots = FileRootConfig::from_roots(vec![root_dir.path().to_path_buf()]);
    let target = root_dir.path().join("hello.txt");
    fs::write(&target, b"hello").unwrap();

    let (_file, pool, bearer) = pool_with_admin();
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(roots.clone()))
            .service(api::route::v1()),
    )
    .await;

    let list_uri = format!(
        "/api/v1/file/list?path={}&current=1&pageSize=10",
        root_dir.path().display()
    );
    let list = actix_test::TestRequest::get()
        .uri(&list_uri)
        .insert_header(("Authorization", format!("Bearer {bearer}")))
        .to_request();
    assert_eq!(
        actix_test::call_service(&app, list).await.status(),
        StatusCode::OK
    );

    let save_path = root_dir.path().join("written.txt");
    let bound =
        path_bounds::resolve_for_write_under(save_path.to_str().unwrap(), &roots.roots).unwrap();
    path_bounds::atomic_write(&bound, b"panel").unwrap();
    assert_eq!(fs::read(&save_path).unwrap(), b"panel");
}
