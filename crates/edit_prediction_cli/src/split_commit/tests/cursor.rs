use super::*;

#[test]
fn test_cursor_excerpt_with_multibyte_utf8() {
    // Test that cursor excerpt handles multi-byte UTF-8 characters correctly
    // The Chinese character '第' is 3 bytes (0..3)
    let cursor = CursorPosition {
        file: "test.md".to_string(),
        line: 1,
        column: 1, // Byte index 1 is inside '第' (bytes 0..3)
        line_length: 80,
    };

    let source_patch = r#"--- a/test.md
+++ b/test.md
@@ -1,1 +1,1 @@
+第 14 章 Flask 工作原理与机制解析**
"#;

    let target_patch = "";

    // This should not panic even though column=1 is not a char boundary
    let result = get_cursor_excerpt(&cursor, source_patch, target_patch);

    // The function should handle the invalid byte index gracefully
    if let Some(excerpt) = result {
        assert!(
            excerpt.contains("<|user_cursor|>"),
            "Cursor excerpt should contain marker"
        );
        // The marker should be placed at a valid character boundary
        // (either at the start or after '第')
    }
}
