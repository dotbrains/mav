use super::*;

#[test]
fn object_replace() {
    #[track_caller]
    fn check_object_replace(
        input: String,
        key_path: &[&str],
        value: Option<Value>,
        expected: String,
    ) {
        let result = replace_value_in_json_text(&input, key_path, 4, value.as_ref(), None);
        let mut result_str = input;
        result_str.replace_range(result.0, &result.1);
        pretty_assertions::assert_eq!(expected, result_str);
    }
    check_object_replace(
        r#"{
                "a": 1,
                "b": 2
            }"#
        .unindent(),
        &["b"],
        Some(json!(3)),
        r#"{
                "a": 1,
                "b": 3
            }"#
        .unindent(),
    );
    check_object_replace(
        r#"{
                "a": 1,
                "b": 2
            }"#
        .unindent(),
        &["b"],
        None,
        r#"{
                "a": 1
            }"#
        .unindent(),
    );
    check_object_replace(
        r#"{
                "a": 1,
                "b": 2
            }"#
        .unindent(),
        &["c"],
        Some(json!(3)),
        r#"{
                "c": 3,
                "a": 1,
                "b": 2
            }"#
        .unindent(),
    );
    check_object_replace(
        r#"{
                "a": 1,
                "b": {
                    "c": 2,
                    "d": 3,
                }
            }"#
        .unindent(),
        &["b", "c"],
        Some(json!([1, 2, 3])),
        r#"{
                "a": 1,
                "b": {
                    "c": [
                        1,
                        2,
                        3
                    ],
                    "d": 3,
                }
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "name": "old_name",
                "id": 123
            }"#
        .unindent(),
        &["name"],
        Some(json!("new_name")),
        r#"{
                "name": "new_name",
                "id": 123
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "enabled": false,
                "count": 5
            }"#
        .unindent(),
        &["enabled"],
        Some(json!(true)),
        r#"{
                "enabled": true,
                "count": 5
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "value": null,
                "other": "test"
            }"#
        .unindent(),
        &["value"],
        Some(json!(42)),
        r#"{
                "value": 42,
                "other": "test"
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "config": {
                    "old": true
                },
                "name": "test"
            }"#
        .unindent(),
        &["config"],
        Some(json!({"new": false, "count": 3})),
        r#"{
                "config": {
                    "new": false,
                    "count": 3
                },
                "name": "test"
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                // This is a comment
                "a": 1,
                "b": 2 // Another comment
            }"#
        .unindent(),
        &["b"],
        Some(json!({"foo": "bar"})),
        r#"{
                // This is a comment
                "a": 1,
                "b": {
                    "foo": "bar"
                } // Another comment
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{}"#.to_string(),
        &["new_key"],
        Some(json!("value")),
        r#"{
                "new_key": "value"
            }
            "#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "only_key": 123
            }"#
        .unindent(),
        &["only_key"],
        None,
        "{\n    \n}".to_string(),
    );

    check_object_replace(
        r#"{
                "level1": {
                    "level2": {
                        "level3": {
                            "target": "old"
                        }
                    }
                }
            }"#
        .unindent(),
        &["level1", "level2", "level3", "target"],
        Some(json!("new")),
        r#"{
                "level1": {
                    "level2": {
                        "level3": {
                            "target": "new"
                        }
                    }
                }
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "parent": {}
            }"#
        .unindent(),
        &["parent", "child"],
        Some(json!("value")),
        r#"{
                "parent": {
                    "child": "value"
                }
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "a": 1,
                "b": 2,
            }"#
        .unindent(),
        &["b"],
        Some(json!(3)),
        r#"{
                "a": 1,
                "b": 3,
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "items": [1, 2, 3],
                "count": 3
            }"#
        .unindent(),
        &["items", "1"],
        Some(json!(5)),
        r#"{
                "items": {
                    "1": 5
                },
                "count": 3
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "items": [1, 2, 3],
                "count": 3
            }"#
        .unindent(),
        &["items", "1"],
        None,
        r#"{
                "items": {
                    "1": null
                },
                "count": 3
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "items": [1, 2, 3],
                "count": 3
            }"#
        .unindent(),
        &["items"],
        Some(json!(["a", "b", "c", "d"])),
        r#"{
                "items": [
                    "a",
                    "b",
                    "c",
                    "d"
                ],
                "count": 3
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                "0": "zero",
                "1": "one"
            }"#
        .unindent(),
        &["1"],
        Some(json!("ONE")),
        r#"{
                "0": "zero",
                "1": "ONE"
            }"#
        .unindent(),
    );
    // Test with comments between object members
    check_object_replace(
        r#"{
                "a": 1,
                // Comment between members
                "b": 2,
                /* Block comment */
                "c": 3
            }"#
        .unindent(),
        &["b"],
        Some(json!({"nested": true})),
        r#"{
                "a": 1,
                // Comment between members
                "b": {
                    "nested": true
                },
                /* Block comment */
                "c": 3
            }"#
        .unindent(),
    );

    // Test with trailing comments on replaced value
    check_object_replace(
        r#"{
                "a": 1, // keep this comment
                "b": 2  // this should stay
            }"#
        .unindent(),
        &["a"],
        Some(json!("changed")),
        r#"{
                "a": "changed", // keep this comment
                "b": 2  // this should stay
            }"#
        .unindent(),
    );

    // Test with deep indentation
    check_object_replace(
        r#"{
                        "deeply": {
                                "nested": {
                                        "value": "old"
                                }
                        }
                }"#
        .unindent(),
        &["deeply", "nested", "value"],
        Some(json!("new")),
        r#"{
                        "deeply": {
                                "nested": {
                                        "value": "new"
                                }
                        }
                }"#
        .unindent(),
    );

    // Test removing value with comment preservation
    check_object_replace(
        r#"{
                // Header comment
                "a": 1,
                // This comment belongs to b
                "b": 2,
                // This comment belongs to c
                "c": 3
            }"#
        .unindent(),
        &["b"],
        None,
        r#"{
                // Header comment
                "a": 1,
                // This comment belongs to b
                // This comment belongs to c
                "c": 3
            }"#
        .unindent(),
    );

    // Test with multiline block comments
    check_object_replace(
        r#"{
                /*
                 * This is a multiline
                 * block comment
                 */
                "value": "old",
                /* Another block */ "other": 123
            }"#
        .unindent(),
        &["value"],
        Some(json!("new")),
        r#"{
                /*
                 * This is a multiline
                 * block comment
                 */
                "value": "new",
                /* Another block */ "other": 123
            }"#
        .unindent(),
    );

    check_object_replace(
        r#"{
                // This object is empty
            }"#
        .unindent(),
        &["key"],
        Some(json!("value")),
        r#"{
                // This object is empty
                "key": "value"
            }
            "#
        .unindent(),
    );

    // Test replacing in object with only comments
    check_object_replace(
        r#"{
                // Comment 1
                // Comment 2
            }"#
        .unindent(),
        &["new"],
        Some(json!(42)),
        r#"{
                // Comment 1
                // Comment 2
                "new": 42
            }
            "#
        .unindent(),
    );

    // Test with inconsistent spacing
    check_object_replace(
        r#"{
              "a":1,
                    "b"  :  2  ,
                "c":   3
            }"#
        .unindent(),
        &["b"],
        Some(json!("spaced")),
        r#"{
              "a":1,
                    "b"  :  "spaced"  ,
                "c":   3
            }"#
        .unindent(),
    );
}
