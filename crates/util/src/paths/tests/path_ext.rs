use super::*;

#[perf]
fn test_path_compact() {
    let path: PathBuf = [
        home_dir().to_string_lossy().into_owned(),
        "some_file.txt".to_string(),
    ]
    .iter()
    .collect();
    if cfg!(any(target_os = "linux", target_os = "freebsd")) || cfg!(target_os = "macos") {
        assert_eq!(path.compact().to_str(), Some("~/some_file.txt"));
    } else {
        assert_eq!(path.compact().to_str(), path.to_str());
    }
}

#[perf]
fn test_extension_or_hidden_file_name() {
    // No dots in name
    let path = Path::new("/a/b/c/file_name.rs");
    assert_eq!(path.extension_or_hidden_file_name(), Some("rs"));

    // Single dot in name
    let path = Path::new("/a/b/c/file.name.rs");
    assert_eq!(path.extension_or_hidden_file_name(), Some("rs"));

    // Multiple dots in name
    let path = Path::new("/a/b/c/long.file.name.rs");
    assert_eq!(path.extension_or_hidden_file_name(), Some("rs"));

    // Hidden file, no extension
    let path = Path::new("/a/b/c/.gitignore");
    assert_eq!(path.extension_or_hidden_file_name(), Some("gitignore"));

    // Hidden file, with extension
    let path = Path::new("/a/b/c/.eslintrc.js");
    assert_eq!(path.extension_or_hidden_file_name(), Some("eslintrc.js"));
}

#[perf]
// fn edge_of_glob() {
//     let path = Path::new("/work/node_modules");
//     let path_matcher =
//         PathMatcher::new(&["**/node_modules/**".to_owned()], PathStyle::Posix).unwrap();
//     assert!(
//         path_matcher.is_match(path),
//         "Path matcher should match {path:?}"
//     );
// }

// #[perf]
// fn file_in_dirs() {
//     let path = Path::new("/work/.env");
//     let path_matcher = PathMatcher::new(&["**/.env".to_owned()], PathStyle::Posix).unwrap();
//     assert!(
//         path_matcher.is_match(path),
//         "Path matcher should match {path:?}"
//     );
//     let path = Path::new("/work/package.json");
//     assert!(
//         !path_matcher.is_match(path),
//         "Path matcher should not match {path:?}"
//     );
// }

// #[perf]
// fn project_search() {
//     let path = Path::new("/Users/someonetoignore/work/mav/mav.dev/node_modules");
//     let path_matcher =
//         PathMatcher::new(&["**/node_modules/**".to_owned()], PathStyle::Posix).unwrap();
//     assert!(
//         path_matcher.is_match(path),
//         "Path matcher should match {path:?}"
//     );
// }
#[perf]
#[cfg(target_os = "windows")]
fn test_sanitized_path() {
    let path = Path::new("C:\\Users\\someone\\test_file.rs");
    let sanitized_path = SanitizedPath::new(path);
    assert_eq!(
        sanitized_path.to_string(),
        "C:\\Users\\someone\\test_file.rs"
    );

    let path = Path::new("\\\\?\\C:\\Users\\someone\\test_file.rs");
    let sanitized_path = SanitizedPath::new(path);
    assert_eq!(
        sanitized_path.to_string(),
        "C:\\Users\\someone\\test_file.rs"
    );
}
