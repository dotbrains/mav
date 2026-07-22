use super::*;

#[perf]
fn test_compare_numeric_segments() {
    // Helper function to create peekable iterators and test
    fn compare(a: &str, b: &str) -> Ordering {
        let mut a_iter = a.chars().peekable();
        let mut b_iter = b.chars().peekable();

        let result = compare_numeric_segments(&mut a_iter, &mut b_iter);

        // Verify iterators advanced correctly
        assert!(
            !a_iter.next().is_some_and(|c| c.is_ascii_digit()),
            "Iterator a should have consumed all digits"
        );
        assert!(
            !b_iter.next().is_some_and(|c| c.is_ascii_digit()),
            "Iterator b should have consumed all digits"
        );

        result
    }

    // Basic numeric comparisons
    assert_eq!(compare("0", "0"), Ordering::Equal);
    assert_eq!(compare("1", "2"), Ordering::Less);
    assert_eq!(compare("9", "10"), Ordering::Less);
    assert_eq!(compare("10", "9"), Ordering::Greater);
    assert_eq!(compare("99", "100"), Ordering::Less);

    // Leading zeros
    assert_eq!(compare("0", "00"), Ordering::Less);
    assert_eq!(compare("00", "0"), Ordering::Greater);
    assert_eq!(compare("01", "1"), Ordering::Greater);
    assert_eq!(compare("001", "1"), Ordering::Greater);
    assert_eq!(compare("001", "01"), Ordering::Greater);

    // Same value different representation
    assert_eq!(compare("000100", "100"), Ordering::Greater);
    assert_eq!(compare("100", "0100"), Ordering::Less);
    assert_eq!(compare("0100", "00100"), Ordering::Less);

    // Large numbers
    assert_eq!(compare("9999999999", "10000000000"), Ordering::Less);
    assert_eq!(
        compare(
            "340282366920938463463374607431768211455", // u128::MAX
            "340282366920938463463374607431768211456"
        ),
        Ordering::Less
    );
    assert_eq!(
        compare(
            "340282366920938463463374607431768211456", // > u128::MAX
            "340282366920938463463374607431768211455"
        ),
        Ordering::Greater
    );

    // Iterator advancement verification
    let mut a_iter = "123abc".chars().peekable();
    let mut b_iter = "456def".chars().peekable();

    compare_numeric_segments(&mut a_iter, &mut b_iter);

    assert_eq!(a_iter.collect::<String>(), "abc");
    assert_eq!(b_iter.collect::<String>(), "def");
}

#[perf]
fn test_natural_sort() {
    // Basic alphanumeric
    assert_eq!(natural_sort("a", "b"), Ordering::Less);
    assert_eq!(natural_sort("b", "a"), Ordering::Greater);
    assert_eq!(natural_sort("a", "a"), Ordering::Equal);

    // Case sensitivity
    assert_eq!(natural_sort("a", "A"), Ordering::Less);
    assert_eq!(natural_sort("A", "a"), Ordering::Greater);
    assert_eq!(natural_sort("aA", "aa"), Ordering::Greater);
    assert_eq!(natural_sort("aa", "aA"), Ordering::Less);

    // Numbers
    assert_eq!(natural_sort("1", "2"), Ordering::Less);
    assert_eq!(natural_sort("2", "10"), Ordering::Less);
    assert_eq!(natural_sort("02", "10"), Ordering::Less);
    assert_eq!(natural_sort("02", "2"), Ordering::Greater);

    // Mixed alphanumeric
    assert_eq!(natural_sort("a1", "a2"), Ordering::Less);
    assert_eq!(natural_sort("a2", "a10"), Ordering::Less);
    assert_eq!(natural_sort("a02", "a2"), Ordering::Greater);
    assert_eq!(natural_sort("a1b", "a1c"), Ordering::Less);

    // Multiple numeric segments
    assert_eq!(natural_sort("1a2", "1a10"), Ordering::Less);
    assert_eq!(natural_sort("1a10", "1a2"), Ordering::Greater);
    assert_eq!(natural_sort("2a1", "10a1"), Ordering::Less);

    // Special characters
    assert_eq!(natural_sort("a-1", "a-2"), Ordering::Less);
    assert_eq!(natural_sort("a_1", "a_2"), Ordering::Less);
    assert_eq!(natural_sort("a.1", "a.2"), Ordering::Less);

    // Unicode
    assert_eq!(natural_sort("文1", "文2"), Ordering::Less);
    assert_eq!(natural_sort("文2", "文10"), Ordering::Less);
    assert_eq!(natural_sort("🔤1", "🔤2"), Ordering::Less);

    // Empty and special cases
    assert_eq!(natural_sort("", ""), Ordering::Equal);
    assert_eq!(natural_sort("", "a"), Ordering::Less);
    assert_eq!(natural_sort("a", ""), Ordering::Greater);
    assert_eq!(natural_sort(" ", "  "), Ordering::Less);

    // Mixed everything
    assert_eq!(natural_sort("File-1.txt", "File-2.txt"), Ordering::Less);
    assert_eq!(natural_sort("File-02.txt", "File-2.txt"), Ordering::Greater);
    assert_eq!(natural_sort("File-2.txt", "File-10.txt"), Ordering::Less);
    assert_eq!(natural_sort("File_A1", "File_A2"), Ordering::Less);
    assert_eq!(natural_sort("File_a1", "File_A1"), Ordering::Less);
}

#[perf]
fn test_compare_paths() {
    // Helper function for cleaner tests
    fn compare(a: &str, is_a_file: bool, b: &str, is_b_file: bool) -> Ordering {
        compare_paths((Path::new(a), is_a_file), (Path::new(b), is_b_file))
    }

    // Basic path comparison
    assert_eq!(compare("a", true, "b", true), Ordering::Less);
    assert_eq!(compare("b", true, "a", true), Ordering::Greater);
    assert_eq!(compare("a", true, "a", true), Ordering::Equal);

    // Files vs Directories
    assert_eq!(compare("a", true, "a", false), Ordering::Greater);
    assert_eq!(compare("a", false, "a", true), Ordering::Less);
    assert_eq!(compare("b", false, "a", true), Ordering::Less);

    // Extensions
    assert_eq!(compare("a.txt", true, "a.md", true), Ordering::Greater);
    assert_eq!(compare("a.md", true, "a.txt", true), Ordering::Less);
    assert_eq!(compare("a", true, "a.txt", true), Ordering::Less);

    // Nested paths
    assert_eq!(compare("dir/a", true, "dir/b", true), Ordering::Less);
    assert_eq!(compare("dir1/a", true, "dir2/a", true), Ordering::Less);
    assert_eq!(compare("dir/sub/a", true, "dir/a", true), Ordering::Less);

    // Case sensitivity in paths
    assert_eq!(
        compare("Dir/file", true, "dir/file", true),
        Ordering::Greater
    );
    assert_eq!(
        compare("dir/File", true, "dir/file", true),
        Ordering::Greater
    );
    assert_eq!(compare("dir/file", true, "Dir/File", true), Ordering::Less);

    // Hidden files and special names
    assert_eq!(compare(".hidden", true, "visible", true), Ordering::Less);
    assert_eq!(compare("_special", true, "normal", true), Ordering::Less);
    assert_eq!(compare(".config", false, ".data", false), Ordering::Less);

    // Mixed numeric paths
    assert_eq!(
        compare("dir1/file", true, "dir2/file", true),
        Ordering::Less
    );
    assert_eq!(
        compare("dir2/file", true, "dir10/file", true),
        Ordering::Less
    );
    assert_eq!(
        compare("dir02/file", true, "dir2/file", true),
        Ordering::Greater
    );

    // Root paths
    assert_eq!(compare("/a", true, "/b", true), Ordering::Less);
    assert_eq!(compare("/", false, "/a", true), Ordering::Less);

    // Complex real-world examples
    assert_eq!(
        compare("project/src/main.rs", true, "project/src/lib.rs", true),
        Ordering::Greater
    );
    assert_eq!(
        compare(
            "project/tests/test_1.rs",
            true,
            "project/tests/test_2.rs",
            true
        ),
        Ordering::Less
    );
    assert_eq!(
        compare(
            "project/v1.0.0/README.md",
            true,
            "project/v1.10.0/README.md",
            true
        ),
        Ordering::Less
    );
}

#[perf]
fn test_natural_sort_case_sensitivity() {
    std::thread::sleep(std::time::Duration::from_millis(100));
    // Same letter different case - lowercase should come first
    assert_eq!(natural_sort("a", "A"), Ordering::Less);
    assert_eq!(natural_sort("A", "a"), Ordering::Greater);
    assert_eq!(natural_sort("a", "a"), Ordering::Equal);
    assert_eq!(natural_sort("A", "A"), Ordering::Equal);

    // Mixed case strings
    assert_eq!(natural_sort("aaa", "AAA"), Ordering::Less);
    assert_eq!(natural_sort("AAA", "aaa"), Ordering::Greater);
    assert_eq!(natural_sort("aAa", "AaA"), Ordering::Less);

    // Different letters
    assert_eq!(natural_sort("a", "b"), Ordering::Less);
    assert_eq!(natural_sort("A", "b"), Ordering::Less);
    assert_eq!(natural_sort("a", "B"), Ordering::Less);
}

#[perf]
fn test_natural_sort_with_numbers() {
    // Basic number ordering
    assert_eq!(natural_sort("file1", "file2"), Ordering::Less);
    assert_eq!(natural_sort("file2", "file10"), Ordering::Less);
    assert_eq!(natural_sort("file10", "file2"), Ordering::Greater);

    // Numbers in different positions
    assert_eq!(natural_sort("1file", "2file"), Ordering::Less);
    assert_eq!(natural_sort("file1text", "file2text"), Ordering::Less);
    assert_eq!(natural_sort("text1file", "text2file"), Ordering::Less);

    // Multiple numbers in string
    assert_eq!(natural_sort("file1-2", "file1-10"), Ordering::Less);
    assert_eq!(natural_sort("2-1file", "10-1file"), Ordering::Less);

    // Leading zeros
    assert_eq!(natural_sort("file002", "file2"), Ordering::Greater);
    assert_eq!(natural_sort("file002", "file10"), Ordering::Less);

    // Very large numbers
    assert_eq!(
        natural_sort("file999999999999999999999", "file999999999999999999998"),
        Ordering::Greater
    );

    // u128 edge cases

    // Numbers near u128::MAX (340,282,366,920,938,463,463,374,607,431,768,211,455)
    assert_eq!(
        natural_sort(
            "file340282366920938463463374607431768211454",
            "file340282366920938463463374607431768211455"
        ),
        Ordering::Less
    );

    // Equal length numbers that overflow u128
    assert_eq!(
        natural_sort(
            "file340282366920938463463374607431768211456",
            "file340282366920938463463374607431768211455"
        ),
        Ordering::Greater
    );

    // Different length numbers that overflow u128
    assert_eq!(
        natural_sort(
            "file3402823669209384634633746074317682114560",
            "file340282366920938463463374607431768211455"
        ),
        Ordering::Greater
    );

    // Leading zeros with numbers near u128::MAX
    assert_eq!(
        natural_sort(
            "file0340282366920938463463374607431768211455",
            "file340282366920938463463374607431768211455"
        ),
        Ordering::Greater
    );

    // Very large numbers with different lengths (both overflow u128)
    assert_eq!(
        natural_sort(
            "file999999999999999999999999999999999999999999999999",
            "file9999999999999999999999999999999999999999999999999"
        ),
        Ordering::Less
    );
}

#[perf]
fn test_natural_sort_case_sensitive() {
    // Numerically smaller values come first.
    assert_eq!(natural_sort("File1", "file2"), Ordering::Less);
    assert_eq!(natural_sort("file1", "File2"), Ordering::Less);

    // Numerically equal values: the case-insensitive comparison decides first.
    // Case-sensitive comparison only occurs when both are equal case-insensitively.
    assert_eq!(natural_sort("Dir1", "dir01"), Ordering::Less);
    assert_eq!(natural_sort("dir2", "Dir02"), Ordering::Less);
    assert_eq!(natural_sort("dir2", "dir02"), Ordering::Less);

    // Numerically equal and case-insensitively equal:
    // the lexicographically smaller (case-sensitive) one wins.
    assert_eq!(natural_sort("dir1", "Dir1"), Ordering::Less);
    assert_eq!(natural_sort("dir02", "Dir02"), Ordering::Less);
    assert_eq!(natural_sort("dir10", "Dir10"), Ordering::Less);
}

#[perf]
fn test_natural_sort_edge_cases() {
    // Empty strings
    assert_eq!(natural_sort("", ""), Ordering::Equal);
    assert_eq!(natural_sort("", "a"), Ordering::Less);
    assert_eq!(natural_sort("a", ""), Ordering::Greater);

    // Special characters
    assert_eq!(natural_sort("file-1", "file_1"), Ordering::Less);
    assert_eq!(natural_sort("file.1", "file_1"), Ordering::Less);
    assert_eq!(natural_sort("file 1", "file_1"), Ordering::Less);

    // Unicode characters
    // 9312 vs 9313
    assert_eq!(natural_sort("file①", "file②"), Ordering::Less);
    // 9321 vs 9313
    assert_eq!(natural_sort("file⑩", "file②"), Ordering::Greater);
    // 28450 vs 23383
    assert_eq!(natural_sort("file漢", "file字"), Ordering::Greater);

    // Mixed alphanumeric with special chars
    assert_eq!(natural_sort("file-1a", "file-1b"), Ordering::Less);
    assert_eq!(natural_sort("file-1.2", "file-1.10"), Ordering::Less);
    assert_eq!(natural_sort("file-1.10", "file-1.2"), Ordering::Greater);
}
