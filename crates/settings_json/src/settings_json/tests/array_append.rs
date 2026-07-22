use super::*;

#[test]
fn array_append() {
    #[track_caller]
    fn check_array_append(input: impl ToString, value: Value, expected: impl ToString) {
        let input = input.to_string();
        let result = append_top_level_array_value_in_json_text(&input, &value, 4);
        let mut result_str = input;
        result_str.replace_range(result.0, &result.1);
        pretty_assertions::assert_eq!(expected.to_string(), result_str);
    }
    check_array_append(r#"[1, 3, 3]"#, json!(4), r#"[1, 3, 3, 4]"#);
    check_array_append(r#"[1, 3, 3,]"#, json!(4), r#"[1, 3, 3, 4]"#);
    check_array_append(r#"[1, 3, 3   ]"#, json!(4), r#"[1, 3, 3, 4]"#);
    check_array_append(r#"[1, 3, 3,   ]"#, json!(4), r#"[1, 3, 3, 4]"#);
    check_array_append(
        r#"[
                1,
                2,
                3
            ]"#
        .unindent(),
        json!(4),
        r#"[
                1,
                2,
                3,
                4
            ]"#
        .unindent(),
    );
    check_array_append(
        r#"[
                1,
                2,
                3,
            ]"#
        .unindent(),
        json!(4),
        r#"[
                1,
                2,
                3,
                4
            ]"#
        .unindent(),
    );
    check_array_append(
        r#"[
                1,
                2,
                3,
            ]"#
        .unindent(),
        json!({"foo": "bar", "baz": "qux"}),
        r#"[
                1,
                2,
                3,
                {
                    "foo": "bar",
                    "baz": "qux"
                }
            ]"#
        .unindent(),
    );
    check_array_append(
        r#"[ 1, 2, 3, ]"#.unindent(),
        json!({"foo": "bar", "baz": "qux"}),
        r#"[ 1, 2, 3, { "foo": "bar", "baz": "qux" }]"#.unindent(),
    );
    check_array_append(
        r#"[]"#,
        json!({"foo": "bar"}),
        r#"[
                {
                    "foo": "bar"
                }
            ]"#
        .unindent(),
    );

    // Test with comments between array elements
    check_array_append(
        r#"[
                1,
                // Comment between elements
                2,
                /* Block comment */ 3
            ]"#
        .unindent(),
        json!(4),
        r#"[
                1,
                // Comment between elements
                2,
                /* Block comment */ 3,
                4
            ]"#
        .unindent(),
    );

    // Test with trailing comment on last element
    check_array_append(
        r#"[
                1,
                2,
                3 // Trailing comment
            ]"#
        .unindent(),
        json!("new"),
        r#"[
                1,
                2,
                3 // Trailing comment
            ,
                "new"
            ]"#
        .unindent(),
    );

    // Test empty array with comments
    check_array_append(
        r#"[
                // Empty array with comment
            ]"#
        .unindent(),
        json!("first"),
        r#"[
                // Empty array with comment
                "first"
            ]"#
        .unindent(),
    );

    // Test with multiline block comment at end
    check_array_append(
        r#"[
                1,
                2
                /*
                 * This is a
                 * multiline comment
                 */
            ]"#
        .unindent(),
        json!(3),
        r#"[
                1,
                2
                /*
                 * This is a
                 * multiline comment
                 */
            ,
                3
            ]"#
        .unindent(),
    );

    // Test with deep indentation
    check_array_append(
        r#"[
                1,
                    2,
                        3
            ]"#
        .unindent(),
        json!("deep"),
        r#"[
                1,
                    2,
                        3,
                        "deep"
            ]"#
        .unindent(),
    );

    // Test with no spacing
    check_array_append(r#"[1,2,3]"#, json!(4), r#"[1,2,3, 4]"#);

    // Test appending complex nested structure
    check_array_append(
        r#"[
                {"a": 1},
                {"b": 2}
            ]"#
        .unindent(),
        json!({"c": {"nested": [1, 2, 3]}}),
        r#"[
                {"a": 1},
                {"b": 2},
                {
                    "c": {
                        "nested": [
                            1,
                            2,
                            3
                        ]
                    }
                }
            ]"#
        .unindent(),
    );

    // Test array ending with comment after bracket
    check_array_append(
        r#"[
                1,
                2,
                3
            ] // Comment after array"#
            .unindent(),
        json!(4),
        r#"[
                1,
                2,
                3,
                4
            ] // Comment after array"#
            .unindent(),
    );

    // Test with inconsistent element formatting
    check_array_append(
        r#"[1,
               2,
                    3,
            ]"#
        .unindent(),
        json!(4),
        r#"[1,
               2,
                    3,
                    4
            ]"#
        .unindent(),
    );

    // Test appending to single-line array with trailing comma
    check_array_append(
        r#"[1, 2, 3,]"#,
        json!({"key": "value"}),
        r#"[1, 2, 3, { "key": "value" }]"#,
    );

    // Test appending null value
    check_array_append(r#"[true, false]"#, json!(null), r#"[true, false, null]"#);

    // Test appending to array with only comments
    check_array_append(
        r#"[
                // Just comments here
                // More comments
            ]"#
        .unindent(),
        json!(42),
        r#"[
                // Just comments here
                // More comments
                42
            ]"#
        .unindent(),
    );

    check_array_append(
        r#""#,
        json!(42),
        r#"[
                42
            ]"#
        .unindent(),
    )
}
