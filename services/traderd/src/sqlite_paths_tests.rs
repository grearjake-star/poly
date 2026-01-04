use super::{ensure_sqlite_parent_dir, normalize_windows_style_sqlite_path};
use std::env;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn extract_path_part(uri: &str) -> &str {
    uri.trim_start_matches("sqlite://")
        .split_once('?')
        .map(|(path, _)| path)
        .unwrap_or_else(|| uri.trim_start_matches("sqlite://"))
}

#[test]
fn normalizes_windows_style_paths_from_urls() {
    let cases = [
        ("sqlite://C:/foo", PathBuf::from("C:foo")),
        ("sqlite:///C:/foo", PathBuf::from("C:foo")),
        ("sqlite://D:\\bar", PathBuf::from("D:bar")),
    ];

    for (input, expected) in cases {
        let path_part = extract_path_part(input);
        let normalized = normalize_windows_style_sqlite_path(path_part);
        assert_eq!(normalized, expected, "input `{input}` should normalize");
    }
}

#[test]
fn creates_parent_directories_for_various_sqlite_urls() {
    let _guard = crate::cwd_guard().lock().expect("cwd guard should lock");
    let temp_root = env::temp_dir().join(format!("traderd_sqlite_paths_{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_root).expect("temp dir should be creatable");

    let original_cwd = env::current_dir().expect("cwd should be available");
    env::set_current_dir(&temp_root).expect("should be able to change to temp dir");

    let cases = [
        ("sqlite://C:/foo/db.sqlite", temp_root.join("C:foo")),
        ("sqlite:///C:/foo/db.sqlite", temp_root.join("C:foo")),
        ("sqlite://D:\\bar/db.sqlite", temp_root.join("D:bar")),
    ];

    for (input, expected_parent) in cases {
        ensure_sqlite_parent_dir(input).expect("parent dir creation should succeed");
        assert!(
            expected_parent.is_dir(),
            "expected parent directory {:?} to exist",
            expected_parent
        );
        fs::remove_dir_all(&expected_parent).expect("parent dir cleanup should succeed");
    }

    ensure_sqlite_parent_dir("sqlite::memory:")
        .expect("memory connection string should be skipped");
    assert_eq!(
        fs::read_dir(&temp_root)
            .expect("temp dir should exist")
            .count(),
        0,
        "memory URLs should not create filesystem entries"
    );

    env::set_current_dir(original_cwd).expect("should be able to restore cwd");
    fs::remove_dir_all(&temp_root).expect("temp dir cleanup should succeed");
}
