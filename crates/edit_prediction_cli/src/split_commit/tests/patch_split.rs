use super::*;

#[test]
fn test_tokenize() {
    let tokens = tokenize("hello world");
    assert_eq!(tokens, vec!["hello", " ", "world"]);

    let tokens = tokenize("foo_bar123 + baz");
    assert_eq!(tokens, vec!["foo_bar123", " ", "+", " ", "baz"]);

    let tokens = tokenize("print(\"hello\")");
    assert_eq!(tokens, vec!["print", "(", "\"", "hello", "\"", ")"]);

    let tokens = tokenize("hello_world");
    assert_eq!(tokens, vec!["hello_world"]);

    let tokens = tokenize("fn();");
    assert_eq!(tokens, vec!["fn", "(", ")", ";"]);
}

#[test]
fn test_fuzzy_ratio() {
    assert_eq!(fuzzy_ratio("hello", "hello"), 100);
    assert_eq!(fuzzy_ratio("", ""), 100);
    assert!(fuzzy_ratio("hello", "world") < 50);
    assert!(fuzzy_ratio("hello world", "hello worl") > 80);
}

#[test]
fn test_split_ordered_commit() {
    let commit = r#"// First change
--- a/test.rs
+++ b/test.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("hello");
+    println!("world");
 }
"#;
    let patch = Patch::parse_unified_diff(commit);
    let stats = patch.stats();
    assert_eq!(stats.added, 2);

    let (source, target) = split_ordered_patch(&patch, 1);

    // Source should have 1 addition
    let src_patch = Patch::parse_unified_diff(&source);
    assert_eq!(src_patch.stats().added, 1);

    // Target should have 1 addition
    let tgt_patch = Patch::parse_unified_diff(&target);
    assert_eq!(tgt_patch.stats().added, 1);
}

#[test]
fn test_split_ordered_commit_with_deletions() {
    let commit = r#"// Change
--- a/test.rs
+++ b/test.rs
@@ -1,3 +1,3 @@
 fn main() {
-    println!("old");
+    println!("new");
 }
"#;
    let patch = Patch::parse_unified_diff(commit);
    let stats = patch.stats();
    assert_eq!(stats.added, 1);
    assert_eq!(stats.removed, 1);

    // Split at position 1 (after the deletion)
    let (source, target) = split_ordered_patch(&patch, 1);

    let src_patch = Patch::parse_unified_diff(&source);
    let tgt_patch = Patch::parse_unified_diff(&target);

    // Source should have the deletion
    assert_eq!(src_patch.stats().removed, 1);
    // Target should have the addition
    assert_eq!(tgt_patch.stats().added, 1);
}

#[test]
fn test_split_ordered_commit_target_header_continues_current_group() {
    let commit = r#"////////////////////////////////////////////////////////////////////////////////
// Update dependency version
////////////////////////////////////////////////////////////////////////////////
--- a/go.mod
+++ b/go.mod
@@ -1,3 +1,3 @@
 require (
-	gopkg.in/yaml.v3 v3.0.0 // indirect
+	gopkg.in/yaml.v3 v3.0.1 // indirect
 )
diff --git a/go.sum b/go.sum
index f71a068..b8cc3c2 100644
////////////////////////////////////////////////////////////////////////////////
// Update go.sum checksums
////////////////////////////////////////////////////////////////////////////////
--- a/go.sum
+++ b/go.sum
@@ -1,3 +1,5 @@
 gopkg.in/yaml.v3 v3.0.0 h1:old
 gopkg.in/yaml.v3 v3.0.0/go.mod h1:oldmod
+gopkg.in/yaml.v3 v3.0.1 h1:new
+gopkg.in/yaml.v3 v3.0.1/go.mod h1:newmod
diff --git a/lib/handler.go b/lib/handler.go
index 1827a70..d9b3ed1 100644
////////////////////////////////////////////////////////////////////////////////
// Fix error wrapping
////////////////////////////////////////////////////////////////////////////////
--- a/lib/handler.go
+++ b/lib/handler.go
@@ -1,3 +1,3 @@
-	return fmt.Errorf("failed: %s", err)
+	return fmt.Errorf("failed: %w", err)
"#;

    let (_source, target) = split_ordered_patch(&Patch::parse_unified_diff(commit), 3);

    assert!(
            target.starts_with(
                "////////////////////////////////////////////////////////////////////////////////\n// Update go.sum checksums\n////////////////////////////////////////////////////////////////////////////////\n"
            ),
            "target patch should continue with the active group header:\n{target}"
        );
    assert!(!target.starts_with(
            "////////////////////////////////////////////////////////////////////////////////\n// Update dependency version\n////////////////////////////////////////////////////////////////////////////////\n"
        ));
}
