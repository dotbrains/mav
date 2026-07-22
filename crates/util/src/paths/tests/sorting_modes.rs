use super::*;

#[perf]
fn compare_rel_paths_mixed_case_insensitive() {
    // Test that mixed mode is case-insensitive
    let mut paths = vec![
        (RelPath::unix("zebra.txt").unwrap(), true),
        (RelPath::unix("Apple").unwrap(), false),
        (RelPath::unix("banana.rs").unwrap(), true),
        (RelPath::unix("Carrot").unwrap(), false),
        (RelPath::unix("aardvark.txt").unwrap(), true),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    // Case-insensitive: aardvark < Apple < banana < Carrot < zebra
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("aardvark.txt").unwrap(), true),
            (RelPath::unix("Apple").unwrap(), false),
            (RelPath::unix("banana.rs").unwrap(), true),
            (RelPath::unix("Carrot").unwrap(), false),
            (RelPath::unix("zebra.txt").unwrap(), true),
        ]
    );
}

#[perf]
fn compare_rel_paths_files_first_basic() {
    // Test that files come before directories
    let mut paths = vec![
        (RelPath::unix("zebra.txt").unwrap(), true),
        (RelPath::unix("Apple").unwrap(), false),
        (RelPath::unix("banana.rs").unwrap(), true),
        (RelPath::unix("Carrot").unwrap(), false),
        (RelPath::unix("aardvark.txt").unwrap(), true),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::FilesFirst, SortOrder::Default));
    // Files first (case-insensitive), then directories (case-insensitive)
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("aardvark.txt").unwrap(), true),
            (RelPath::unix("banana.rs").unwrap(), true),
            (RelPath::unix("zebra.txt").unwrap(), true),
            (RelPath::unix("Apple").unwrap(), false),
            (RelPath::unix("Carrot").unwrap(), false),
        ]
    );
}

#[perf]
fn compare_rel_paths_files_first_case_insensitive() {
    // Test case-insensitive sorting within files and directories
    let mut paths = vec![
        (RelPath::unix("Zebra.txt").unwrap(), true),
        (RelPath::unix("apple").unwrap(), false),
        (RelPath::unix("Banana.rs").unwrap(), true),
        (RelPath::unix("carrot").unwrap(), false),
        (RelPath::unix("Aardvark.txt").unwrap(), true),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::FilesFirst, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("Aardvark.txt").unwrap(), true),
            (RelPath::unix("Banana.rs").unwrap(), true),
            (RelPath::unix("Zebra.txt").unwrap(), true),
            (RelPath::unix("apple").unwrap(), false),
            (RelPath::unix("carrot").unwrap(), false),
        ]
    );
}

#[perf]
fn compare_rel_paths_files_first_numeric() {
    // Test natural number sorting with files first
    let mut paths = vec![
        (RelPath::unix("file10.txt").unwrap(), true),
        (RelPath::unix("dir2").unwrap(), false),
        (RelPath::unix("file2.txt").unwrap(), true),
        (RelPath::unix("dir10").unwrap(), false),
        (RelPath::unix("file1.txt").unwrap(), true),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::FilesFirst, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("file1.txt").unwrap(), true),
            (RelPath::unix("file2.txt").unwrap(), true),
            (RelPath::unix("file10.txt").unwrap(), true),
            (RelPath::unix("dir2").unwrap(), false),
            (RelPath::unix("dir10").unwrap(), false),
        ]
    );
}

#[perf]
fn compare_rel_paths_mixed_case() {
    // Test case-insensitive sorting with varied capitalization
    let mut paths = vec![
        (RelPath::unix("README.md").unwrap(), true),
        (RelPath::unix("readme.txt").unwrap(), true),
        (RelPath::unix("ReadMe.rs").unwrap(), true),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    // All "readme" variants should group together, sorted by extension
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("README.md").unwrap(), true),
            (RelPath::unix("ReadMe.rs").unwrap(), true),
            (RelPath::unix("readme.txt").unwrap(), true),
        ]
    );
}

#[perf]
fn compare_rel_paths_mixed_files_and_dirs() {
    // Verify directories and files are still mixed
    let mut paths = vec![
        (RelPath::unix("file2.txt").unwrap(), true),
        (RelPath::unix("Dir1").unwrap(), false),
        (RelPath::unix("file1.txt").unwrap(), true),
        (RelPath::unix("dir2").unwrap(), false),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    // Case-insensitive: dir1, dir2, file1, file2 (all mixed)
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("Dir1").unwrap(), false),
            (RelPath::unix("dir2").unwrap(), false),
            (RelPath::unix("file1.txt").unwrap(), true),
            (RelPath::unix("file2.txt").unwrap(), true),
        ]
    );
}

#[perf]
fn compare_rel_paths_mixed_same_name_different_case_file_and_dir() {
    let mut paths = vec![
        (RelPath::unix("Hello.txt").unwrap(), true),
        (RelPath::unix("hello").unwrap(), false),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("hello").unwrap(), false),
            (RelPath::unix("Hello.txt").unwrap(), true),
        ]
    );

    let mut paths = vec![
        (RelPath::unix("hello").unwrap(), false),
        (RelPath::unix("Hello.txt").unwrap(), true),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("hello").unwrap(), false),
            (RelPath::unix("Hello.txt").unwrap(), true),
        ]
    );
}

#[perf]
fn compare_rel_paths_mixed_with_nested_paths() {
    // Test that nested paths still work correctly
    let mut paths = vec![
        (RelPath::unix("src/main.rs").unwrap(), true),
        (RelPath::unix("Cargo.toml").unwrap(), true),
        (RelPath::unix("src").unwrap(), false),
        (RelPath::unix("target").unwrap(), false),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("Cargo.toml").unwrap(), true),
            (RelPath::unix("src").unwrap(), false),
            (RelPath::unix("src/main.rs").unwrap(), true),
            (RelPath::unix("target").unwrap(), false),
        ]
    );
}

#[perf]
fn compare_rel_paths_files_first_with_nested() {
    // Files come before directories, even with nested paths
    let mut paths = vec![
        (RelPath::unix("src/lib.rs").unwrap(), true),
        (RelPath::unix("README.md").unwrap(), true),
        (RelPath::unix("src").unwrap(), false),
        (RelPath::unix("tests").unwrap(), false),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::FilesFirst, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("README.md").unwrap(), true),
            (RelPath::unix("src").unwrap(), false),
            (RelPath::unix("src/lib.rs").unwrap(), true),
            (RelPath::unix("tests").unwrap(), false),
        ]
    );
}

#[perf]
fn compare_rel_paths_mixed_dotfiles() {
    // Test that dotfiles are handled correctly in mixed mode
    let mut paths = vec![
        (RelPath::unix(".gitignore").unwrap(), true),
        (RelPath::unix("README.md").unwrap(), true),
        (RelPath::unix(".github").unwrap(), false),
        (RelPath::unix("src").unwrap(), false),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix(".github").unwrap(), false),
            (RelPath::unix(".gitignore").unwrap(), true),
            (RelPath::unix("README.md").unwrap(), true),
            (RelPath::unix("src").unwrap(), false),
        ]
    );
}

#[perf]
fn compare_rel_paths_files_first_dotfiles() {
    // Test that dotfiles come first when they're files
    let mut paths = vec![
        (RelPath::unix(".gitignore").unwrap(), true),
        (RelPath::unix("README.md").unwrap(), true),
        (RelPath::unix(".github").unwrap(), false),
        (RelPath::unix("src").unwrap(), false),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::FilesFirst, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix(".gitignore").unwrap(), true),
            (RelPath::unix("README.md").unwrap(), true),
            (RelPath::unix(".github").unwrap(), false),
            (RelPath::unix("src").unwrap(), false),
        ]
    );
}

#[perf]
fn compare_rel_paths_mixed_same_stem_different_extension() {
    // Files with same stem but different extensions should sort by extension
    let mut paths = vec![
        (RelPath::unix("file.rs").unwrap(), true),
        (RelPath::unix("file.md").unwrap(), true),
        (RelPath::unix("file.txt").unwrap(), true),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("file.md").unwrap(), true),
            (RelPath::unix("file.rs").unwrap(), true),
            (RelPath::unix("file.txt").unwrap(), true),
        ]
    );
}

#[perf]
fn compare_rel_paths_files_first_same_stem() {
    // Same stem files should still sort by extension with files_first
    let mut paths = vec![
        (RelPath::unix("main.rs").unwrap(), true),
        (RelPath::unix("main.c").unwrap(), true),
        (RelPath::unix("main").unwrap(), false),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::FilesFirst, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("main.c").unwrap(), true),
            (RelPath::unix("main.rs").unwrap(), true),
            (RelPath::unix("main").unwrap(), false),
        ]
    );
}

#[perf]
fn compare_rel_paths_mixed_deep_nesting() {
    // Test sorting with deeply nested paths
    let mut paths = vec![
        (RelPath::unix("a/b/c.txt").unwrap(), true),
        (RelPath::unix("A/B.txt").unwrap(), true),
        (RelPath::unix("a.txt").unwrap(), true),
        (RelPath::unix("A.txt").unwrap(), true),
    ];
    paths.sort_by(|&a, &b| compare_rel_paths_by(a, b, SortMode::Mixed, SortOrder::Default));
    assert_eq!(
        paths,
        vec![
            (RelPath::unix("a/b/c.txt").unwrap(), true),
            (RelPath::unix("A/B.txt").unwrap(), true),
            (RelPath::unix("a.txt").unwrap(), true),
            (RelPath::unix("A.txt").unwrap(), true),
        ]
    );
}
