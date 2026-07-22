use super::*;

#[test]
fn test_cursor_excerpt_contains_marker() {
    let commit = r#"////////////////////////////////////////////////////////////////////////////////
// Add code
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
        "",
        "hash",
        Some(SplitPoint::Fraction(0.5)),
        Some(42),
        None,
    )
    .unwrap();

    // Cursor excerpt should contain the cursor marker
    assert!(
        result.cursor_position.contains("<|user_cursor|>"),
        "Cursor excerpt should contain marker: {}",
        result.cursor_position
    );
}

#[test]
fn test_evaluation_case_json_serialization() {
    let case = ExampleSpec {
        name: "test-abc123".to_string(),
        repository_url: "https://github.com/test/repo".to_string(),
        revision: "abc123~1".to_string(),
        edit_history: "patch1".to_string(),
        cursor_path: Path::new("file.rs").into(),
        cursor_position: "some code<|user_cursor|>".to_string(),
        expected_patches: vec!["patch".to_string()],
        tags: vec![],
        reasoning: None,
        uncommitted_diff: String::new(),
        recently_opened_files: Vec::new(),
        recently_viewed_files: Vec::new(),
        uncommitted_diff_contains_edit_history: false,
        rejected_patch: None,

        telemetry: None,
        human_feedback: Vec::new(),
        rating: None,
    };

    let json = serde_json::to_string(&case).unwrap();
    let deserialized: ExampleSpec = serde_json::from_str(&json).unwrap();

    assert_eq!(case.repository_url, deserialized.repository_url);
    assert_eq!(case.revision, deserialized.revision);
    assert_eq!(case.cursor_position, deserialized.cursor_position);
}

#[test]
fn test_empty_commit_returns_error() {
    let commit = "";

    let result = generate_evaluation_example_from_ordered_commit(
        commit,
        "",
        "hash",
        Some(SplitPoint::Fraction(0.5)),
        Some(1),
        None,
    );

    assert!(result.is_err());
}

#[test]
fn test_header_filtering() {
    let commit = r#"commit abc123
Author: Test
Date: Today

    Message

diff --git a/test.rs b/test.rs
index 123..456 789
////////////////////////////////////////////////////////////////////////////////
// First group
////////////////////////////////////////////////////////////////////////////////
--- a/test.rs
+++ b/test.rs
@@ -1,3 +1,4 @@
 fn main() {
+    code();
 }
"#;

    let result = generate_evaluation_example_from_ordered_commit(
        commit,
        "",
        "hash",
        Some(SplitPoint::Index(1)),
        Some(1),
        None,
    );

    assert!(result.is_ok());
    let case = result.unwrap();

    // The edit history should contain the group header (// lines)
    // but not the commit metadata
    assert!(!case.edit_history.contains("Author:"));
    assert!(!case.edit_history.contains("Date:"));
}

#[test]
fn test_service_file_detection() {
    assert!(is_service_file("package.json"));
    assert!(is_service_file("frontend/yarn.lock"));
    assert!(is_service_file("a/src/generated/types.pb.go"));
    assert!(is_service_file("b/.github/workflows/ci.yml"));
    assert!(is_service_file("web/node_modules/pkg/index.js"));
    assert!(is_service_file("dist/app.bundle.js"));

    assert!(!is_service_file("src/main.rs"));
    assert!(!is_service_file("src/build.rs"));
    assert!(!is_service_file("Cargo.toml"));
}

#[test]
fn test_edit_starts_on_service_file() {
    let commit = r#"--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,1 +1,2 @@
 fn lib() {}
+pub fn added() {}
--- a/package-lock.json
+++ b/package-lock.json
@@ -1,1 +1,2 @@
 {}
+{"lockfileVersion": 3}
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,1 +1,2 @@
 fn main() {}
+println!("hello");
"#;
    let patch = Patch::parse_unified_diff(commit);

    assert!(edit_starts_on_service_file(&patch, 1));
    assert!(!edit_starts_on_service_file(&patch, 2));
}

#[test]
fn test_submodule_gitlink_hunk_detection() {
    assert!(has_submodule_gitlink_hunk(
        r#"diff --git a/controllers/llguidance b/controllers/llguidance
index 21e68b9..cadabda 160000
--- a/controllers/llguidance
+++ b/controllers/llguidance
@@ -1 +1 @@
-Subproject commit 21e68b916d4705107e1c45ea7bc927e829136258
+Subproject commit cadabdad21f3b81ff58b1918f8c23116b4ff7af3
"#
    ));
    assert!(has_submodule_gitlink_hunk(
        r#"--- a/controllers/derivre
+++ b/controllers/derivre
@@ -1 +1 @@
-Subproject commit e83d8fb3cd92d2c6dd0437e98bfa9b64d8d8284b
+Subproject commit fb0ba7b6307782e0d43a0ca598b237836cb6d304
"#
    ));
    assert!(has_submodule_gitlink_hunk(
        r#"diff --git a/vendor/dependency b/vendor/dependency
new file mode 160000
index 0000000..1234567
--- /dev/null
+++ b/vendor/dependency
"#
    ));
    assert!(!has_submodule_gitlink_hunk(
        r#"diff --git a/src/lib.rs b/src/lib.rs
index 1234567..89abcde 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1,2 @@
 fn lib() {}
+fn helper() {}
"#
    ));
}

#[test]
fn test_generate_evaluation_example_rejects_submodule_gitlink_hunk() {
    let commit = r#"diff --git a/controllers/llguidance b/controllers/llguidance
index 21e68b9..cadabda 160000
--- a/controllers/llguidance
+++ b/controllers/llguidance
@@ -1 +1 @@
-Subproject commit 21e68b916d4705107e1c45ea7bc927e829136258
+Subproject commit cadabdad21f3b81ff58b1918f8c23116b4ff7af3
"#;

    let result = generate_evaluation_example_from_ordered_commit(
        commit,
        "https://github.com/microsoft/aici",
        "cadabdad21f3b81ff58b1918f8c23116b4ff7af3",
        None,
        Some(0),
        None,
    );

    let Err(error) = result else {
        panic!("expected submodule/gitlink commit to be rejected");
    };
    assert!(error.to_string().contains("submodule/gitlink"));
}

#[test]
fn test_position_weight() {
    // High weight positions (natural pause points)
    assert_eq!(position_weight("foo(", 4), 10); // After '('
    assert_eq!(position_weight("a, b", 2), 10); // After ','
    assert_eq!(position_weight("x;", 2), 10); // After ';'
    assert_eq!(position_weight("a: b", 2), 10); // After ':'
    assert_eq!(position_weight("[", 1), 10); // After '['
    assert_eq!(position_weight("{", 1), 10); // After '{'

    // High weight for closing brackets
    assert_eq!(position_weight("foo)", 4), 8); // After ')'
    assert_eq!(position_weight("]", 1), 8); // After ']'
    assert_eq!(position_weight("}", 1), 8); // After '}'

    // High weight at end of identifier
    assert_eq!(position_weight("foo ", 3), 8); // End of 'foo' before space
    assert_eq!(position_weight("bar(", 3), 8); // End of 'bar' before '('

    // Medium weight for operators
    assert_eq!(position_weight("a + b", 3), 5); // After '+'
    assert_eq!(position_weight("x.", 2), 5); // After '.'
    assert_eq!(position_weight("a=b", 2), 5); // After '='

    // Medium weight for whitespace
    assert_eq!(position_weight("a ", 2), 6); // After space

    // Low weight mid-identifier
    assert_eq!(position_weight("foobar", 3), 1); // Mid-identifier 'foo|bar'

    // Edge cases
    assert_eq!(position_weight("", 0), 1); // Empty string
    assert_eq!(position_weight("a", 0), 1); // Position 0
}
