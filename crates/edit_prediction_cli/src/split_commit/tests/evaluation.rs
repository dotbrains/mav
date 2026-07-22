use super::*;

#[test]
fn test_generate_evaluation_example() {
    let commit = r#"commit abc123
Author: Test <test@example.com>
Date: Mon Jan 1 00:00:00 2024

    Test commit

////////////////////////////////////////////////////////////////////////////////
// Add greeting
////////////////////////////////////////////////////////////////////////////////
--- a/test.rs
+++ b/test.rs
@@ -1,3 +1,5 @@
 fn main() {
+    println!("hello");
+    println!("world");
 }
"#;

    let result = generate_evaluation_example_from_ordered_commit(
        commit,
        "https://github.com/test/repo",
        "abc123",
        Some(SplitPoint::Fraction(0.5)),
        Some(42),
        None,
    );

    assert!(result.is_ok());
    let case = result.unwrap();
    assert_eq!(case.repository_url, "https://github.com/test/repo");
    assert_eq!(case.revision, "abc123~1");
    assert!(!case.edit_history.is_empty());
}

#[test]
fn test_generate_evaluation_example_reproducible() {
    let commit = r#"////////////////////////////////////////////////////////////////////////////////
// Add greeting
////////////////////////////////////////////////////////////////////////////////
--- a/test.rs
+++ b/test.rs
@@ -1,3 +1,5 @@
 fn main() {
+    println!("hello");
+    println!("world");
 }
"#;

    // Run twice with the same seed
    let result1 = generate_evaluation_example_from_ordered_commit(
        commit,
        "https://github.com/test/repo",
        "abc123",
        Some(SplitPoint::Fraction(0.5)),
        Some(12345),
        None,
    )
    .unwrap();

    let result2 = generate_evaluation_example_from_ordered_commit(
        commit,
        "https://github.com/test/repo",
        "abc123",
        Some(SplitPoint::Fraction(0.5)),
        Some(12345),
        None,
    )
    .unwrap();

    // Results should be identical
    assert_eq!(result1.edit_history, result2.edit_history);
    assert_eq!(result1.expected_patches, result2.expected_patches);
    assert_eq!(result1.cursor_position, result2.cursor_position);
}

#[test]
fn test_cursor_position_display() {
    let cursor = CursorPosition {
        file: "src/main.rs".to_string(),
        line: 42,
        column: 10,
        line_length: 80,
    };
    assert_eq!(cursor.to_string(), "src/main.rs:42:10");
}

#[test]
fn test_imitate_human_edits_no_change_when_no_replacement() {
    // Source and target patches that don't form a replacement pattern
    let source = r#"--- a/test.rs
+++ b/test.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("hello");
 }
"#;
    let target = r#"--- a/test.rs
+++ b/test.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("world");
 }
"#;

    let (new_src, new_tgt, cursor) = imitate_human_edits(source, target, 42);

    // Should return unchanged when not a replacement pattern
    assert_eq!(new_src, source);
    assert_eq!(new_tgt, target);
    assert!(cursor.is_none());
}

#[test]
fn test_parse_typed_split_points() {
    assert_eq!(
        parse_split_point("fim").unwrap(),
        SplitPoint::Kind(SplitPointKind::Fim)
    );
    assert_eq!(
        parse_split_point("same-file-near").unwrap(),
        SplitPoint::Kind(SplitPointKind::SameFileNear)
    );
    assert_eq!(
        parse_split_point("same-file-far:2").unwrap(),
        SplitPoint::KindWithSplit {
            kind: SplitPointKind::SameFileFar,
            split_point: SplitPointValue::Index(2),
        }
    );
    assert_eq!(
        parse_split_point("cross-file:0.5").unwrap(),
        SplitPoint::KindWithSplit {
            kind: SplitPointKind::CrossFile,
            split_point: SplitPointValue::Fraction(0.5),
        }
    );
}
