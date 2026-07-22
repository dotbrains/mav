use super::*;

#[test]
fn test_parse_str_treats_paren_suffix_as_position() {
    // This documents the behavior that causes the folder-drop bug: a name ending in
    // `(N)` is parsed as `name ` + row N. The fix lives in `derive_paths_with_position`,
    // which restores the original path when it exists on disk (file or directory).
    let parsed = PathWithPosition::parse_str("/root/Test (3)");
    assert_eq!(parsed.path, PathBuf::from("/root/Test "));
    assert_eq!(parsed.row, Some(3));
}

#[test]
fn test_join_path_uses_path_style_separator() {
    let posix_path = PathStyle::Posix
        .join_path(Path::new("/home/user/dev"), "worktrees")
        .unwrap();
    let windows_path = PathStyle::Windows
        .join_path(Path::new("C:\\Users\\user\\dev"), "worktrees")
        .unwrap();

    assert_eq!(posix_path, PathBuf::from("/home/user/dev/worktrees"));
    assert_eq!(
        windows_path.to_string_lossy(),
        "C:\\Users\\user\\dev\\worktrees"
    );
}

#[test]
fn test_normalize_uses_path_style_separator() {
    assert_eq!(
        PathStyle::Posix.normalize("/home/user/dev/../worktrees/./mav"),
        "/home/user/worktrees/mav"
    );
    assert_eq!(
        PathStyle::Windows.normalize("C:\\Users\\user\\dev\\worktrees"),
        "C:\\Users\\user\\dev\\worktrees"
    );
}
