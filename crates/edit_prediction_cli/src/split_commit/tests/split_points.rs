use super::*;

fn assert_generated_split_kind(
    commit: &str,
    kind: SplitPointKind,
    seed: u64,
) -> GeneratedSplitCommit {
    let patch = Patch::parse_unified_diff(commit);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let generated_split_commit = sample_split_commit_of_kind(&patch, kind, &mut rng).unwrap();
    assert_eq!(
        classify_generated_split_commit(&generated_split_commit),
        Some(kind)
    );
    generated_split_commit
}

#[test]
fn test_classify_generated_split_commit() {
    let target_patch = r#"--- a/src/main.rs
+++ b/src/main.rs
@@ -10,3 +10,3 @@
 fn main() {
-old();
+new();
 }
"#;
    let mut generated_split_commit = GeneratedSplitCommit {
        split: 1,
        split_commit: SplitCommit {
            source_patch: String::new(),
            target_patch: target_patch.to_string(),
        },
        cursor: CursorPosition {
            file: "src/main.rs".to_string(),
            line: 11,
            column: 5,
            line_length: 10,
        },
        cursor_from_human_edit: true,
    };
    assert_eq!(
        classify_generated_split_commit(&generated_split_commit),
        Some(SplitPointKind::Fim)
    );

    generated_split_commit.cursor_from_human_edit = false;
    assert_eq!(
        classify_generated_split_commit(&generated_split_commit),
        Some(SplitPointKind::SameFileNear)
    );

    generated_split_commit.cursor.line = 100;
    assert_eq!(
        classify_generated_split_commit(&generated_split_commit),
        Some(SplitPointKind::SameFileFar)
    );

    generated_split_commit.cursor.file = "src/other.rs".to_string();
    assert_eq!(
        classify_generated_split_commit(&generated_split_commit),
        Some(SplitPointKind::CrossFile)
    );
}

#[test]
fn test_sample_fim_split_point() {
    let commit = r#"--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,5 @@
 fn main() {
+    let first = 1;
+    let second = 2;
 }
"#;

    assert_generated_split_kind(commit, SplitPointKind::Fim, 1);
}

#[test]
fn test_sample_same_file_near_split_point() {
    let commit = r#"--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@
 fn main() {
+    let inserted = 0;
-    old();
+    new();
 }
"#;

    assert_generated_split_kind(commit, SplitPointKind::SameFileNear, 1);
}

#[test]
fn test_sample_same_file_far_split_point() {
    let commit = r#"--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,3 @@
 start
+source_edit();
 context
@@ -100,2 +101,2 @@
-far_old();
+far_new();
 end
"#;

    assert_generated_split_kind(commit, SplitPointKind::SameFileFar, 1);
}

#[test]
fn test_sample_cross_file_split_point() {
    let commit = r#"--- a/src/main.rs
+++ b/src/main.rs
@@ -1,2 +1,3 @@
 fn main() {
+    source_edit();
 }
--- a/src/other.rs
+++ b/src/other.rs
@@ -1,3 +1,3 @@
 fn other() {
-    old();
+    new();
 }
"#;

    assert_generated_split_kind(commit, SplitPointKind::CrossFile, 1);
}

#[test]
fn test_split_point_fraction() {
    let commit = r#"// Change
--- a/test.rs
+++ b/test.rs
@@ -1,5 +1,10 @@
 fn main() {
+    line1();
+    line2();
+    line3();
+    line4();
+    line5();
 }
"#;

    // Split at 20% should give first edit in source
    let result = generate_evaluation_example_from_ordered_commit(
        commit,
        "",
        "hash",
        Some(SplitPoint::Fraction(0.2)),
        Some(1),
        None,
    );

    assert!(result.is_ok());
    let case = result.unwrap();

    // Source should have some edits
    let src_patch = Patch::parse_unified_diff(&case.edit_history);
    assert!(src_patch.stats().added > 0);
}

#[test]
fn test_split_point_index() {
    let commit = r#"// Change
--- a/test.rs
+++ b/test.rs
@@ -1,5 +1,10 @@
 fn main() {
+    line1();
+    line2();
+    line3();
+    line4();
+    line5();
 }
"#;

    // Split at index 2 should give first 2 edits in source
    // With pure insertion handling, source gets 2 original + 1 partial = 3 additions
    let result = generate_evaluation_example_from_ordered_commit(
        commit,
        "",
        "hash",
        Some(SplitPoint::Index(2)),
        Some(1),
        None,
    );

    assert!(result.is_ok());
    let case = result.unwrap();

    let src_patch = Patch::parse_unified_diff(&case.edit_history);
    // Pure insertion adds a partial line, so we expect 3 (2 original + 1 partial)
    assert_eq!(src_patch.stats().added, 3);
}
