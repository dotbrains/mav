use super::*;

#[test]
fn test_weighted_select() {
    // Test that weighted selection returns correct indices
    let weights = vec![1, 10, 1];

    // With total weight 12, seed 0 should select index 0
    // seed 0 % 12 = 0, cumulative: 1 at idx 0, so returns 0
    assert_eq!(weighted_select(&weights, 0), 0);

    // seed 1 % 12 = 1, cumulative: 1 at idx 0 (1 < 1 is false), 11 at idx 1 (1 < 11 is true)
    assert_eq!(weighted_select(&weights, 1), 1);

    // seed 10 % 12 = 10, cumulative: 1, 11 at idx 1 (10 < 11 is true)
    assert_eq!(weighted_select(&weights, 10), 1);

    // seed 11 % 12 = 11, cumulative: 1, 11 at idx 1 (11 < 11 is false), 12 at idx 2 (11 < 12 is true)
    assert_eq!(weighted_select(&weights, 11), 2);

    // Empty weights should return 0
    let empty: Vec<u32> = vec![];
    assert_eq!(weighted_select(&empty, 42), 0);

    // Single weight should always return index 0
    let single = vec![10];
    assert_eq!(weighted_select(&single, 0), 0);
    assert_eq!(weighted_select(&single, 100), 0);
}

#[test]
fn test_weighted_split_prefers_natural_boundaries() {
    // Test that with different seeds, weighted selection tends to prefer
    // positions after punctuation over mid-identifier positions
    let text_with_punctuation = "foo(bar, baz)";
    let text_mid_identifier = "foobar";

    // Position after '(' should have high weight
    let weight_after_paren = position_weight(text_with_punctuation, 4);
    // Position after ',' should have high weight
    let weight_after_comma = position_weight(text_with_punctuation, 8);
    // Position mid-identifier should have low weight
    let weight_mid_ident = position_weight(text_mid_identifier, 3);

    assert!(
        weight_after_paren > weight_mid_ident,
        "After '(' ({}) should be weighted higher than mid-identifier ({})",
        weight_after_paren,
        weight_mid_ident
    );
    assert!(
        weight_after_comma > weight_mid_ident,
        "After ',' ({}) should be weighted higher than mid-identifier ({})",
        weight_after_comma,
        weight_mid_ident
    );
}

#[test]
fn test_imitate_human_edits_pure_insertion() {
    // Source patch is empty (no edits yet)
    // Target patch has a pure insertion (adding a new line)
    let source = r#"--- a/test.rs
+++ b/test.rs
@@ -1,2 +1,2 @@
 fn main() {
 }
"#;
    let target = r#"--- a/test.rs
+++ b/test.rs
@@ -1,2 +1,3 @@
 fn main() {
+    println!("debug");
 }
"#;

    let (new_src, new_tgt, cursor) = imitate_human_edits(source, target, 42);

    // Should have transformed the patches
    assert_ne!(
        new_src, source,
        "Source should be modified for pure insertion"
    );
    assert_ne!(
        new_tgt, target,
        "Target should be modified for pure insertion"
    );
    assert!(cursor.is_some(), "Cursor should be set");

    // Source should now have a partial addition
    let src_patch = Patch::parse_unified_diff(&new_src);
    assert!(
        src_patch.stats().added > 0,
        "Source should have added lines"
    );

    // Target should have both a deletion (of partial) and addition (of full)
    let tgt_patch = Patch::parse_unified_diff(&new_tgt);
    assert!(
        tgt_patch.stats().removed > 0,
        "Target should have removed lines (partial)"
    );
    assert!(
        tgt_patch.stats().added > 0,
        "Target should have added lines (full)"
    );

    // The cursor should be in test.rs
    let cursor = cursor.unwrap();
    assert_eq!(cursor.file, "test.rs");
}

#[test]
fn test_imitate_human_edits_pure_insertion_empty_source() {
    // Source patch has no hunks at all
    let source = "";
    let target = r#"--- a/test.rs
+++ b/test.rs
@@ -1,2 +1,3 @@
 fn main() {
+    println!("hello");
 }
"#;

    let (new_src, _new_tgt, cursor) = imitate_human_edits(source, target, 123);

    // Should have created a source patch with partial insertion
    assert!(!new_src.is_empty(), "Source should not be empty");
    assert!(cursor.is_some(), "Cursor should be set");

    let src_patch = Patch::parse_unified_diff(&new_src);
    assert!(
        src_patch.stats().added > 0,
        "Source should have added lines"
    );
}

#[test]
fn test_imitate_human_edits_pure_insertion_intermediate_content() {
    // Verify the actual intermediate content is a realistic partial typing state
    let source = "";
    let target = r#"--- a/test.rs
+++ b/test.rs
@@ -1,2 +1,3 @@
 fn main() {
+    println!("hello world");
 }
"#;

    // Test with multiple seeds to see different split points
    let mut found_partial = false;
    for seed in 1..=50 {
        let (new_src, new_tgt, cursor) = imitate_human_edits(source, target, seed);

        if cursor.is_some() {
            let src_patch = Patch::parse_unified_diff(&new_src);
            let tgt_patch = Patch::parse_unified_diff(&new_tgt);

            // Find the added line in source
            for hunk in &src_patch.hunks {
                for line in &hunk.lines {
                    if let PatchLine::Addition(content) = line {
                        // The partial line should be a prefix of the full line
                        let full_line = "    println!(\"hello world\");";
                        if content != full_line && full_line.starts_with(content) {
                            found_partial = true;

                            // Verify target has the partial as deletion
                            let mut has_deletion = false;
                            for tgt_hunk in &tgt_patch.hunks {
                                for tgt_line in &tgt_hunk.lines {
                                    if let PatchLine::Deletion(del_content) = tgt_line {
                                        if del_content == content {
                                            has_deletion = true;
                                        }
                                    }
                                }
                            }
                            assert!(has_deletion, "Target should have deletion of partial line");
                        }
                    }
                }
            }
        }
    }

    assert!(
        found_partial,
        "At least one seed should produce a partial intermediate state"
    );
}

#[test]
fn test_imitate_human_edits_inserts_after_last_source_edit() {
    // Regression test: intermediate content should appear after the last edit
    // in the source patch, not at the position of the first target edit.
    // This ensures the diff output correctly imitates human typing order.
    //
    // The bug was: when source has edits and target has a pure insertion,
    // the intermediate content was inserted at tgt_edit_loc.line_index_within_hunk
    // (position of first target edit) instead of after the last source edit.
    //
    // Source patch has edits at lines 1-4, target has a new edit at line 10
    // (different location to avoid the "same line" early return)
    let source = r#"--- a/test.py
+++ b/test.py
@@ -1,4 +1,5 @@
+import foo
 import bar
-import old
 import baz
+import qux
"#;
    // Target has a pure insertion at a different line (line 10, not overlapping with source)
    let target = r#"--- a/test.py
+++ b/test.py
@@ -10,3 +10,4 @@
 def main():
+    print("hello world")
     pass
"#;

    // Use a seed that produces a partial result
    let (new_src, _new_tgt, cursor) = imitate_human_edits(source, target, 42);

    // The function should produce a modified patch
    assert!(cursor.is_some(), "Should produce intermediate state");

    let src_patch = Patch::parse_unified_diff(&new_src);
    let all_additions: Vec<_> = src_patch
        .hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter_map(|l| match l {
            PatchLine::Addition(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    // The intermediate content (partial 'print("hello world")') should be
    // the LAST addition, appearing after "+import qux" (the last source edit)
    let last_addition = all_additions.last().expect("Should have additions");
    assert!(
        last_addition.trim_start().starts_with("pr"),
        "Intermediate content should be the last addition (partial 'print'), but last was: {:?}",
        last_addition
    );

    // Verify the original source edits are still in order before the intermediate
    let foo_pos = all_additions.iter().position(|s| *s == "import foo");
    let qux_pos = all_additions.iter().position(|s| *s == "import qux");
    let intermediate_pos = all_additions
        .iter()
        .position(|s| s.trim_start().starts_with("pr"));

    assert!(foo_pos.is_some(), "Should have 'import foo'");
    assert!(qux_pos.is_some(), "Should have 'import qux'");
    assert!(
        intermediate_pos.is_some(),
        "Should have intermediate content"
    );

    assert!(
        foo_pos < qux_pos && qux_pos < intermediate_pos,
        "Order should be: foo < qux < intermediate. Got foo={:?}, qux={:?}, intermediate={:?}",
        foo_pos,
        qux_pos,
        intermediate_pos
    );
}
