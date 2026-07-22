use super::*;

#[test]
fn array_replace() {
    #[track_caller]
    fn check_array_replace(
        input: impl ToString,
        index: usize,
        key_path: &[&str],
        value: Option<Value>,
        expected: impl ToString,
    ) {
        let input = input.to_string();
        let result = replace_top_level_array_value_in_json_text(
            &input,
            key_path,
            value.as_ref(),
            None,
            index,
            4,
        );
        let mut result_str = input;
        result_str.replace_range(result.0, &result.1);
        pretty_assertions::assert_eq!(expected.to_string(), result_str);
    }

    check_array_replace(r#"[1, 3, 3]"#, 1, &[], Some(json!(2)), r#"[1, 2, 3]"#);
    check_array_replace(r#"[1, 3, 3]"#, 2, &[], Some(json!(2)), r#"[1, 3, 2]"#);
    check_array_replace(r#"[1, 3, 3,]"#, 3, &[], Some(json!(2)), r#"[1, 3, 3, 2]"#);
    check_array_replace(r#"[1, 3, 3,]"#, 100, &[], Some(json!(2)), r#"[1, 3, 3, 2]"#);
    check_array_replace(
        r#"[
                1,
                2,
                3,
            ]"#
        .unindent(),
        1,
        &[],
        Some(json!({"foo": "bar", "baz": "qux"})),
        r#"[
                1,
                {
                    "foo": "bar",
                    "baz": "qux"
                },
                3,
            ]"#
        .unindent(),
    );
    check_array_replace(
        r#"[1, 3, 3,]"#,
        1,
        &[],
        Some(json!({"foo": "bar", "baz": "qux"})),
        r#"[1, { "foo": "bar", "baz": "qux" }, 3,]"#,
    );

    check_array_replace(
        r#"[1, { "foo": "bar", "baz": "qux" }, 3,]"#,
        1,
        &["baz"],
        Some(json!({"qux": "quz"})),
        r#"[1, { "foo": "bar", "baz": { "qux": "quz" } }, 3,]"#,
    );

    check_array_replace(
        r#"[
                1,
                {
                    "foo": "bar",
                    "baz": "qux"
                },
                3
            ]"#,
        1,
        &["baz"],
        Some(json!({"qux": "quz"})),
        r#"[
                1,
                {
                    "foo": "bar",
                    "baz": {
                        "qux": "quz"
                    }
                },
                3
            ]"#,
    );

    check_array_replace(
        r#"[
                1,
                {
                    "foo": "bar",
                    "baz": {
                        "qux": "quz"
                    }
                },
                3
            ]"#,
        1,
        &["baz"],
        Some(json!("qux")),
        r#"[
                1,
                {
                    "foo": "bar",
                    "baz": "qux"
                },
                3
            ]"#,
    );

    check_array_replace(
        r#"[
                1,
                {
                    "foo": "bar",
                    // some comment to keep
                    "baz": {
                        // some comment to remove
                        "qux": "quz"
                    }
                    // some other comment to keep
                },
                3
            ]"#,
        1,
        &["baz"],
        Some(json!("qux")),
        r#"[
                1,
                {
                    "foo": "bar",
                    // some comment to keep
                    "baz": "qux"
                    // some other comment to keep
                },
                3
            ]"#,
    );

    // Test with comments between array elements
    check_array_replace(
        r#"[
                1,
                // This is element 2
                2,
                /* Block comment */ 3,
                4 // Trailing comment
            ]"#,
        2,
        &[],
        Some(json!("replaced")),
        r#"[
                1,
                // This is element 2
                2,
                /* Block comment */ "replaced",
                4 // Trailing comment
            ]"#,
    );

    // Test empty array with comments
    check_array_replace(
        r#"[
                // Empty array with comment
            ]"#
        .unindent(),
        0,
        &[],
        Some(json!("first")),
        r#"[
                // Empty array with comment
                "first"
            ]"#
        .unindent(),
    );
    check_array_replace(
        r#"[]"#.unindent(),
        0,
        &[],
        Some(json!("first")),
        r#"["first"]"#.unindent(),
    );

    // Test array with leading comments
    check_array_replace(
        r#"[
                // Leading comment
                // Another leading comment
                1,
                2
            ]"#,
        0,
        &[],
        Some(json!({"new": "object"})),
        r#"[
                // Leading comment
                // Another leading comment
                {
                    "new": "object"
                },
                2
            ]"#,
    );

    // Test with deep indentation
    check_array_replace(
        r#"[
                        1,
                        2,
                        3
                    ]"#,
        1,
        &[],
        Some(json!("deep")),
        r#"[
                        1,
                        "deep",
                        3
                    ]"#,
    );

    // Test with mixed spacing
    check_array_replace(
        r#"[1,2,   3,    4]"#,
        2,
        &[],
        Some(json!("spaced")),
        r#"[1,2,   "spaced",    4]"#,
    );

    // Test replacing nested array element
    check_array_replace(
        r#"[
                [1, 2, 3],
                [4, 5, 6],
                [7, 8, 9]
            ]"#,
        1,
        &[],
        Some(json!(["a", "b", "c", "d"])),
        r#"[
                [1, 2, 3],
                [
                    "a",
                    "b",
                    "c",
                    "d"
                ],
                [7, 8, 9]
            ]"#,
    );

    // Test with multiline block comments
    check_array_replace(
        r#"[
                /*
                 * This is a
                 * multiline comment
                 */
                "first",
                "second"
            ]"#,
        0,
        &[],
        Some(json!("updated")),
        r#"[
                /*
                 * This is a
                 * multiline comment
                 */
                "updated",
                "second"
            ]"#,
    );

    // Test replacing with null
    check_array_replace(
        r#"[true, false, true]"#,
        1,
        &[],
        Some(json!(null)),
        r#"[true, null, true]"#,
    );

    // Test single element array
    check_array_replace(
        r#"[42]"#,
        0,
        &[],
        Some(json!({"answer": 42})),
        r#"[{ "answer": 42 }]"#,
    );

    // Test array with only comments
    check_array_replace(
        r#"[
                // Comment 1
                // Comment 2
                // Comment 3
            ]"#
        .unindent(),
        10,
        &[],
        Some(json!(123)),
        r#"[
                // Comment 1
                // Comment 2
                // Comment 3
                123
            ]"#
        .unindent(),
    );

    check_array_replace(
        r#"[
                {
                    "key": "value"
                },
                {
                    "key": "value2"
                }
            ]"#
        .unindent(),
        0,
        &[],
        None,
        r#"[
                {
                    "key": "value2"
                }
            ]"#
        .unindent(),
    );

    check_array_replace(
        r#"[
                {
                    "key": "value"
                },
                {
                    "key": "value2"
                },
                {
                    "key": "value3"
                },
            ]"#
        .unindent(),
        1,
        &[],
        None,
        r#"[
                {
                    "key": "value"
                },
                {
                    "key": "value3"
                },
            ]"#
        .unindent(),
    );

    check_array_replace(
        r#""#,
        2,
        &[],
        Some(json!(42)),
        r#"[
                42
            ]"#
        .unindent(),
    );

    check_array_replace(
        r#""#,
        2,
        &["foo", "bar"],
        Some(json!(42)),
        r#"[
                {
                    "foo": {
                        "bar": 42
                    }
                }
            ]"#
        .unindent(),
    );
}
