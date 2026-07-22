use super::*;

#[test]
fn test_multiple_extensions() {
    // No extensions
    let path = Path::new("/a/b/c/file_name");
    assert_eq!(path.multiple_extensions(), None);

    // Only one extension
    let path = Path::new("/a/b/c/file_name.tsx");
    assert_eq!(path.multiple_extensions(), None);

    // Stories sample extension
    let path = Path::new("/a/b/c/file_name.stories.tsx");
    assert_eq!(path.multiple_extensions(), Some("stories.tsx".to_string()));

    // Longer sample extension
    let path = Path::new("/a/b/c/long.app.tar.gz");
    assert_eq!(path.multiple_extensions(), Some("app.tar.gz".to_string()));
}

#[test]
fn test_strip_path_suffix() {
    let base = Path::new("/a/b/c/file_name");
    let suffix = Path::new("file_name");
    assert_eq!(strip_path_suffix(base, suffix), Some(Path::new("/a/b/c")));

    let base = Path::new("/a/b/c/file_name.tsx");
    let suffix = Path::new("file_name.tsx");
    assert_eq!(strip_path_suffix(base, suffix), Some(Path::new("/a/b/c")));

    let base = Path::new("/a/b/c/file_name.stories.tsx");
    let suffix = Path::new("c/file_name.stories.tsx");
    assert_eq!(strip_path_suffix(base, suffix), Some(Path::new("/a/b")));

    let base = Path::new("/a/b/c/long.app.tar.gz");
    let suffix = Path::new("b/c/long.app.tar.gz");
    assert_eq!(strip_path_suffix(base, suffix), Some(Path::new("/a")));

    let base = Path::new("/a/b/c/long.app.tar.gz");
    let suffix = Path::new("/a/b/c/long.app.tar.gz");
    assert_eq!(strip_path_suffix(base, suffix), Some(Path::new("")));

    let base = Path::new("/a/b/c/long.app.tar.gz");
    let suffix = Path::new("/a/b/c/no_match.app.tar.gz");
    assert_eq!(strip_path_suffix(base, suffix), None);

    let base = Path::new("/a/b/c/long.app.tar.gz");
    let suffix = Path::new("app.tar.gz");
    assert_eq!(strip_path_suffix(base, suffix), None);
}

#[test]
fn test_strip_prefix() {
    let expected = [
        (
            PathStyle::Posix,
            "/a/b/c",
            "/a/b",
            Some(rel_path("c").into_arc()),
        ),
        (
            PathStyle::Posix,
            "/a/b/c",
            "/a/b/",
            Some(rel_path("c").into_arc()),
        ),
        (
            PathStyle::Posix,
            "/a/b/c",
            "/",
            Some(rel_path("a/b/c").into_arc()),
        ),
        (PathStyle::Posix, "/a/b/c", "", None),
        (PathStyle::Posix, "/a/b//c", "/a/b/", None),
        (PathStyle::Posix, "/a/bc", "/a/b", None),
        (
            PathStyle::Posix,
            "/a/b/c",
            "/a/b/c",
            Some(rel_path("").into_arc()),
        ),
        (
            PathStyle::Windows,
            "C:\\a\\b\\c",
            "C:\\a\\b",
            Some(rel_path("c").into_arc()),
        ),
        (
            PathStyle::Windows,
            "C:\\a\\b\\c",
            "C:\\a\\b\\",
            Some(rel_path("c").into_arc()),
        ),
        (
            PathStyle::Windows,
            "C:\\a\\b\\c",
            "C:\\",
            Some(rel_path("a/b/c").into_arc()),
        ),
        (PathStyle::Windows, "C:\\a\\b\\c", "", None),
        (PathStyle::Windows, "C:\\a\\b\\\\c", "C:\\a\\b\\", None),
        (PathStyle::Windows, "C:\\a\\bc", "C:\\a\\b", None),
        (
            PathStyle::Windows,
            "C:\\a\\b/c",
            "C:\\a\\b",
            Some(rel_path("c").into_arc()),
        ),
        (
            PathStyle::Windows,
            "C:\\a\\b/c",
            "C:\\a\\b\\",
            Some(rel_path("c").into_arc()),
        ),
        (
            PathStyle::Windows,
            "C:\\a\\b/c",
            "C:\\a\\b/",
            Some(rel_path("c").into_arc()),
        ),
    ];
    let actual = expected.clone().map(|(style, child, parent, _)| {
        (
            style,
            child,
            parent,
            style
                .strip_prefix(child.as_ref(), parent.as_ref())
                .map(|rel_path| rel_path.into_arc()),
        )
    });
    pretty_assertions::assert_eq!(actual, expected);
}
