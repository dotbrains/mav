use super::*;

#[perf]
fn path_with_position_parse_posix_path() {
    // Test POSIX filename edge cases
    // Read more at https://en.wikipedia.org/wiki/Filename
    assert_eq!(
        PathWithPosition::parse_str("test_file"),
        PathWithPosition {
            path: PathBuf::from("test_file"),
            row: None,
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("a:bc:.zip:1"),
        PathWithPosition {
            path: PathBuf::from("a:bc:.zip"),
            row: Some(1),
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("one.second.zip:1"),
        PathWithPosition {
            path: PathBuf::from("one.second.zip"),
            row: Some(1),
            column: None
        }
    );

    // Trim off trailing `:`s for otherwise valid input.
    assert_eq!(
        PathWithPosition::parse_str("test_file:10:1:"),
        PathWithPosition {
            path: PathBuf::from("test_file"),
            row: Some(10),
            column: Some(1)
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("test_file.rs:"),
        PathWithPosition {
            path: PathBuf::from("test_file.rs"),
            row: None,
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("test_file.rs:1:"),
        PathWithPosition {
            path: PathBuf::from("test_file.rs"),
            row: Some(1),
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("ab\ncd"),
        PathWithPosition {
            path: PathBuf::from("ab\ncd"),
            row: None,
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("👋\nab"),
        PathWithPosition {
            path: PathBuf::from("👋\nab"),
            row: None,
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("Types.hs:(617,9)-(670,28):"),
        PathWithPosition {
            path: PathBuf::from("Types.hs"),
            row: Some(617),
            column: Some(9),
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("main (1).log"),
        PathWithPosition {
            path: PathBuf::from("main (1).log"),
            row: None,
            column: None
        }
    );
}

#[perf]
#[cfg(not(target_os = "windows"))]
fn path_with_position_parse_posix_path_with_suffix() {
    assert_eq!(
        PathWithPosition::parse_str("foo/bar:34:in"),
        PathWithPosition {
            path: PathBuf::from("foo/bar"),
            row: Some(34),
            column: None,
        }
    );
    assert_eq!(
        PathWithPosition::parse_str("foo/bar.rs:1902:::15:"),
        PathWithPosition {
            path: PathBuf::from("foo/bar.rs:1902"),
            row: Some(15),
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("app-editors:mav-0.143.6:20240710-201212.log:34:"),
        PathWithPosition {
            path: PathBuf::from("app-editors:mav-0.143.6:20240710-201212.log"),
            row: Some(34),
            column: None,
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("crates/file_finder/src/file_finder.rs:1902:13:"),
        PathWithPosition {
            path: PathBuf::from("crates/file_finder/src/file_finder.rs"),
            row: Some(1902),
            column: Some(13),
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("crate/utils/src/test:today.log:34"),
        PathWithPosition {
            path: PathBuf::from("crate/utils/src/test:today.log"),
            row: Some(34),
            column: None,
        }
    );
    assert_eq!(
        PathWithPosition::parse_str("/testing/out/src/file_finder.odin(7:15)"),
        PathWithPosition {
            path: PathBuf::from("/testing/out/src/file_finder.odin"),
            row: Some(7),
            column: Some(15),
        }
    );
}

#[perf]
#[cfg(target_os = "windows")]
fn path_with_position_parse_windows_path() {
    assert_eq!(
        PathWithPosition::parse_str("crates\\utils\\paths.rs"),
        PathWithPosition {
            path: PathBuf::from("crates\\utils\\paths.rs"),
            row: None,
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("C:\\Users\\someone\\test_file.rs"),
        PathWithPosition {
            path: PathBuf::from("C:\\Users\\someone\\test_file.rs"),
            row: None,
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("C:\\Users\\someone\\main (1).log"),
        PathWithPosition {
            path: PathBuf::from("C:\\Users\\someone\\main (1).log"),
            row: None,
            column: None
        }
    );
}

#[perf]
#[cfg(target_os = "windows")]
fn path_with_position_parse_windows_path_with_suffix() {
    assert_eq!(
        PathWithPosition::parse_str("crates\\utils\\paths.rs:101"),
        PathWithPosition {
            path: PathBuf::from("crates\\utils\\paths.rs"),
            row: Some(101),
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("\\\\?\\C:\\Users\\someone\\test_file.rs:1:20"),
        PathWithPosition {
            path: PathBuf::from("\\\\?\\C:\\Users\\someone\\test_file.rs"),
            row: Some(1),
            column: Some(20)
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("C:\\Users\\someone\\test_file.rs(1902,13)"),
        PathWithPosition {
            path: PathBuf::from("C:\\Users\\someone\\test_file.rs"),
            row: Some(1902),
            column: Some(13)
        }
    );

    // Trim off trailing `:`s for otherwise valid input.
    assert_eq!(
        PathWithPosition::parse_str("\\\\?\\C:\\Users\\someone\\test_file.rs:1902:13:"),
        PathWithPosition {
            path: PathBuf::from("\\\\?\\C:\\Users\\someone\\test_file.rs"),
            row: Some(1902),
            column: Some(13)
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("\\\\?\\C:\\Users\\someone\\test_file.rs:1902:13:15:"),
        PathWithPosition {
            path: PathBuf::from("\\\\?\\C:\\Users\\someone\\test_file.rs:1902"),
            row: Some(13),
            column: Some(15)
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("\\\\?\\C:\\Users\\someone\\test_file.rs:1902:::15:"),
        PathWithPosition {
            path: PathBuf::from("\\\\?\\C:\\Users\\someone\\test_file.rs:1902"),
            row: Some(15),
            column: None
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("\\\\?\\C:\\Users\\someone\\test_file.rs(1902,13):"),
        PathWithPosition {
            path: PathBuf::from("\\\\?\\C:\\Users\\someone\\test_file.rs"),
            row: Some(1902),
            column: Some(13),
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("\\\\?\\C:\\Users\\someone\\test_file.rs(1902):"),
        PathWithPosition {
            path: PathBuf::from("\\\\?\\C:\\Users\\someone\\test_file.rs"),
            row: Some(1902),
            column: None,
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("C:\\Users\\someone\\test_file.rs:1902:13:"),
        PathWithPosition {
            path: PathBuf::from("C:\\Users\\someone\\test_file.rs"),
            row: Some(1902),
            column: Some(13),
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("C:\\Users\\someone\\test_file.rs(1902,13):"),
        PathWithPosition {
            path: PathBuf::from("C:\\Users\\someone\\test_file.rs"),
            row: Some(1902),
            column: Some(13),
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("C:\\Users\\someone\\test_file.rs(1902):"),
        PathWithPosition {
            path: PathBuf::from("C:\\Users\\someone\\test_file.rs"),
            row: Some(1902),
            column: None,
        }
    );

    assert_eq!(
        PathWithPosition::parse_str("crates/utils/paths.rs:101"),
        PathWithPosition {
            path: PathBuf::from("crates\\utils\\paths.rs"),
            row: Some(101),
            column: None,
        }
    );
}
