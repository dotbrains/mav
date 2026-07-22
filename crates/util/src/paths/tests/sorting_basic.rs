use super::*;

pub(crate) fn rel_path_entry(path: &'static str, is_file: bool) -> (&'static RelPath, bool) {
    (RelPath::unix(path).unwrap(), is_file)
}

pub(crate) fn sorted_rel_paths(
    mut paths: Vec<(&'static RelPath, bool)>,
    mode: SortMode,
    order: SortOrder,
) -> Vec<(&'static RelPath, bool)> {
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, mode, order));
    paths
}

#[perf]
fn compare_paths_with_dots() {
    let mut paths = vec![
        (Path::new("test_dirs"), false),
        (Path::new("test_dirs/1.46"), false),
        (Path::new("test_dirs/1.46/bar_1"), true),
        (Path::new("test_dirs/1.46/bar_2"), true),
        (Path::new("test_dirs/1.45"), false),
        (Path::new("test_dirs/1.45/foo_2"), true),
        (Path::new("test_dirs/1.45/foo_1"), true),
    ];
    paths.sort_by(|&a, &b| compare_paths(a, b));
    assert_eq!(
        paths,
        vec![
            (Path::new("test_dirs"), false),
            (Path::new("test_dirs/1.45"), false),
            (Path::new("test_dirs/1.45/foo_1"), true),
            (Path::new("test_dirs/1.45/foo_2"), true),
            (Path::new("test_dirs/1.46"), false),
            (Path::new("test_dirs/1.46/bar_1"), true),
            (Path::new("test_dirs/1.46/bar_2"), true),
        ]
    );
    let mut paths = vec![
        (Path::new("root1/one.txt"), true),
        (Path::new("root1/one.two.txt"), true),
    ];
    paths.sort_by(|&a, &b| compare_paths(a, b));
    assert_eq!(
        paths,
        vec![
            (Path::new("root1/one.txt"), true),
            (Path::new("root1/one.two.txt"), true),
        ]
    );
}

#[perf]
fn compare_paths_with_same_name_different_extensions() {
    let mut paths = vec![
        (Path::new("test_dirs/file.rs"), true),
        (Path::new("test_dirs/file.txt"), true),
        (Path::new("test_dirs/file.md"), true),
        (Path::new("test_dirs/file"), true),
        (Path::new("test_dirs/file.a"), true),
    ];
    paths.sort_by(|&a, &b| compare_paths(a, b));
    assert_eq!(
        paths,
        vec![
            (Path::new("test_dirs/file"), true),
            (Path::new("test_dirs/file.a"), true),
            (Path::new("test_dirs/file.md"), true),
            (Path::new("test_dirs/file.rs"), true),
            (Path::new("test_dirs/file.txt"), true),
        ]
    );
}

#[perf]
fn compare_paths_case_semi_sensitive() {
    let mut paths = vec![
        (Path::new("test_DIRS"), false),
        (Path::new("test_DIRS/foo_1"), true),
        (Path::new("test_DIRS/foo_2"), true),
        (Path::new("test_DIRS/bar"), true),
        (Path::new("test_DIRS/BAR"), true),
        (Path::new("test_dirs"), false),
        (Path::new("test_dirs/foo_1"), true),
        (Path::new("test_dirs/foo_2"), true),
        (Path::new("test_dirs/bar"), true),
        (Path::new("test_dirs/BAR"), true),
    ];
    paths.sort_by(|&a, &b| compare_paths(a, b));
    assert_eq!(
        paths,
        vec![
            (Path::new("test_dirs"), false),
            (Path::new("test_dirs/bar"), true),
            (Path::new("test_dirs/BAR"), true),
            (Path::new("test_dirs/foo_1"), true),
            (Path::new("test_dirs/foo_2"), true),
            (Path::new("test_DIRS"), false),
            (Path::new("test_DIRS/bar"), true),
            (Path::new("test_DIRS/BAR"), true),
            (Path::new("test_DIRS/foo_1"), true),
            (Path::new("test_DIRS/foo_2"), true),
        ]
    );
}

#[perf]
fn compare_paths_mixed_case_numeric_ordering() {
    let mut entries = [
        (Path::new(".config"), false),
        (Path::new("Dir1"), false),
        (Path::new("dir01"), false),
        (Path::new("dir2"), false),
        (Path::new("Dir02"), false),
        (Path::new("dir10"), false),
        (Path::new("Dir10"), false),
    ];

    entries.sort_by(|&a, &b| compare_paths(a, b));

    let ordered: Vec<&str> = entries
        .iter()
        .map(|(path, _)| path.to_str().unwrap())
        .collect();

    assert_eq!(
        ordered,
        vec![
            ".config", "Dir1", "dir01", "dir2", "Dir02", "dir10", "Dir10"
        ]
    );
}
