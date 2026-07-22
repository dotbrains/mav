use super::*;

#[test]
fn test_reverse_diff() {
    let forward_diff = indoc! {"
             --- a/file.rs
             +++ b/file.rs
             @@ -1,3 +1,4 @@
              fn main() {
             +    let x = 42;
                  println!(\"hello\");
             }"};

    let reversed = reverse_diff(forward_diff);

    assert!(
        reversed.contains("+++ a/file.rs"),
        "Should have +++ for old path"
    );
    assert!(
        reversed.contains("--- b/file.rs"),
        "Should have --- for new path"
    );
    assert!(
        reversed.contains("-    let x = 42;"),
        "Added line should become deletion"
    );
    assert!(
        reversed.contains(" fn main()"),
        "Context lines should be unchanged"
    );
}

#[test]
fn test_reverse_diff_roundtrip() {
    // Applying a diff and then its reverse should get back to original
    let original = indoc! {"
             first line
             hello world
             last line
         "};
    let modified = indoc! {"
             first line
             hello beautiful world
             last line
         "};

    // unified_diff_with_context doesn't include file headers, but apply_diff_to_string needs them
    let diff_body = unified_diff_with_context(original, modified, 0, 0, 3);
    let forward_diff = format!("--- a/file\n+++ b/file\n{}", diff_body);
    let reversed_diff = reverse_diff(&forward_diff);

    // Apply forward diff to original
    let after_forward = apply_diff_to_string(&forward_diff, original).unwrap();
    assert_eq!(after_forward, modified);

    // Apply reversed diff to modified
    let after_reverse = apply_diff_to_string(&reversed_diff, &after_forward).unwrap();
    assert_eq!(after_reverse, original);
}
