use super::*;

#[test]
fn test_filter_edit_history_by_path() {
    // Test that filter_edit_history_by_path correctly matches paths when
    // the edit history has paths with a repo prefix (e.g., "repo/src/file.rs")
    // but the cursor_path doesn't have the repo prefix (e.g., "src/file.rs")
    let events = vec![
        Arc::new(zeta_prompt::Event::BufferChange {
            path: Arc::from(Path::new("myrepo/src/file.rs")),
            old_path: Arc::from(Path::new("myrepo/src/file.rs")),
            diff: indoc! {"
                     @@ -1 +1 @@
                     -old
                     +new"}
            .into(),
            old_range: 0..0,
            new_range: 0..0,
            predicted: false,
            in_open_source_repo: true,
        }),
        Arc::new(zeta_prompt::Event::BufferChange {
            path: Arc::from(Path::new("myrepo/other.rs")),
            old_path: Arc::from(Path::new("myrepo/other.rs")),
            diff: indoc! {"
                     @@ -1 +1 @@
                     -a
                     +b"}
            .into(),
            old_range: 0..0,
            new_range: 0..0,
            predicted: false,
            in_open_source_repo: true,
        }),
        Arc::new(zeta_prompt::Event::BufferChange {
            path: Arc::from(Path::new("src/file.rs")),
            old_path: Arc::from(Path::new("src/file.rs")),
            diff: indoc! {"
                     @@ -1 +1 @@
                     -x
                     +y"}
            .into(),
            old_range: 0..0,
            new_range: 0..0,
            predicted: false,
            in_open_source_repo: true,
        }),
    ];

    // "myrepo/src/file.rs" stripped -> "src/file.rs" matches cursor_path
    // "src/file.rs" exact match
    let cursor_path = Path::new("src/file.rs");
    let filtered = filter_edit_history_by_path(&events, cursor_path);
    assert_eq!(
        filtered.len(),
        2,
        "Should match myrepo/src/file.rs (stripped) and src/file.rs (exact)"
    );

    // "myrepo/src/file.rs" stripped -> "src/file.rs" != "file.rs"
    // "src/file.rs" stripped -> "file.rs" == "file.rs"
    let cursor_path = Path::new("file.rs");
    let filtered = filter_edit_history_by_path(&events, cursor_path);
    assert_eq!(
        filtered.len(),
        1,
        "Should only match src/file.rs (stripped to file.rs)"
    );

    // "myrepo/other.rs" stripped -> "other.rs" == "other.rs"
    let cursor_path = Path::new("other.rs");
    let filtered = filter_edit_history_by_path(&events, cursor_path);
    assert_eq!(filtered.len(), 1, "Should match only myrepo/other.rs");
}

#[test]
fn test_reverse_diff_preserves_trailing_newline() {
    let diff_with_trailing_newline = indoc! {"
             --- a/file
             +++ b/file
             @@ -1 +1 @@
             -old
             +new
         "};
    let reversed = reverse_diff(diff_with_trailing_newline);
    assert!(
        reversed.ends_with('\n'),
        "Reversed diff should preserve trailing newline"
    );

    let diff_without_trailing_newline = indoc! {"
             --- a/file
             +++ b/file
             @@ -1 +1 @@
             -old
             +new"};
    let reversed = reverse_diff(diff_without_trailing_newline);
    assert!(
        !reversed.ends_with('\n'),
        "Reversed diff should not add trailing newline if original didn't have one"
    );
}
