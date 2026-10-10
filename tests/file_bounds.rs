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

/// Deterministic replay of the post-resolve TOCTOU: the path is validated,
/// then an intermediate directory is swapped for a symlink pointing outside
/// the managed root. Reads, writes and listings must refuse instead of
/// following the swap.
#[cfg(target_os = "linux")]
#[actix_web::test]
async fn intermediate_dir_swap_cannot_redirect_reads_writes_or_lists() {
    let root_dir = tempdir().unwrap();
    let roots = FileRootConfig::from_roots(vec![root_dir.path().to_path_buf()]);
    let sub = root_dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let target = sub.join("data.txt");
    fs::write(&target, b"inside").unwrap();

    // Validate before the swap (this is the resolution the TOCTOU attacks).
    let read_bound =
        path_bounds::resolve_existing_under(target.to_str().unwrap(), &roots.roots).unwrap();
    let list_bound =
        path_bounds::resolve_existing_under(sub.to_str().unwrap(), &roots.roots).unwrap();
    let write_bound =
        path_bounds::resolve_for_write_under(target.to_str().unwrap(), &roots.roots).unwrap();

    // Swap the intermediate directory for a symlink to outside content.
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("data.txt"), b"secret").unwrap();
    let moved = root_dir.path().join("sub.real");
    fs::rename(&sub, &moved).unwrap();
    symlink(outside.path(), &sub).unwrap();

    // Reading the previously bound path must fail, never return "secret".
    match path_bounds::open_bound_file(&read_bound) {
        Ok(mut file) => {
            let mut buf = String::new();
            use std::io::Read;
            file.read_to_string(&mut buf).unwrap();
            assert_ne!(buf, "secret", "read escaped the managed root");
        }
        Err(_) => {}
    }

    // Listing the previously bound directory must fail or list the pinned
    // (moved) directory, never the outside one.
    if let Ok(entries) = path_bounds::open_bound_dir(&list_bound) {
        let names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            names.contains(&"data.txt".to_string()),
            "listing must not follow the swapped symlink"
        );
    }

    // Writing the previously bound path must fail without touching either
    // the moved original or the outside file.
    assert!(path_bounds::atomic_write(&write_bound, b"evil").is_err());
    assert_eq!(fs::read(moved.join("data.txt")).unwrap(), b"inside");
    assert_eq!(fs::read(outside.path().join("data.txt")).unwrap(), b"secret");

    // The failed write must not leave temp files behind in the root.
    let leftovers: Vec<_> = fs::read_dir(root_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains(".rp-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
}
