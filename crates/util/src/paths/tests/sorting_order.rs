use super::*;

#[perf]
fn compare_rel_paths_upper() {
    let directories_only_paths = vec![
        rel_path_entry("mixedCase", false),
        rel_path_entry("Zebra", false),
        rel_path_entry("banana", false),
        rel_path_entry("ALLCAPS", false),
        rel_path_entry("Apple", false),
        rel_path_entry("dog", false),
        rel_path_entry(".hidden", false),
        rel_path_entry("Carrot", false),
    ];
    assert_eq!(
        sorted_rel_paths(
            directories_only_paths,
            SortMode::DirectoriesFirst,
            SortOrder::Upper,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple", false),
            rel_path_entry("Carrot", false),
            rel_path_entry("Zebra", false),
            rel_path_entry("banana", false),
            rel_path_entry("dog", false),
            rel_path_entry("mixedCase", false),
        ]
    );

    let file_and_directory_paths = vec![
        rel_path_entry("banana", false),
        rel_path_entry("Apple.txt", true),
        rel_path_entry("dog.md", true),
        rel_path_entry("ALLCAPS", false),
        rel_path_entry("file1.txt", true),
        rel_path_entry("File2.txt", true),
        rel_path_entry(".hidden", false),
    ];
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths.clone(),
            SortMode::DirectoriesFirst,
            SortOrder::Upper,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("banana", false),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
        ]
    );
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths.clone(),
            SortMode::Mixed,
            SortOrder::Upper,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry("banana", false),
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
        ]
    );
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths,
            SortMode::FilesFirst,
            SortOrder::Upper,
        ),
        vec![
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
            rel_path_entry(".hidden", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("banana", false),
        ]
    );

    let natural_sort_paths = vec![
        rel_path_entry("file10.txt", true),
        rel_path_entry("file1.txt", true),
        rel_path_entry("file20.txt", true),
        rel_path_entry("file2.txt", true),
    ];
    assert_eq!(
        sorted_rel_paths(natural_sort_paths, SortMode::Mixed, SortOrder::Upper,),
        vec![
            rel_path_entry("file1.txt", true),
            rel_path_entry("file2.txt", true),
            rel_path_entry("file10.txt", true),
            rel_path_entry("file20.txt", true),
        ]
    );

    let accented_paths = vec![
        rel_path_entry("\u{00C9}something.txt", true),
        rel_path_entry("zebra.txt", true),
        rel_path_entry("Apple.txt", true),
    ];
    assert_eq!(
        sorted_rel_paths(accented_paths, SortMode::Mixed, SortOrder::Upper),
        vec![
            rel_path_entry("Apple.txt", true),
            rel_path_entry("\u{00C9}something.txt", true),
            rel_path_entry("zebra.txt", true),
        ]
    );
}

#[perf]
fn compare_rel_paths_lower() {
    let directories_only_paths = vec![
        rel_path_entry("mixedCase", false),
        rel_path_entry("Zebra", false),
        rel_path_entry("banana", false),
        rel_path_entry("ALLCAPS", false),
        rel_path_entry("Apple", false),
        rel_path_entry("dog", false),
        rel_path_entry(".hidden", false),
        rel_path_entry("Carrot", false),
    ];
    assert_eq!(
        sorted_rel_paths(
            directories_only_paths,
            SortMode::DirectoriesFirst,
            SortOrder::Lower,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("banana", false),
            rel_path_entry("dog", false),
            rel_path_entry("mixedCase", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple", false),
            rel_path_entry("Carrot", false),
            rel_path_entry("Zebra", false),
        ]
    );

    let file_and_directory_paths = vec![
        rel_path_entry("banana", false),
        rel_path_entry("Apple.txt", true),
        rel_path_entry("dog.md", true),
        rel_path_entry("ALLCAPS", false),
        rel_path_entry("file1.txt", true),
        rel_path_entry("File2.txt", true),
        rel_path_entry(".hidden", false),
    ];
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths.clone(),
            SortMode::DirectoriesFirst,
            SortOrder::Lower,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("banana", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
        ]
    );
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths.clone(),
            SortMode::Mixed,
            SortOrder::Lower,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("banana", false),
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
        ]
    );
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths,
            SortMode::FilesFirst,
            SortOrder::Lower,
        ),
        vec![
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry(".hidden", false),
            rel_path_entry("banana", false),
            rel_path_entry("ALLCAPS", false),
        ]
    );
}

#[perf]
fn compare_rel_paths_unicode() {
    let directories_only_paths = vec![
        rel_path_entry("mixedCase", false),
        rel_path_entry("Zebra", false),
        rel_path_entry("banana", false),
        rel_path_entry("ALLCAPS", false),
        rel_path_entry("Apple", false),
        rel_path_entry("dog", false),
        rel_path_entry(".hidden", false),
        rel_path_entry("Carrot", false),
    ];
    assert_eq!(
        sorted_rel_paths(
            directories_only_paths,
            SortMode::DirectoriesFirst,
            SortOrder::Unicode,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple", false),
            rel_path_entry("Carrot", false),
            rel_path_entry("Zebra", false),
            rel_path_entry("banana", false),
            rel_path_entry("dog", false),
            rel_path_entry("mixedCase", false),
        ]
    );

    let file_and_directory_paths = vec![
        rel_path_entry("banana", false),
        rel_path_entry("Apple.txt", true),
        rel_path_entry("dog.md", true),
        rel_path_entry("ALLCAPS", false),
        rel_path_entry("file1.txt", true),
        rel_path_entry("File2.txt", true),
        rel_path_entry(".hidden", false),
    ];
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths.clone(),
            SortMode::DirectoriesFirst,
            SortOrder::Unicode,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("banana", false),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
        ]
    );
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths.clone(),
            SortMode::Mixed,
            SortOrder::Unicode,
        ),
        vec![
            rel_path_entry(".hidden", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry("banana", false),
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
        ]
    );
    assert_eq!(
        sorted_rel_paths(
            file_and_directory_paths,
            SortMode::FilesFirst,
            SortOrder::Unicode,
        ),
        vec![
            rel_path_entry("Apple.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry("dog.md", true),
            rel_path_entry("file1.txt", true),
            rel_path_entry(".hidden", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("banana", false),
        ]
    );

    let numeric_paths = vec![
        rel_path_entry("file10.txt", true),
        rel_path_entry("file1.txt", true),
        rel_path_entry("file2.txt", true),
        rel_path_entry("file20.txt", true),
    ];
    assert_eq!(
        sorted_rel_paths(numeric_paths, SortMode::Mixed, SortOrder::Unicode,),
        vec![
            rel_path_entry("file1.txt", true),
            rel_path_entry("file10.txt", true),
            rel_path_entry("file2.txt", true),
            rel_path_entry("file20.txt", true),
        ]
    );

    let accented_paths = vec![
        rel_path_entry("\u{00C9}something.txt", true),
        rel_path_entry("zebra.txt", true),
        rel_path_entry("Apple.txt", true),
    ];
    assert_eq!(
        sorted_rel_paths(accented_paths, SortMode::Mixed, SortOrder::Unicode),
        vec![
            rel_path_entry("Apple.txt", true),
            rel_path_entry("zebra.txt", true),
            rel_path_entry("\u{00C9}something.txt", true),
        ]
    );
}
