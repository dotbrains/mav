use super::*;

#[test]
fn test_build_extension_map() {
    let map = build_extension_to_language_map();
    assert!(!map.is_empty());
    assert_eq!(map.get("rs"), Some(&"Rust".to_string()));
    assert_eq!(map.get("py"), Some(&"Python".to_string()));
    assert_eq!(map.get("go"), Some(&"Go".to_string()));
}

#[test]
fn test_detect_language_by_extension() {
    let map = build_extension_to_language_map();

    assert_eq!(
        detect_language("src/main.rs", &map),
        Some("Rust".to_string())
    );
    assert_eq!(
        detect_language("lib/foo.py", &map),
        Some("Python".to_string())
    );
    assert_eq!(
        detect_language("cmd/server.go", &map),
        Some("Go".to_string())
    );
    assert_eq!(detect_language("index.tsx", &map), Some("TSX".to_string()));
}

#[test]
fn test_detect_language_by_filename() {
    let map = build_extension_to_language_map();

    // PKGBUILD is a filename-based match for Shell Script
    assert_eq!(
        detect_language("PKGBUILD", &map),
        Some("Shell Script".to_string())
    );
    assert_eq!(
        detect_language("project/PKGBUILD", &map),
        Some("Shell Script".to_string())
    );
    // .env files are also Shell Script
    assert_eq!(
        detect_language(".env", &map),
        Some("Shell Script".to_string())
    );
}

#[test]
fn test_detect_language_unknown() {
    let map = build_extension_to_language_map();

    assert_eq!(detect_language("file.xyz123", &map), None);
    assert_eq!(detect_language("random_file", &map), None);
}

#[test]
fn test_get_cursor_path() {
    let line = r#"{"cursor_path": "src/main.rs", "other": "data"}"#;
    assert_eq!(get_cursor_path(line), Some("src/main.rs".to_string()));

    let line_no_cursor = r#"{"other": "data"}"#;
    assert_eq!(get_cursor_path(line_no_cursor), None);

    let invalid_json = "not json";
    assert_eq!(get_cursor_path(invalid_json), None);
}

#[test]
fn test_get_all_languages() {
    let map = build_extension_to_language_map();
    let languages = get_all_languages(&map);

    assert!(!languages.is_empty());

    let rust_entry = languages.iter().find(|(name, _)| name == "Rust");
    assert!(rust_entry.is_some());
    let (_, rust_extensions) = rust_entry.unwrap();
    assert!(rust_extensions.contains(&"rs".to_string()));
}
